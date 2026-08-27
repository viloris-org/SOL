# ADR-0034: Capability-Based Permission Model

## Status
Proposed

## Context

SOL作为Linux Family OS需要一套现代的权限模型来保护用户数据和系统资源。我们需要：

1. **最小权限原则** - 应用默认无权限，必须显式授予
2. **运行时可撤销** - 用户可随时撤销已授予的权限
3. **协议层强制** - 权限检查在compositor层，应用无法绕过
4. **透明且可审计** - 用户清楚知道每个应用拥有什么权限
5. **特殊目录保护** - Pictures/Documents等敏感目录需额外保护

## Decision

采用**Capability-based模型 + Portal中介 + 动态撤销**的三层权限架构。

### 1. 核心架构

```
Application
    ↓ (SCP request)
Compositor (capability check)
    ↓ (if granted)
Portal/Service (resource access)
    ↓ (sandboxed)
System Resource
```

### 2. 权限分层

#### Tier 0: 基础能力（自动授予）
无安全风险的基础窗口操作：
- `create_toplevel` - 创建顶层窗口
- `create_popup(parent)` - 创建popup（需parent引用）
- `surface_rendering` - 渲染到自己的surface
- `input_events` - 接收自己窗口内的输入事件

#### Tier 1: Portal中介（用户交互授权）
通过系统portal访问，用户选择即授权：
- `file_open` - 打开文件选择器
- `file_save` - 打开保存对话框
- `clipboard_read` - 读剪贴板（需前台窗口）
- `screenshot_request` - 单次截图请求

#### Tier 2: 声明式权限（manifest + 运行时）
需在manifest声明，运行时请求用户授权：
- `notifications` - 发送通知
- `network_access` - 网络访问
- `background_task` - 后台运行
- `clipboard_write` - 写剪贴板
- `camera` - 摄像头访问
- `microphone` - 麦克风访问
- `location` - 位置信息

#### Tier 3: 特权能力（系统组件专用）
仅限系统签名的应用：
- `layer_shell` - 创建shell layer（仅shell）
- `global_shortcuts` - 注册全局快捷键（shell/IME）
- `screen_record` - 屏幕录制
- `system_settings` - 修改系统设置
- `device_admin` - 设备管理

### 3. Manifest格式

```toml
# /usr/share/applications/sol-files.manifest
[app]
id = "org.sol.files"
name = "SOL Files"
version = "1.0.0"
signature = "SHA256:..."  # 开发者签名

[capabilities.static]
# 启动时自动授予（Tier 0）
create_toplevel = true

[capabilities.dynamic]
# 需运行时请求（Tier 1/2）
notifications = { justification = "通知文件操作完成" }
network_access = { 
    justification = "访问网络存储",
    optional = true  # 用户拒绝后应用仍可运行
}

[capabilities.forbidden]
# 显式禁止（防止运行时请求）
screen_record = true
layer_shell = true

[special_directories]
# 特殊目录访问策略
pictures = "read_write"  # read_only, read_write, denied
documents = "read_write"
downloads = "read_write"
music = "read_only"
videos = "read_only"
```

### 4. SCP协议扩展

```rust
// compositor/src/scp/protocol.rs

/// 能力请求
pub enum SolRequest {
    /// 请求新能力
    RequestCapability {
        capability: Capability,
        justification: String,
    },
    
    /// 使用能力执行操作
    UseCapability {
        token: CapabilityToken,
        action: CapabilityAction,
    },
}

/// 能力定义
pub enum Capability {
    // Tier 0
    CreateToplevel,
    CreatePopup { parent: SurfaceId },
    
    // Tier 1 (Portal)
    FileOpen { 
        mime_types: Vec<String>,
        multiple: bool,
    },
    FileSave { default_name: Option<String> },
    ClipboardRead,
    ScreenshotOnce,
    
    // Tier 2 (Dynamic)
    Notifications,
    NetworkAccess,
    BackgroundTask,
    ClipboardWrite,
    Camera,
    Microphone,
    
    // Tier 3 (Privileged)
    LayerShell,
    GlobalShortcuts,
    ScreenRecord,
}

/// 授权类型
pub enum GrantType {
    /// 永久授予（直到用户撤销）
    Permanent,
    /// 会话期间（应用退出即失效）
    Session,
    /// 单次使用（用完即失效）
    OneTime,
}

/// 能力token（不可伪造）
pub struct CapabilityToken {
    id: u64,
    app_id: String,
    capability: Capability,
    grant_type: GrantType,
    granted_at: Timestamp,
    expires_at: Option<Timestamp>,
}

/// Compositor响应
pub enum SolEvent {
    CapabilityGranted {
        token: CapabilityToken,
        scope: GrantType,
    },
    CapabilityDenied {
        capability: Capability,
        reason: DenialReason,
    },
    CapabilityRevoked {
        token_id: u64,
        reason: String,
    },
}

pub enum DenialReason {
    NotInManifest,
    UserDenied,
    SystemPolicy,
    MissingDependency,
}
```

### 5. 运行时撤销机制

#### Compositor状态管理
```rust
// compositor/src/state.rs

pub struct SolState {
    // 每个应用的能力集合
    app_capabilities: HashMap<AppId, AppCapabilities>,
    // 能力token索引
    capability_tokens: HashMap<TokenId, CapabilityToken>,
    // 撤销监听器
    revocation_listeners: HashMap<TokenId, Vec<RevocationCallback>>,
}

impl SolState {
    /// 撤销能力（立即生效）
    pub fn revoke_capability(&mut self, app_id: &str, capability: &Capability) {
        // 1. 从应用能力集移除
        if let Some(caps) = self.app_capabilities.get_mut(app_id) {
            caps.revoke(capability);
        }
        
        // 2. 使所有相关token失效
        let tokens_to_revoke: Vec<_> = self.capability_tokens
            .iter()
            .filter(|(_, token)| {
                token.app_id == app_id && &token.capability == capability
            })
            .map(|(id, _)| *id)
            .collect();
        
        for token_id in tokens_to_revoke {
            self.revoke_token(token_id);
        }
        
        // 3. 通知应用
        self.send_event(app_id, SolEvent::CapabilityRevoked {
            token_id: 0,  // 特殊值表示整类能力
            reason: "User revoked permission".into(),
        });
    }
    
    /// 检查能力token有效性
    pub fn validate_token(&self, token_id: TokenId) -> Result<&CapabilityToken> {
        let token = self.capability_tokens.get(&token_id)
            .ok_or(Error::InvalidToken)?;
        
        // 检查过期
        if let Some(expires) = token.expires_at {
            if Instant::now() > expires {
                return Err(Error::TokenExpired);
            }
        }
        
        // 检查是否已撤销
        if let Some(caps) = self.app_capabilities.get(&token.app_id) {
            if !caps.has(&token.capability) {
                return Err(Error::CapabilityRevoked);
            }
        }
        
        Ok(token)
    }
}
```

#### 应用端处理
```rust
// sdk/sol-app/src/capabilities.rs

pub struct CapabilityManager {
    granted: HashMap<Capability, CapabilityToken>,
    revocation_handlers: HashMap<Capability, Box<dyn Fn()>>,
}

impl CapabilityManager {
    /// 请求能力
    pub async fn request(&mut self, cap: Capability) -> Result<CapabilityToken> {
        // 发送SCP请求
        let response = self.connection.request_capability(cap.clone()).await?;
        
        match response {
            SolEvent::CapabilityGranted { token, .. } => {
                self.granted.insert(cap, token.clone());
                Ok(token)
            }
            SolEvent::CapabilityDenied { reason, .. } => {
                Err(Error::PermissionDenied(reason))
            }
        }
    }
    
    /// 监听撤销事件
    pub fn on_revoked(&mut self, cap: Capability, handler: impl Fn() + 'static) {
        self.revocation_handlers.insert(cap, Box::new(handler));
    }
    
    /// 处理compositor发来的撤销通知
    fn handle_revocation(&mut self, event: SolEvent) {
        if let SolEvent::CapabilityRevoked { token_id, .. } = event {
            // 找到对应能力
            let cap = self.granted.iter()
                .find(|(_, token)| token.id == token_id)
                .map(|(cap, _)| cap.clone());
            
            if let Some(cap) = cap {
                // 移除token
                self.granted.remove(&cap);
                
                // 调用用户处理器
                if let Some(handler) = self.revocation_handlers.get(&cap) {
                    handler();
                }
            }
        }
    }
}
```

### 6. 全局文件级授权模型

#### 核心原则：零信任文件系统访问

SOL采用**全局文件级授权模型** - 不仅特殊目录，而是**用户HOME目录下的所有位置**都需要显式授权。这是因为：

1. **用户习惯难以预测** - 敏感文件可能放在任何地方（`~/tmp/passwords.txt`, `~/old/bank.pdf`）
2. **零信任原则** - 应用默认无法访问任何用户数据
3. **Portal为主要接口** - 用户主动选择 = 授权，无需预设"安全"目录
4. **所有权为次要机制** - 应用对自己创建的文件拥有控制权

#### 文件系统访问规则（全局）
#### 文件系统访问规则（全局）

```rust
// services/sol-portal/src/filesystem.rs

/// 文件系统访问策略 - 适用于整个 $HOME
pub struct FilesystemPolicy {
    // 应用私有数据目录（完全访问，无需portal）
    app_data_dir: PathBuf,  // ~/.local/share/sol/{app_id}/
    app_cache_dir: PathBuf,  // ~/.cache/sol/{app_id}/
    app_config_dir: PathBuf, // ~/.config/sol/{app_id}/
}

/// $HOME 下的访问规则（除了应用私有目录）
pub enum HomeAccessRule {
    /// 读取现有文件 - ❌ 禁止（必须通过portal）
    ReadExistingFile,
    
    /// 写入现有文件 - ❌ 禁止（除非拥有所有权）
    WriteExistingFile,
    
    /// 删除文件 - ❌ 禁止（除非拥有所有权）
    DeleteFile,
    
    /// 列举目录 - ❌ 禁止（除非拥有所有权）
    ListDirectory,
    
    /// 创建新文件 - ⚠️ 仅在允许的位置（见下方）
    CreateNewFile,
    
    /// 创建新目录 - ⚠️ 仅在允许的位置（见下方）
    CreateNewDirectory,
}

/// 允许应用创建文件/目录的位置
pub enum CreationAllowedLocation {
    /// 应用私有目录（完全访问，无限制）
    AppPrivateDir {
        data: PathBuf,    // ~/.local/share/sol/{app_id}/
        cache: PathBuf,   // ~/.cache/sol/{app_id}/
        config: PathBuf,  // ~/.config/sol/{app_id}/
    },
    
    /// XDG用户目录（顶层创建，获得所有权）
    XdgUserDirs {
        // 所有这些目录都允许在顶层创建新文件/目录
        documents: PathBuf,   // ~/Documents/
        downloads: PathBuf,   // ~/Downloads/
        pictures: PathBuf,    // ~/Pictures/
        videos: PathBuf,      // ~/Videos/
        music: PathBuf,       // ~/Music/
        desktop: PathBuf,     // ~/Desktop/
    },
    
    /// 用户显式授权的目录（通过 portal.create_folder_access）
    PortalGrantedDir {
        path: PathBuf,
        granted_at: Timestamp,
    },
}

impl FilesystemPolicy {
    /// 检查路径访问权限
    pub fn check_access(
        &self,
        app_id: &str,
        path: &Path,
        operation: FilesystemOperation,
    ) -> Result<AccessDecision> {
        // 1. 应用私有目录 - 完全访问
        if path.starts_with(&self.app_data_dir) 
            || path.starts_with(&self.app_cache_dir)
            || path.starts_with(&self.app_config_dir) {
            return Ok(AccessDecision::Allow);
        }
        
        // 2. 系统只读资源 - 允许读取
        if path.starts_with("/usr/share/sol-design")
            || path.starts_with("/usr/share/fonts")
            || path.starts_with("/usr/lib/sol-runtime") {
            return match operation {
                FilesystemOperation::Read => Ok(AccessDecision::Allow),
                _ => Err(Error::PermissionDenied),
            };
        }
        
        // 3. $HOME 下的其他位置
        if path.starts_with(env::var("HOME")?) {
            return self.check_home_access(app_id, path, operation);
        }
        
        // 4. $HOME 外的位置 - 完全禁止
        Err(Error::PermissionDenied)
    }
    
    /// 检查 $HOME 内的访问
    fn check_home_access(
        &self,
        app_id: &str,
        path: &Path,
        operation: FilesystemOperation,
    ) -> Result<AccessDecision> {
        match operation {
            // 读/写/删除现有文件 - 检查所有权或portal授权
            FilesystemOperation::Read 
            | FilesystemOperation::Write 
            | FilesystemOperation::Delete => {
                let ownership = OWNERSHIP_DB.get_ownership(path)?;
                
                match ownership {
                    Some(Ownership::OwnedByApp { app_id: owner, .. }) if owner == app_id => {
                        Ok(AccessDecision::Allow)
                    }
                    Some(Ownership::GrantedByUser { handle_id, .. }) => {
                        // 通过portal授权的，检查handle是否仍有效
                        Ok(AccessDecision::AllowWithHandle(handle_id))
                    }
                    _ => Err(Error::MustUsePortal),
                }
            }
            
            // 列举目录 - 必须拥有该目录
            FilesystemOperation::ListDir => {
                let ownership = OWNERSHIP_DB.get_ownership(path)?;
                
                match ownership {
                    Some(Ownership::OwnedByApp { app_id: owner, .. }) if owner == app_id => {
                        Ok(AccessDecision::Allow)
                    }
                    _ => Err(Error::MustUsePortal),
                }
            }
            
            // 创建新文件/目录 - 检查是否在允许的位置
            FilesystemOperation::CreateFile | FilesystemOperation::CreateDir => {
                self.check_creation_allowed(app_id, path, operation)
            }
        }
    }
    
    /// 检查是否允许在此位置创建
    fn check_creation_allowed(
        &self,
        app_id: &str,
        path: &Path,
        operation: FilesystemOperation,
    ) -> Result<AccessDecision> {
        let user_dirs = xdg::UserDirs::new()?;
        
        // 1. 在XDG用户目录的顶层创建 - 允许
        let allowed_parents = vec![
            user_dirs.documents(),
            user_dirs.downloads(),
            user_dirs.pictures(),
            user_dirs.videos(),
            user_dirs.music(),
            user_dirs.desktop(),
        ];
        
        if let Some(parent) = path.parent() {
            if allowed_parents.iter().any(|p| Some(parent) == *p) {
                // 在顶层创建，必须是新的
                if path.exists() {
                    return Err(Error::AlreadyExists);
                }
                
                // 注册所有权
                OWNERSHIP_DB.register_ownership(
                    path,
                    Ownership::OwnedByApp {
                        app_id: app_id.into(),
                        created_at: Instant::now(),
                    }
                );
                
                return Ok(AccessDecision::Allow);
            }
        }
        
        // 2. 在自己拥有的目录下创建 - 允许
        if let Some(parent) = path.parent() {
            if let Some(ownership) = OWNERSHIP_DB.get_ownership(parent)? {
                if let Ownership::OwnedByApp { app_id: owner, .. } = ownership {
                    if owner == app_id {
                        // 父目录是自己的，允许创建
                        OWNERSHIP_DB.register_ownership(
                            path,
                            Ownership::OwnedByApp {
                                app_id: app_id.into(),
                                created_at: Instant::now(),
                            }
                        );
                        return Ok(AccessDecision::Allow);
                    }
                }
            }
        }
        
        // 3. 在portal授权的目录下创建 - 检查授权范围
        // (通过 portal.request_folder_access() 授予)
        if let Some(granted) = OWNERSHIP_DB.get_portal_grant(app_id, path)? {
            if granted.allows_creation {
                OWNERSHIP_DB.register_ownership(
                    path,
                    Ownership::OwnedByApp {
                        app_id: app_id.into(),
                        created_at: Instant::now(),
                    }
                );
                return Ok(AccessDecision::Allow);
            }
        }
        
        // 4. 其他位置 - 拒绝
        Err(Error::CreationNotAllowed)
    }
}

pub enum FilesystemOperation {
    Read,
    Write,
    Delete,
    ListDir,
    CreateFile,
    CreateDir,
}

pub enum AccessDecision {
    Allow,
    AllowWithHandle(u64),
}
```

#### 应用视角的文件系统

```rust
// sdk/sol-system/src/filesystem.rs

/// 应用可见的文件系统API
pub struct Filesystem {
    connection: PortalConnection,
}

impl Filesystem {
    /// 读取文件 - 必须通过portal或已拥有所有权
    pub async fn read_file(&self, path: &Path) -> Result<Vec<u8>> {
        // 尝试直接访问（如果有所有权）
        match self.connection.request_file_access(path, FileMode::Read).await {
            Ok(handle) => {
                // 通过handle读取
                Ok(handle.read_all()?)
            }
            Err(Error::MustUsePortal) => {
                // 必须通过picker
                Err(Error::MustUsePortal)
            }
            Err(e) => Err(e),
        }
    }
    
    /// 打开文件选择器（推荐的读取现有文件方式）
    pub async fn pick_file(&self, options: FilePickerOptions) -> Result<FileHandle> {
        self.connection.show_file_picker(options).await
    }
    
    /// 在标准位置创建新文件
    pub async fn create_in_documents(&self, filename: &str) -> Result<File> {
        self.connection.create_file_in_user_dir(UserDir::Documents, filename).await
    }
    
    pub async fn create_in_downloads(&self, filename: &str) -> Result<File> {
        self.connection.create_file_in_user_dir(UserDir::Downloads, filename).await
    }
    
    pub async fn create_in_pictures(&self, filename: &str) -> Result<File> {
        self.connection.create_file_in_user_dir(UserDir::Pictures, filename).await
    }
    
    /// 在应用私有目录操作（无限制）
    pub fn app_data_dir(&self) -> &Path {
        &self.app_data_dir  // ~/.local/share/sol/{app_id}/
    }
    
    pub fn app_cache_dir(&self) -> &Path {
        &self.app_cache_dir  // ~/.cache/sol/{app_id}/
    }
    
    /// 创建应用专属文件夹（获得完全所有权）
    pub async fn create_app_folder_in_documents(&self, folder_name: &str) -> Result<PathBuf> {
        // 在 ~/Documents/ 下创建文件夹
        // 例如: ~/Documents/MyAppProjects/
        self.connection.create_directory_in_user_dir(
            UserDir::Documents, 
            folder_name
        ).await
    }
    
    /// 请求访问特定目录（高级用例）
    /// 用户会看到确认对话框
    pub async fn request_folder_access(&self, purpose: &str) -> Result<FolderHandle> {
        // 显示目录选择器 + 权限说明
        // 用户选择后，应用获得该目录的创建权限
        self.connection.request_folder_access(purpose).await
    }
}
```

#### 实际场景示例

```rust
// 照片编辑器应用
pub struct PhotoEditor {
    fs: Filesystem,
}

impl PhotoEditor {
    /// 场景1: 打开用户的照片
    pub async fn open_photo(&self) -> Result<Image> {
        // ✅ 正确：通过portal
        let handle = self.fs.pick_file(FilePickerOptions {
            mime_types: vec!["image/*"],
            title: "选择照片",
        }).await?;
        
        let data = handle.read_all()?;
        Ok(Image::from_bytes(data))
        
        // ❌ 错误：尝试直接读取
        // self.fs.read_file(Path::new("~/Pictures/vacation.jpg"))  // Error::MustUsePortal
    }
    
    /// 场景2: 导出编辑后的照片
    pub async fn export_photo(&self, image: &Image) -> Result<()> {
        // ✅ 方式1: 在 Pictures 顶层创建新文件
        let filename = format!("edited_{}.jpg", timestamp());
        let mut file = self.fs.create_in_pictures(&filename).await?;
        file.write_all(&image.to_jpeg())?;
        
        // ✅ 方式2: 在自己的目录下创建
        let app_folder = self.fs.create_app_folder_in_pictures("PhotoEditor").await?;
        // 现在拥有 ~/Pictures/PhotoEditor/ 的完全控制权
        let export_path = app_folder.join("export_001.jpg");
        std::fs::write(&export_path, image.to_jpeg())?;  // 直接写入，无需portal
        
        Ok(())
    }
    
    /// 场景3: 列举自己导出的照片
    pub async fn list_exports(&self) -> Result<Vec<PathBuf>> {
        // ✅ 可以列举自己的目录
        let app_folder = Path::new(&format!(
            "{}/Pictures/PhotoEditor",
            env::var("HOME")?
        ));
        
        let entries = std::fs::read_dir(app_folder)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .collect();
        
        Ok(entries)
        
        // ❌ 错误：无法列举整个 Pictures
        // std::fs::read_dir("~/Pictures")  // Error::MustUsePortal
    }
    
    /// 场景4: 项目管理（高级用例）
    pub async fn create_project(&self, name: &str) -> Result<Project> {
        // 请求访问用户选择的项目目录
        let folder = self.fs.request_folder_access(
            "PhotoEditor需要一个目录来存储项目文件"
        ).await?;
        
        // 用户选择了 ~/MyProjects/，应用获得在其中创建的权限
        let project_dir = folder.create_subdirectory(name)?;
        
        // 现在拥有 ~/MyProjects/{name}/ 的完全控制权
        project_dir.create_file("metadata.json")?;
        project_dir.create_subdirectory("assets")?;
        
        Ok(Project { root: project_dir })
    }
}

// 下载管理器应用
pub struct DownloadManager {
    fs: Filesystem,
}

impl DownloadManager {
    /// 下载文件到 Downloads
    pub async fn download(&self, url: &str) -> Result<()> {
        let filename = extract_filename(url);
        
        // ✅ 直接在 Downloads 创建新文件
        let mut file = self.fs.create_in_downloads(&filename).await?;
        
        let content = http_get(url).await?;
        file.write_all(&content)?;
        
        Ok(())
        
        // ❌ 错误：无法覆盖已存在的文件（除非拥有所有权）
        // std::fs::write("~/Downloads/existing.zip", data)  // Error
    }
    
    /// 列举自己下载的文件
    pub async fn list_my_downloads(&self) -> Result<Vec<PathBuf>> {
        // 只能看到自己创建的文件
        OWNERSHIP_DB.list_app_files(&self.app_id)
            .into_iter()
            .filter(|p| p.starts_with(&downloads_dir()))
            .collect()
    }
}
```
```

#### Portal实现 - 文件级授权
```rust
// services/sol-portal/src/file.rs

pub struct FilePortal {
    policies: HashMap<SpecialDirectory, SpecialDirPolicy>,
    // 活跃的文件句柄（portal授权的）
    active_handles: HashMap<FileHandle, FileAccess>,
    // 应用所有权数据库
    ownership_db: OwnershipDatabase,
}

pub struct FileAccess {
    app_id: String,
    path: PathBuf,
    permissions: FilePermissions,
    granted_at: Instant,
    ownership: Ownership,
}

impl FilePortal {
    /// 打开文件（通过picker - 唯一合法的读取现有文件方式）
    pub async fn open_file(&mut self, 
        app_id: &str,
        request: OpenFileRequest
    ) -> Result<FileHandle> {
        // 1. 显示系统文件选择器（用户主动授权）
        let selected = self.show_picker(request).await?;
        
        // 2. 检查是否在特殊目录
        let ownership = if let Some(dir) = self.check_special_directory(&selected.path) {
            // 在特殊目录内，检查所有权
            self.ownership_db.get_ownership(&selected.path)
                .unwrap_or(Ownership::External)
        } else {
            // 非特殊目录，允许访问
            Ownership::External  // 但通过portal授权了
        };
        
        // 3. 创建受限文件句柄
        let permissions = FilePermissions {
            read: true,
            write: request.mode.is_write(),
            delete: false,  // portal授权的文件不能删除（除非拥有所有权）
        };
        
        let handle = self.create_handle(
            app_id, 
            selected.path, 
            permissions,
            Ownership::GrantedByUser {
                handle_id: self.next_handle_id(),
                permissions,
                granted_at: Instant::now(),
            }
        )?;
        
        Ok(handle)
    }
    
    /// 保存文件（创建新文件或写入已有文件）
    pub async fn save_file(&mut self,
        app_id: &str,
        request: SaveFileRequest
    ) -> Result<FileHandle> {
        // 1. 显示保存对话框
        let selected = self.show_save_dialog(request).await?;
        
        // 2. 检查是否在特殊目录
        if let Some(dir) = self.check_special_directory(&selected.path) {
            if selected.path.exists() {
                // 写入现有文件 - 必须拥有所有权
                let ownership = self.ownership_db.get_ownership(&selected.path)
                    .ok_or(Error::PermissionDenied)?;
                
                match ownership {
                    Ownership::OwnedByApp { app_id: owner, .. } if owner == app_id => {
                        // OK: 自己的文件
                    }
                    _ => {
                        return Err(Error::NotOwned);
                    }
                }
            } else {
                // 创建新文件 - 自动获得所有权
                self.ownership_db.register_ownership(
                    &selected.path,
                    Ownership::OwnedByApp {
                        app_id: app_id.into(),
                        created_at: Instant::now(),
                    }
                );
            }
        }
        
        // 3. 创建写入句柄
        let permissions = FilePermissions {
            read: true,
            write: true,
            delete: true,  // 自己创建的文件可以删除
        };
        
        let handle = self.create_handle(app_id, selected.path, permissions, 
            Ownership::OwnedByApp {
                app_id: app_id.into(),
                created_at: Instant::now(),
            }
        )?;
        
        Ok(handle)
    }
    
    /// 在特殊目录创建新文件（直接API，不通过picker）
    pub fn create_file_in_special_dir(&mut self,
        app_id: &str,
        dir: SpecialDirectory,
        filename: &str,
    ) -> Result<FileHandle> {
        // 1. 解析目标路径
        let base_path = self.resolve_special_dir(dir)?;
        let target_path = base_path.join(filename);
        
        // 2. 确保文件不存在
        if target_path.exists() {
            return Err(Error::AlreadyExists);
        }
        
        // 3. 确保在目录直接子级（不能创建子目录中的文件）
        if filename.contains('/') || filename.contains('\\') {
            return Err(Error::InvalidPath);
        }
        
        // 4. 创建文件并获得所有权
        let file = File::create(&target_path)?;
        
        self.ownership_db.register_ownership(
            &target_path,
            Ownership::OwnedByApp {
                app_id: app_id.into(),
                created_at: Instant::now(),
            }
        );
        
        // 5. 返回完全访问的句柄
        let permissions = FilePermissions {
            read: true,
            write: true,
            delete: true,
        };
        
        let handle = self.create_handle_from_file(app_id, file, target_path, permissions)?;
        
        Ok(handle)
    }
    
    /// 在特殊目录创建新目录（直接API）
    pub fn create_directory_in_special_dir(&mut self,
        app_id: &str,
        dir: SpecialDirectory,
        dirname: &str,
    ) -> Result<PathBuf> {
        // 1. 解析目标路径
        let base_path = self.resolve_special_dir(dir)?;
        let target_path = base_path.join(dirname);
        
        // 2. 必须不存在
        if target_path.exists() {
            return Err(Error::AlreadyExists);
        }
        
        // 3. 确保在目录直接子级
        if dirname.contains('/') || dirname.contains('\\') {
            return Err(Error::InvalidPath);
        }
        
        // 4. 创建目录
        std::fs::create_dir(&target_path)?;
        
        // 5. 注册目录所有权
        self.ownership_db.register_ownership(
            &target_path,
            Ownership::OwnedByApp {
                app_id: app_id.into(),
                created_at: Instant::now(),
            }
        );
        
        log::info!("{} created directory {} with full ownership", 
            app_id, target_path.display());
        
        Ok(target_path)
    }
    
    /// 列举目录内容（仅限拥有所有权的目录）
    pub fn list_directory(&self,
        app_id: &str,
        path: &Path,
    ) -> Result<Vec<DirEntry>> {
        // 1. 检查是否在特殊目录
        if self.check_special_directory(path).is_some() {
            // 2. 必须拥有该目录
            let ownership = self.ownership_db.get_ownership(path)
                .ok_or(Error::PermissionDenied)?;
            
            match ownership {
                Ownership::OwnedByApp { app_id: owner, .. } if owner == app_id => {
                    // OK: 自己的目录
                }
                _ => {
                    return Err(Error::NotOwned);
                }
            }
        }
        
        // 3. 列举内容
        let entries: Result<Vec<_>> = std::fs::read_dir(path)?
            .map(|e| e.map(|e| DirEntry {
                name: e.file_name().to_string_lossy().into(),
                is_dir: e.file_type()?.is_dir(),
                size: e.metadata()?.len(),
            }))
            .collect();
        
        entries
    }
    
    /// 删除文件（仅限拥有所有权）
    pub fn delete_file(&mut self,
        app_id: &str,
        path: &Path,
    ) -> Result<()> {
        // 1. 检查所有权
        let ownership = self.ownership_db.get_ownership(path)
            .ok_or(Error::PermissionDenied)?;
        
        match ownership {
            Ownership::OwnedByApp { app_id: owner, .. } if owner == app_id => {
                // OK: 自己的文件
            }
            _ => {
                return Err(Error::NotOwned);
            }
        }
        
        // 2. 删除文件
        std::fs::remove_file(path)?;
        
        // 3. 清除所有权记录
        self.ownership_db.remove_ownership(path);
        
        Ok(())
    }
    
    /// 删除目录（仅限拥有所有权的空目录）
    pub fn delete_directory(&mut self,
        app_id: &str,
        path: &Path,
    ) -> Result<()> {
        // 1. 检查所有权
        let ownership = self.ownership_db.get_ownership(path)
            .ok_or(Error::PermissionDenied)?;
        
        match ownership {
            Ownership::OwnedByApp { app_id: owner, .. } if owner == app_id => {
                // OK: 自己的目录
            }
            _ => {
                return Err(Error::NotOwned);
            }
        }
        
        // 2. 确保目录为空或只包含自己拥有的内容
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let entry_ownership = self.ownership_db.get_ownership(&entry.path());
            
            match entry_ownership {
                Some(Ownership::OwnedByApp { app_id: owner, .. }) if owner == app_id => {
                    // OK: 自己的内容
                }
                _ => {
                    return Err(Error::DirectoryNotEmpty);
                }
            }
        }
        
        // 3. 递归删除
        std::fs::remove_dir_all(path)?;
        
        // 4. 清除所有权记录（包括子内容）
        self.ownership_db.remove_ownership_recursive(path);
        
        Ok(())
    }
    
    /// 检查路径是否在特殊目录
    fn check_special_directory(&self, path: &Path) -> Option<SpecialDirectory> {
        let user_dirs = xdg::UserDirs::new()?;
        
        for (dir, _) in &self.policies {
            let dir_path = match dir {
                SpecialDirectory::Pictures => user_dirs.pictures()?,
                SpecialDirectory::Documents => user_dirs.documents()?,
                SpecialDirectory::Downloads => user_dirs.downloads()?,
                SpecialDirectory::Music => user_dirs.music()?,
                SpecialDirectory::Videos => user_dirs.videos()?,
                SpecialDirectory::Desktop => user_dirs.desktop()?,
                _ => continue,
            };
            
            if path.starts_with(dir_path) {
                return Some(*dir);
            }
        }
        
        None
    }
    
    /// 创建受限文件句柄
    fn create_handle(&mut self, 
        app_id: &str,
        path: PathBuf,
        permissions: FilePermissions,
        ownership: Ownership,
    ) -> Result<FileHandle> {
        // 使用O_PATH + Landlock确保应用无法逃逸
        let fd = self.open_restricted(&path, &permissions)?;
        
        let access = FileAccess {
            app_id: app_id.into(),
            path: path.clone(),
            permissions,
            granted_at: Instant::now(),
            ownership,
        };
        
        let handle = FileHandle::new(fd);
        self.active_handles.insert(handle, access);
        
        Ok(handle)
    }
    
    /// 撤销文件访问
    pub fn revoke_access(&mut self, app_id: &str, handle: FileHandle) {
        if let Some(access) = self.active_handles.remove(&handle) {
            if access.app_id == app_id {
                drop(handle);
                log::info!("Revoked file access for {}: {}", app_id, access.path.display());
            }
        }
    }
    
    /// 撤销应用的所有文件访问（但保留所有权）
    pub fn revoke_all_access(&mut self, app_id: &str) {
        self.active_handles.retain(|_, access| {
            if access.app_id == app_id {
                log::info!("Revoking access to {}", access.path.display());
                false
            } else {
                true
            }
        });
        
        // 注意：不删除ownership记录，应用创建的文件仍归其所有
    }
}

/// 所有权数据库
pub struct OwnershipDatabase {
    // path -> ownership mapping
    // 持久化到 ~/.local/share/sol-portal/ownership.db (sqlite)
    db: Connection,
}

impl OwnershipDatabase {
    pub fn register_ownership(&mut self, path: &Path, ownership: Ownership) {
        // INSERT INTO ownership (path, app_id, created_at) VALUES (?, ?, ?)
    }
    
    pub fn get_ownership(&self, path: &Path) -> Option<Ownership> {
        // SELECT * FROM ownership WHERE path = ?
    }
    
    pub fn remove_ownership(&mut self, path: &Path) {
        // DELETE FROM ownership WHERE path = ?
    }
    
    pub fn remove_ownership_recursive(&mut self, path: &Path) {
        // DELETE FROM ownership WHERE path LIKE 'path/%'
    }
    
    pub fn list_app_files(&self, app_id: &str) -> Vec<PathBuf> {
        // SELECT path FROM ownership WHERE app_id = ?
    }
}
```

#### Manifest中的特殊目录声明
```toml
[special_directories]
# 声明需要的目录访问级别
pictures = "read_write"  # 可通过portal访问Pictures
documents = "read_only"  # 只读访问Documents
downloads = "read_write"

# 细粒度子目录
"pictures/screenshots" = "read_only"  # 只能读取截图
"documents/projects" = "denied"       # 明确禁止

[special_directories.confirmation]
# 这些操作需要每次确认
pictures_write = true
documents_delete = true
```

### 7. 沙盒强制

#### Landlock LSM
```rust
// services/sol-runtime/src/sandbox.rs

use landlock::{Access, AccessFs, Ruleset, RulesetAttr, ABI};

pub struct FilesystemSandbox {
    manifest: AppManifest,
}

impl FilesystemSandbox {
    pub fn apply(&self) -> Result<()> {
        let abi = ABI::V4;
        
        let mut ruleset = Ruleset::default()
            .handle_access(AccessFs::from_all(abi))?
            .create()?;
        
        // 1. 只读访问：runtime + system resources
        ruleset = ruleset
            .add_rule(PathBeneath::new("/usr/lib/sol-runtime", AccessFs::ReadFile))?
            .add_rule(PathBeneath::new("/usr/share/sol-design", AccessFs::ReadFile))?
            .add_rule(PathBeneath::new("/usr/share/fonts", AccessFs::ReadFile))?;
        
        // 2. 应用私有数据目录（完全访问）
        let app_data = format!("/home/{}/.local/share/sol/{}", 
            std::env::var("USER")?, 
            self.manifest.app.id
        );
        ruleset = ruleset.add_rule(
            PathBeneath::new(&app_data, AccessFs::from_all(abi))?
        )?;
        
        // 3. 特殊目录（根据manifest）
        for (dir, level) in &self.manifest.special_directories {
            let path = self.resolve_special_dir(dir)?;
            let access = match level.as_str() {
                "read_only" => AccessFs::ReadFile | AccessFs::ReadDir,
                "read_write" => AccessFs::from_read(abi) | AccessFs::from_write(abi),
                "denied" => continue,
                _ => return Err(Error::InvalidManifest),
            };
            
            ruleset = ruleset.add_rule(PathBeneath::new(path, access))?;
        }
        
        // 4. 应用规则集（无法绕过）
        ruleset.restrict_self()?;
        
        Ok(())
    }
}
```

### 8. 权限UI

#### 系统设置中的权限管理
```rust
// apps/sol-settings/src/permissions.rs

pub struct PermissionManager {
    compositor: CompositorConnection,
    apps: Vec<InstalledApp>,
}

impl PermissionManager {
    /// 获取应用当前权限
    pub fn get_app_permissions(&self, app_id: &str) -> Vec<GrantedCapability> {
        self.compositor.query_capabilities(app_id)
    }
    
    /// 撤销权限（立即生效）
    pub fn revoke_permission(&mut self, app_id: &str, cap: Capability) -> Result<()> {
        // 1. 通知compositor撤销
        self.compositor.revoke_capability(app_id, cap)?;
        
        // 2. 更新本地状态
        // 3. 显示toast通知用户
        
        Ok(())
    }
    
    /// 查看权限使用历史
    pub fn get_permission_log(&self, app_id: &str) -> Vec<PermissionEvent> {
        // 从audit log读取
        self.compositor.query_audit_log(app_id)
    }
}
```

#### 运行时权限请求对话框
```
┌─────────────────────────────────────┐
│  SOL Files 请求访问                   │
├─────────────────────────────────────┤
│                                     │
│  📷 摄像头                            │
│  "扫描文档到PDF"                      │
│                                     │
│  ⚠ 此权限允许应用：                   │
│    • 访问摄像头画面                   │
│    • 在后台拍照                       │
│                                     │
│  [ ] 本次允许（关闭后失效）            │
│  [ ] 使用期间允许（推荐）              │
│  [ ] 始终允许                         │
│                                     │
│       [拒绝]        [允许]            │
└─────────────────────────────────────┘
```

### 9. 审计日志

```rust
// compositor/src/audit.rs

pub struct AuditLog {
    events: VecDeque<AuditEvent>,
    max_size: usize,
}

pub struct AuditEvent {
    timestamp: Timestamp,
    app_id: String,
    event_type: AuditEventType,
    success: bool,
    details: String,
}

pub enum AuditEventType {
    CapabilityRequested(Capability),
    CapabilityGranted(Capability, GrantType),
    CapabilityDenied(Capability, DenialReason),
    CapabilityRevoked(Capability),
    CapabilityUsed(Capability),
    
    FileAccessed(PathBuf, OpenMode),
    FilePortalUsed(SpecialDirectory),
    
    NetworkConnection(SocketAddr),
    DeviceAccessed(DeviceType),
}

impl AuditLog {
    pub fn record(&mut self, event: AuditEvent) {
        self.events.push_back(event);
        
        // 保持大小限制
        while self.events.len() > self.max_size {
            self.events.pop_front();
        }
        
        // 异步写入持久化存储
        self.persist_async(&event);
    }
}
```

## Consequences

### 优势
1. **强安全性** - 协议层+内核层双重强制，应用无法绕过
2. **用户控制** - 随时可见、可撤销权限
3. **透明度高** - 审计日志记录所有敏感操作
4. **开发友好** - 清晰的manifest + runtime API
5. **特殊目录保护** - Pictures等敏感目录有额外防护

### 权衡
1. **Portal开销** - 文件访问需经过中介，略有性能损失
2. **兼容性** - 现有Linux应用需改造才能适配
3. **开发复杂度** - 应用需正确处理权限被拒/撤销的情况

### 与其他系统对比

| 特性 | SOL | Android | Flatpak | macOS |
|------|-----|---------|---------|-------|
| 基础模型 | Capability | Permission Groups | Portal | Entitlements |
| 协议强制 | SCP | Binder | D-Bus Portal | XPC |
| 内核强制 | Landlock | SELinux | bubblewrap | Sandbox.kext |
| 运行时撤销 | ✅ 即时 | ✅ 即时 | ❌ 需重启应用 | ⚠️ 部分支持 |
| 特殊目录 | ✅ 细粒度 | ✅ Scoped Storage | ✅ Portal | ✅ TCC |
| 审计日志 | ✅ | ✅ | ❌ | ✅ |

## Implementation Plan

### Phase 0 (Foundation)
- [ ] 定义`Capability`枚举和SCP协议扩展
- [ ] 在`SolState`中实现capability tracking
- [ ] 实现基础的token验证机制

### Phase 1 (Core)
- [ ] 实现manifest parser
- [ ] 添加Landlock沙盒支持
- [ ] 实现FilePortal with special directory protection
- [ ] 实现运行时撤销机制

### Phase 2 (Polish)
- [ ] 权限管理UI (sol-settings)
- [ ] 运行时权限请求对话框
- [ ] 审计日志系统
- [ ] 开发者文档和示例

## References
- [Android Permissions](https://developer.android.com/guide/topics/permissions/overview)
- [Flatpak Portals](https://docs.flatpak.org/en/latest/portal-api-reference.html)
- [Landlock LSM](https://landlock.io/)
- [XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html)
