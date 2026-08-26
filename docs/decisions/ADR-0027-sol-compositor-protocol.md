# ADR-0027: SOL Compositor Protocol (SCP)

**Status:** Accepted  
**Date:** 2026-08-26  
**Authors:** SOL Team

## Context

Wayland 已完成过渡阶段使命。现在需要设计 SOL 原生的 compositor 协议，追求权限最小化、能力导向的安全模型。

### Wayland 的权限问题

1. **全局能力暴露**：所有协议扩展对所有客户端可见
2. **隐式权限**：屏幕截图、输入监听等敏感能力无需显式授权
3. **缺乏细粒度控制**：无法按应用身份限制协议访问
4. **client-side decoration 困境**：客户端可绘制假冒的系统 UI
5. **输入注入**：没有防止输入合成的机制
6. **surface 层级混乱**：客户端可声明任意 layer-shell 层级

## Decision

设计 **SOL Compositor Protocol (SCP)**，基于能力模型的最小权限协议。

## Architecture

### 1. 核心原则

**Capability-based surface management**：每个能力都需要显式授权

```
App 声明需求 → sol-securityd 评估 → 授权 token → Compositor 验证 → 激活能力
```

### 2. 协议分层

```
┌─────────────────────────────────────┐
│  App (通过 sol-runtime ABI)         │
└─────────────────────────────────────┘
              ↓
┌─────────────────────────────────────┐
│  SCP Core (必需协议)                 │
│  - surface 创建/销毁                 │
│  - 缓冲区提交                        │
│  - 基本输入事件                      │
└─────────────────────────────────────┘
              ↓
┌─────────────────────────────────────┐
│  Capability Extensions (需授权)      │
│  - Window management                │
│  - Screen capture                   │
│  - Global shortcuts                 │
│  - Clipboard access                 │
│  - Drag-and-drop                    │
└─────────────────────────────────────┘
```

### 3. 核心对象模型

#### `scp_display`
- 连接建立，身份验证
- 能力协商（客户端声明，compositor 验证）
- 全局事件（输出变化、主题切换）

#### `scp_surface`
最小化 surface：
```rust
pub struct ScpSurface {
    id: SurfaceId,
    app_id: AuthenticatedAppId,  // 由 sol-securityd 提供
    buffer: Option<Buffer>,
    input_region: Option<Region>,
    opaque_region: Option<Region>,
    // 不包含：position、size、layer、parent（这些由能力扩展提供）
}
```

#### `scp_toplevel` (需要 `window:toplevel` 能力)
```rust
pub struct ScpToplevel {
    surface: ScpSurface,
    title: String,  // 由 compositor 渲染，客户端无法绘制
    app_icon: AppId,  // 由 app bundle 提供，不可伪造
    states: ToplevelStates,  // activated, maximized, fullscreen
    // compositor 控制：position, decoration, close button
}
```

### 4. 能力扩展设计

#### 4.1 Window Management (`capability:window`)

**必需授权**：所有非 Shell 应用默认拥有

##### 4.1.1 Toplevel 窗口

```rust
// 客户端请求
message CreateToplevel {
    surface_id: SurfaceId,
    capability_token: Token,  // 由 sol-securityd 签发
    title: String,
    app_id: String,  // 用于分组和图标查找
}

// Compositor 响应
message ToplevelConfigured {
    toplevel_id: ToplevelId,
    size: (i32, i32),           // compositor 强制的尺寸
    decoration_height: i32,      // 标题栏高度（compositor 绘制）
    states: ToplevelStates,     // bitflags: activated, maximized, fullscreen
}

// 客户端请求状态变化
message SetToplevelState {
    toplevel_id: ToplevelId,
    requested_state: ToplevelState {
        Maximized,
        Minimized,
        Fullscreen { output_id: Option<OutputId> },  // 需要 capability:fullscreen
    }
}

// 客户端更新元数据
message SetToplevelTitle {
    toplevel_id: ToplevelId,
    title: String,
}

message SetToplevelAppId {
    toplevel_id: ToplevelId,
    app_id: String,
}
```

**关键约束**：
- 客户端**不能**绘制标题栏（防止钓鱼）
- 客户端**不能**设置窗口位置（由 WM 决定）
- 客户端**不能**绕过 compositor 的 close 按钮
- Fullscreen 需要额外的 `capability:fullscreen`

##### 4.1.2 Popup 窗口

```rust
// 客户端请求
message CreatePopup {
    surface_id: SurfaceId,
    parent_id: SurfaceId,      // 必须有父窗口
    positioner: PopupPositioner {
        anchor_rect: Rect,     // 父 surface 上的锚点区域
        anchor_edge: Edge,     // Top | Bottom | Left | Right
        gravity: Gravity,      // 朝哪个方向展开
        constraint: Constraint, // FlipX | FlipY | SlideX | SlideY | ResizeX | ResizeY
        offset: (i32, i32),
        size: (i32, i32),
    },
    grab: bool,                // 是否抓取输入（右键菜单需要）
}

// Compositor 响应
message PopupConfigured {
    popup_id: PopupId,
    position: (i32, i32),      // 相对父窗口的最终位置
    size: (i32, i32),
}

// Compositor 通知关闭（点击外部时）
message PopupDismissed {
    popup_id: PopupId,
    reason: DismissReason {
        OutsideClick,
        ParentClosed,
        EscapeKey,
    }
}
```

**Popup 语义**：
- 必须有父 surface（toplevel 或其他 popup）
- 支持嵌套 popup（菜单的子菜单）
- `grab=true` 时，点击外部自动关闭
- 父窗口关闭时，所有子 popup 自动销毁
- 不能超出输出边界（compositor 自动调整位置）

#### 4.2 Screen Capture (`capability:capture`)

**需显式授权**，每次捕获都需要用户确认：

```rust
message RequestCapture {
    app_id: AppId,
    scope: CaptureScope {
        SingleWindow(window_id),  // 需要窗口所有者同意
        Output(output_id),        // 需要用户确认
        Workspace,                // 需要用户确认
    },
    cursor_mode: CursorMode,      // Include | Exclude
}

// Compositor 显示系统对话框 → 用户确认 → 签发一次性 token
message CaptureGranted {
    capture_id: CaptureId,
    token: OneTimeToken,          // 单次有效
    buffer_format: Format,
}
```

**防止滥用**：
- 每次捕获都显示 Shell 原生提示（红色边框 + 倒计时）
- Token 仅单次有效，下次捕获需重新授权
- 后台应用无法捕获屏幕

#### 4.3 Global Shortcuts (`capability:shortcuts`)

**需显式声明**，避免快捷键劫持：

```rust
message RegisterShortcut {
    app_id: AppId,
    binding: KeyBinding,
    justification: String,  // 向用户解释为何需要此快捷键
}

// Compositor 检查冲突 → Shell 显示授权对话框
message ShortcutGranted {
    binding: KeyBinding,
    priority: Priority,  // System > Shell > App
}
```

**冲突处理**：
- 系统快捷键（Super+Q, Super+L）不可覆盖
- Shell 快捷键优先级高于应用
- 应用间冲突由用户仲裁

#### 4.4 Clipboard (`capability:clipboard`)

**差异化授权**：

```rust
pub enum ClipboardAccess {
    Read,   // 需要前台焦点
    Write,  // 需要用户交互（按键/点击后的 500ms 内）
}

message ClipboardRequest {
    access: ClipboardAccess,
    mime_types: Vec<String>,
}
```

**安全策略**：
- 后台应用**无法读取**剪贴板（防止密码窃取）
- 写入需要在用户交互后 500ms 内（防止静默污染）
- 敏感数据（密码管理器）通过 `sol-vaultd` 走专用通道

#### 4.5 Drag-and-Drop (`capability:dnd`)

```rust
message StartDrag {
    surface: SurfaceId,
    token: InteractionToken,  // 必须在指针按下后才能启动
    mime_types: Vec<String>,
    icon_surface: Option<SurfaceId>,
}

// Compositor 通知拖动进入目标
message DragEnter {
    surface: SurfaceId,
    serial: u32,
    position: (f64, f64),     // surface-local 坐标
    mime_types: Vec<String>,
    source_app_id: AppId,     // 来源应用（用于访问控制）
}

message DragMotion {
    serial: u32,
    position: (f64, f64),
    time_ms: u32,
}

message DragLeave {
    serial: u32,
}

// 目标响应：接受哪些 MIME 类型
message SetDragActions {
    serial: u32,
    accepted_mime_type: Option<String>,
    preferred_action: DndAction {  // Copy | Move | Ask
        Copy,
        Move,
        Ask,
    },
}

// 拖动完成
message Drop {
    serial: u32,
}

// 目标请求数据
message RequestDragData {
    serial: u32,
    mime_type: String,
}

// 源提供数据
message SendDragData {
    serial: u32,
    mime_type: String,
    data: Vec<u8>,  // 或通过 FD 传递大文件
}

message DragCancelled {
    reason: CancelReason {
        EscapePressed,
        TargetRejected,
        SourceCancelled,
    }
}
```

**约束**：
- 必须由真实用户交互触发（合成的指针事件无效）
- Drop 接收方可以选择拒绝（按 MIME 类型/来源 AppId）
- 跨应用拖放会显示来源 AppId，接收方可据此做访问控制
- 敏感数据（密码）不通过 DnD 传递

### 5. 核心协议：缓冲区与输入

#### 5.1 缓冲区管理

```rust
// 创建共享内存缓冲区池
message CreateShmPool {
    pool_id: PoolId,
    fd: RawFd,           // 通过 SCM_RIGHTS 传递
    size: usize,
}

// 从池中创建缓冲区
message CreateBuffer {
    buffer_id: BufferId,
    pool_id: PoolId,
    offset: usize,
    width: i32,
    height: i32,
    stride: i32,
    format: ShmFormat {  // ARGB8888, XRGB8888, etc.
        Argb8888,
        Xrgb8888,
        Rgb565,
    },
}

// 创建 DMA-BUF 缓冲区（GPU 零拷贝）
message CreateDmabufBuffer {
    buffer_id: BufferId,
    width: i32,
    height: i32,
    format: DrmFormat,   // DRM_FORMAT_* fourcc codes
    modifier: u64,       // DRM format modifier
    planes: Vec<DmabufPlane> {
        fd: RawFd,       // 通过 SCM_RIGHTS 传递
        offset: u32,
        stride: u32,
    },
}

// 将缓冲区附加到 surface
message AttachBuffer {
    surface_id: SurfaceId,
    buffer_id: Option<BufferId>,  // None 表示 detach（隐藏窗口）
    x: i32,  // 缓冲区内的偏移（用于子 surface）
    y: i32,
}

// 标记损坏区域（优化重绘）
message Damage {
    surface_id: SurfaceId,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

// 提交 surface 状态（原子化所有 pending 操作）
message Commit {
    surface_id: SurfaceId,
}

// Compositor 通知缓冲区可以释放
message BufferRelease {
    buffer_id: BufferId,
}
```

**缓冲区语义**：
- 双缓冲机制：客户端维护至少 2 个缓冲区，轮流提交
- `Commit` 之前的所有操作都是 pending 状态，`Commit` 原子化应用
- `BufferRelease` 后客户端才能重新绘制该缓冲区
- DMA-BUF 优先用于 GPU 加速应用（零拷贝）
- 共享内存用于 CPU 渲染（Skia, Cairo）

#### 5.2 输入事件

##### 5.2.1 键盘输入

```rust
// Compositor 通知键盘进入 surface
message KeyboardEnter {
    surface_id: SurfaceId,
    serial: u32,
    keys: Vec<u32>,  // 当前按下的按键（raw keycodes）
}

message KeyboardLeave {
    surface_id: SurfaceId,
    serial: u32,
}

message Key {
    surface_id: SurfaceId,
    serial: u32,
    time_ms: u32,
    key: u32,        // Linux evdev keycode
    state: KeyState { Pressed, Released },
}

message Modifiers {
    surface_id: SurfaceId,
    serial: u32,
    mods_depressed: u32,  // 当前按下的修饰键（XKB bitfield）
    mods_latched: u32,    // 锁存的修饰键（Caps Lock 等）
    mods_locked: u32,     // 锁定的修饰键
    group: u32,           // XKB 键盘布局组
}

// Compositor 通知键盘映射（keymap）
message KeymapFormat {
    format: KeymapFormat {
        NoKeymap,
        XkbV1,
    },
    fd: RawFd,      // XKB keymap 通过 FD 传递（共享内存）
    size: u32,
}

// 按键重复配置
message RepeatInfo {
    rate: i32,      // 每秒重复次数（0 = 禁用）
    delay: i32,     // 首次重复前延迟（毫秒）
}
```

**键盘语义**：
- 焦点跟随鼠标点击（compositor 控制）
- 仅前台 surface 接收键盘事件
- XKB keymap 描述键盘布局（QWERTY, Dvorak, etc.）
- 客户端负责按键重复（compositor 只发送 press/release）
- 修饰键（Shift, Ctrl, Alt）独立事件通知

##### 5.2.2 指针输入

```rust
message PointerEnter {
    surface_id: SurfaceId,
    serial: u32,
    position: (f64, f64),  // surface-local 坐标
}

message PointerLeave {
    surface_id: SurfaceId,
    serial: u32,
}

message PointerMotion {
    surface_id: SurfaceId,
    time_ms: u32,
    position: (f64, f64),
}

message PointerButton {
    surface_id: SurfaceId,
    serial: u32,
    time_ms: u32,
    button: u32,       // Linux evdev button code (BTN_LEFT=0x110, etc.)
    state: ButtonState { Pressed, Released },
}

message PointerAxis {
    surface_id: SurfaceId,
    time_ms: u32,
    axis: AxisSource {
        Wheel,         // 鼠标滚轮
        Finger,        // 触摸板双指滚动
        Continuous,    // 连续滚动（触摸板惯性）
        WheelTilt,     // 滚轮倾斜
    },
    orientation: Orientation { Vertical, Horizontal },
    value: f64,        // 滚动距离（像素）
    discrete: i32,     // 离散步进（滚轮刻度）
}

// 客户端设置光标图像
message SetCursor {
    serial: u32,           // 必须来自最近的 PointerEnter/Button 事件
    surface_id: Option<SurfaceId>,  // 光标 surface（None = 隐藏光标）
    hotspot: (i32, i32),   // 光标热点
}

// Compositor 通知进入/离开 surface 的帧事件（用于光标动画）
message PointerFrame {
    // 标记一组相关事件的结束（批量处理优化）
}
```

**指针语义**：
- 指针位置精度：f64（支持 HiDPI 和亚像素精度）
- 光标图像由客户端提供（compositor 合成到屏幕）
- `serial` 用于关联事件序列，防止陈旧操作
- `PointerFrame` 标记事件批次边界（优化重绘）

##### 5.2.3 触摸输入

```rust
message TouchDown {
    surface_id: SurfaceId,
    serial: u32,
    time_ms: u32,
    touch_id: i32,     // 多点触控时区分不同手指
    position: (f64, f64),
}

message TouchUp {
    serial: u32,
    time_ms: u32,
    touch_id: i32,
}

message TouchMotion {
    time_ms: u32,
    touch_id: i32,
    position: (f64, f64),
}

message TouchCancel {
    // 触摸序列被系统中断（来电、通知等）
}

message TouchFrame {
    // 标记一组触摸事件的结束（批量处理）
}

message TouchShape {
    touch_id: i32,
    major: f64,        // 触摸椭圆长轴（mm）
    minor: f64,        // 触摸椭圆短轴（mm）
}

message TouchOrientation {
    touch_id: i32,
    orientation: f64,  // 触摸椭圆旋转角度（弧度）
}
```

**触摸语义**：
- 每个 touch_id 独立跟踪（支持 10+ 点触控）
- `TouchFrame` 标记手势边界（识别滑动、捏合等）
- `Shape` 和 `Orientation` 用于高级手势识别（压力、倾斜）
- `TouchCancel` 后必须丢弃该手势状态

#### 5.3 输出管理

```rust
// Compositor 通知新输出可用
message OutputAdded {
    output_id: OutputId,
    name: String,           // "HDMI-A-1", "eDP-1"
    description: String,    // "Dell P2415Q"
    geometry: Rect {
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    },
    physical_size: (i32, i32),  // 物理尺寸（mm）
    subpixel: SubpixelLayout {
        Unknown,
        None,
        HorizontalRgb,
        HorizontalBgr,
        VerticalRgb,
        VerticalBgr,
    },
    transform: Transform {
        Normal,
        Rotate90,
        Rotate180,
        Rotate270,
        Flipped,            // 镜像
        Flipped90,
        Flipped180,
        Flipped270,
    },
    scale: i32,             // HiDPI 缩放因子（1, 2, 3, ...）
    modes: Vec<OutputMode>,
    current_mode: OutputMode,
}

message OutputMode {
    width: i32,
    height: i32,
    refresh_rate: i32,  // 刷新率（mHz，例如 60000 = 60Hz）
    flags: ModeFlags {
        Current = 0x1,
        Preferred = 0x2,
    },
}

// 输出属性变化
message OutputGeometryChanged {
    output_id: OutputId,
    geometry: Rect,
}

message OutputScaleChanged {
    output_id: OutputId,
    scale: i32,
}

message OutputModeChanged {
    output_id: OutputId,
    mode: OutputMode,
}

message OutputRemoved {
    output_id: OutputId,
}

// Surface 进入/离开输出
message SurfaceEnterOutput {
    surface_id: SurfaceId,
    output_id: OutputId,
}

message SurfaceLeaveOutput {
    surface_id: SurfaceId,
    output_id: OutputId,
}
```

**输出语义**：
- 多显示器布局由 compositor 管理（客户端只读）
- `SurfaceEnterOutput` 提示客户端调整缩放（HiDPI）
- Surface 可以跨越多个输出（compositor 自动处理）
- `scale` 是整数缩放因子（fractional scaling 由 compositor 处理）

### 6. 身份验证与授权流程

#### 6.1 连接建立

```rust
// 1. 客户端连接
message Connect {
    app_id: AppId,
    pid: ProcessId,
    credential: UnixCredential,
}

// 2. Compositor 验证身份
sol-securityd.verify_app_identity(pid, app_id) -> Result<AuthToken>

// 3. 返回授权的能力清单
message Connected {
    session_id: SessionId,
    granted_capabilities: Vec<Capability>,
    restrictions: Restrictions,  // 例如：no_screenshots, sandbox_level
}
```

#### 6.2 能力请求

```rust
// 运行时请求新能力
message RequestCapability {
    capability: Capability,
    justification: LocalizedString,  // 向用户解释用途
}

// Compositor → sol-securityd → Shell (显示对话框)
sol-securityd.evaluate_capability_request(
    app_id,
    capability,
    user_context,  // 前台/后台、最近交互时间
) -> Decision {
    Granted { token, expires_at },
    Denied { reason },
    NeedsUserConsent { dialog_spec },
}
```

### 7. 与现有架构的整合

#### 7.1 sol-securityd 职责

```rust
pub trait SecurityCoordinator {
    /// 验证应用身份（PID → AppId）
    fn verify_app_identity(&self, pid: u32) -> Result<AppId>;
    
    /// 评估能力请求
    fn evaluate_capability(&self, app_id: &AppId, cap: Capability) -> Decision;
    
    /// 签发能力 token
    fn issue_token(&self, app_id: &AppId, cap: Capability) -> Token;
    
    /// 验证 token 有效性
    fn verify_token(&self, token: &Token) -> Option<(AppId, Capability)>;
    
    /// 审计日志
    fn audit_capability_use(&self, app_id: &AppId, cap: Capability, outcome: Outcome);
}
```

#### 7.2 Compositor 状态

```rust
pub struct ScpState {
    /// 已认证的客户端会话
    sessions: HashMap<SessionId, AuthenticatedSession>,
    
    /// 活跃的能力授权（带过期时间）
    active_capabilities: HashMap<(AppId, Capability), CapabilityGrant>,
    
    /// Surface 到 AppId 的映射
    surfaces: HashMap<SurfaceId, Surface>,
    
    /// 安全协调器（与 sol-securityd 通信）
    security: Arc<dyn SecurityCoordinator>,
    
    /// Shell 连接（用于显示系统对话框）
    shell: Option<ShellConnection>,
}

pub struct AuthenticatedSession {
    app_id: AppId,
    pid: u32,
    granted_capabilities: HashSet<Capability>,
    connection_time: Instant,
    last_user_interaction: Option<Instant>,
    foreground: bool,  // 前台/后台状态（用于隐私保护）
}
```

### 8. 协议传输

#### 8.1 序列化格式

改用 **protobuf**（不使用 Wayland 的 XML 协议定义）：

**优势**：
- 类型安全的序列化
- 前向/后向兼容性
- 不需要运行时代码生成
- 更小的消息体积

```protobuf
// scp.proto
syntax = "proto3";

message ClientMessage {
  oneof message {
    Connect connect = 1;
    CreateSurface create_surface = 2;
    CreateToplevel create_toplevel = 3;
    AttachBuffer attach_buffer = 4;
    Commit commit = 5;
    // ... 其他客户端消息
  }
}

message CompositorMessage {
  oneof message {
    Connected connected = 1;
    ToplevelConfigured toplevel_configured = 2;
    KeyboardEnter keyboard_enter = 3;
    PointerMotion pointer_motion = 4;
    // ... 其他 compositor 消息
  }
}

message CreateSurface {
  uint32 surface_id = 1;
}

message ConfigureToplevel {
  uint32 toplevel_id = 1;
  int32 width = 2;
  int32 height = 3;
  int32 decoration_height = 4;
  uint32 states = 5;  // bitflags
}
```

#### 8.2 传输层

Unix domain socket + `SCM_RIGHTS` 传递文件描述符：

```rust
// 连接路径
const SCP_SOCKET: &str = "sol-compositor-0";  // $XDG_RUNTIME_DIR/sol-compositor-0

// 消息帧格式
struct MessageFrame {
    length: u32,        // 消息长度（不含头部）
    message: Vec<u8>,   // protobuf 序列化数据
}

// 认证流程
1. connect() 到 socket
2. 发送 SCM_CREDS (PID, UID, GID)
3. Compositor 通过 /proc/{pid}/exe 验证 AppId
4. 返回 Connected 消息（包含 session_id 和授权能力）

// FD 传递（缓冲区、keymap）
sendmsg() with SCM_RIGHTS ancillary data
```

**传输保证**：
- 消息按序投递（TCP 语义）
- 客户端崩溃时，compositor 自动清理其 surface
- Compositor 崩溃时，客户端 socket 断开（触发重连逻辑）

### 9. 迁移路径（已修订 - 见 ADR-0028）

**决策更新（2026-08-26）**：SOL 将不维护 Wayland 兼容层。完整理由见 ADR-0028。

#### Phase 1: 纯 SCP（立即执行）
- Compositor **仅支持 SCP**
- 移除所有 Smithay Wayland 协议状态
- 开发期通过 winit backend 在宿主 Wayland session 运行
- 所有测试客户端使用 SCP 协议

#### Phase 2: 原生渲染管道
- 移除 Smithay 渲染抽象
- 直接实现 DRM/GBM compositor
- SCP-aware 的 surface 生命周期管理

#### Phase 3: 生产化
- 完全移除 Smithay 依赖
- 正式文档："SOL 不兼容 Wayland 应用"
- 发布第三方开发者迁移指南

### 9. 实现清单

```rust
// compositor/src/scp/ 新模块结构
mod protocol;       // 协议定义（生成的 protobuf 代码）
mod state;          // ScpState 实现
mod handlers;       // 消息处理器
mod capability;     // 能力验证逻辑
mod security;       // 与 sol-securityd 的 IPC
mod surface;        // Surface 管理
mod input;          // 输入事件分发
mod output;         // 输出管理
mod buffer;         // 缓冲区管理（SHM + DMA-BUF）
```

#### 9.1 实现优先级

**P0 - 核心功能**（Phase 1）：
- [ ] Socket 传输层（Unix domain socket + 消息帧）
- [ ] 身份验证（PID → AppId）
- [ ] Surface 生命周期（创建、提交、销毁）
- [ ] 缓冲区管理（共享内存 + SCM_RIGHTS）
- [ ] Toplevel 窗口协议
- [ ] 键盘输入（enter/leave/key/modifiers）
- [ ] 指针输入（enter/leave/motion/button/axis）
- [ ] 输出通知（添加、移除、属性变化）

**P1 - 基础能力**（Phase 2）：
- [ ] Popup 窗口协议
- [ ] 触摸输入
- [ ] DMA-BUF 缓冲区支持
- [ ] 剪贴板（读/写）
- [ ] Drag-and-Drop
- [ ] sol-securityd 集成（D-Bus IPC）

**P2 - 高级能力**（Phase 3）：
- [ ] 屏幕捕获（含用户授权流程）
- [ ] 全局快捷键
- [ ] Fullscreen 能力
- [ ] 能力 token 过期/刷新机制
- [ ] 审计日志写入

#### 9.2 协议完整性

所有消息类型（截至本文档）：

**客户端 → Compositor**：
- `Connect` - 连接建立
- `CreateSurface` / `DestroySurface` - Surface 生命周期
- `CreateShmPool` / `CreateBuffer` / `CreateDmabufBuffer` - 缓冲区
- `AttachBuffer` / `Damage` / `Commit` - Surface 状态
- `CreateToplevel` / `SetToplevelState` / `SetToplevelTitle` / `SetToplevelAppId` - Toplevel
- `CreatePopup` / `SetDragActions` - Popup 和 DnD
- `RequestCapability` - 能力请求
- `RegisterShortcut` - 全局快捷键
- `RequestCapture` - 屏幕捕获
- `ClipboardRequest` - 剪贴板访问
- `StartDrag` / `RequestDragData` / `SendDragData` - 拖放
- `SetCursor` - 光标设置

**Compositor → 客户端**：
- `Connected` - 连接确认
- `ToplevelConfigured` / `PopupConfigured` - 窗口配置
- `PopupDismissed` - Popup 关闭
- `KeyboardEnter` / `KeyboardLeave` / `Key` / `Modifiers` / `KeymapFormat` / `RepeatInfo` - 键盘
- `PointerEnter` / `PointerLeave` / `PointerMotion` / `PointerButton` / `PointerAxis` / `PointerFrame` - 指针
- `TouchDown` / `TouchUp` / `TouchMotion` / `TouchCancel` / `TouchFrame` / `TouchShape` / `TouchOrientation` - 触摸
- `OutputAdded` / `OutputRemoved` / `OutputGeometryChanged` / `OutputScaleChanged` / `OutputModeChanged` - 输出
- `SurfaceEnterOutput` / `SurfaceLeaveOutput` - Surface 输出关系
- `BufferRelease` - 缓冲区释放
- `CaptureGranted` - 捕获授权
- `ShortcutGranted` - 快捷键授权
- `DragEnter` / `DragMotion` / `DragLeave` / `Drop` / `DragCancelled` - 拖放事件

**双向**：
- FD 传递（`SCM_RIGHTS`）：buffer fd, keymap fd, capture fd


## Consequences

### Positive

1. **最小权限**：应用仅获得声明的能力
2. **可审计**：所有敏感操作都经过 sol-securityd 记录
3. **防钓鱼**：客户端无法伪造系统 UI（标题栏、对话框）
4. **细粒度控制**：可以按 AppId 限制协议访问
5. **前向兼容**：protobuf 支持协议演进

### Negative

1. **生态系统断裂**：需要应用迁移到 SCP（已接受，见 ADR-0028）
2. **开发成本**：需要从零实现协议栈
3. **调试复杂度**：新协议缺乏 Wayland 的工具支持

### Mitigations

- **无 Wayland 兼容层**：SOL 是 Linux Family OS，不是 Linux 发行版（ADR-0028）
- **SDK 封装**：`sol-app` 隐藏协议细节，应用只用高层 API
- **工具开发**：提供 `scp-inspector`（类似 `weston-info`）
- **迁移指南**：为第三方开发者提供详细的移植文档

## References

- ADR-0028: Drop Wayland Compatibility Layer（产品定位决策）
- ADR-0021: 应用沙箱和资源授权
- ADR-0022: 账户系统架构
- ADR-0006: Compositor 与 Shell 分离
- [Wayland security issues](https://gitlab.freedesktop.org/wayland/wayland/-/issues/11)
- [Plan 9 security model](http://doc.cat-v.org/plan_9/4th_edition/papers/auth)
