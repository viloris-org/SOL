# SOL Compositor Protocol (SCP) - 协议概览

> **快速参考**：SCP 是 SOL 的原生 compositor 协议，基于能力模型的权限最小化设计。
> 
> **完整设计**：见 [ADR-0027](decisions/ADR-0027-sol-compositor-protocol.md)  
> **实现状态**：见 [scp-implementation-checklist.md](scp-implementation-checklist.md)

## 核心设计原则

1. **能力导向的安全模型**：每个敏感操作都需要显式授权
2. **身份优先**：每个连接必须认证（PID → AppId）
3. **服务端装饰（SSD）**：客户端无法绘制标题栏（防止钓鱼）
4. **最小权限**：应用仅获得声明的能力
5. **可审计**：所有敏感操作都经过 sol-securityd 记录

## 协议分层

```
┌─────────────────────────────────────┐
│  SCP Core (必需协议)                 │
│  - Surface 创建/提交                 │
│  - 缓冲区管理 (SHM/DMA-BUF)          │
│  - 输入事件 (键盘/指针/触摸)         │
│  - 输出管理 (多显示器/HiDPI)         │
└─────────────────────────────────────┘
              ↓
┌─────────────────────────────────────┐
│  Capability Extensions (需授权)      │
│  - WindowToplevel (默认授予)         │
│  - Popup (默认授予)                  │
│  - Clipboard (需焦点/交互)           │
│  - DragAndDrop (需真实交互)          │
│  - ScreenCapture (每次需确认)        │
│  - GlobalShortcuts (需声明用途)      │
│  - Fullscreen (需显式授权)           │
│  - LayerShell (仅 sol-shell)         │
└─────────────────────────────────────┘
```

## 协议消息速查

### 连接与身份验证

| 消息 | 方向 | 用途 |
|------|------|------|
| `Connect` | Client → Compositor | 建立连接，声明 AppId 和 PID |
| `Connected` | Compositor → Client | 返回 session_id 和授权能力列表 |

**认证流程**：
1. 客户端 connect() 到 `$XDG_RUNTIME_DIR/sol-compositor-0`
2. 发送 `SCM_CREDS` (PID, UID, GID)
3. Compositor 通过 `/proc/{pid}/exe` 验证 AppId
4. 返回 `Connected` 消息

### Surface 生命周期

| 消息 | 方向 | 用途 |
|------|------|------|
| `CreateSurface` | Client → Compositor | 创建 surface |
| `DestroySurface` | Client → Compositor | 销毁 surface |
| `AttachBuffer` | Client → Compositor | 附加缓冲区到 surface |
| `Damage` | Client → Compositor | 标记损坏区域（优化重绘）|
| `Commit` | Client → Compositor | 原子化提交所有 pending 状态 |
| `BufferRelease` | Compositor → Client | 通知缓冲区可以释放 |
| `SurfaceEnterOutput` | Compositor → Client | Surface 进入某个输出 |
| `SurfaceLeaveOutput` | Compositor → Client | Surface 离开某个输出 |

**提交语义**：
- `Commit` 之前的所有操作都是 pending 状态
- `Commit` 原子化应用所有变更（缓冲区、damage、输入区域）
- 双缓冲：客户端至少维护 2 个缓冲区，收到 `BufferRelease` 后才能重绘

### 缓冲区管理

| 消息 | 方向 | 用途 |
|------|------|------|
| `CreateShmPool` | Client → Compositor | 创建共享内存池（通过 `SCM_RIGHTS` 传递 FD）|
| `CreateBuffer` | Client → Compositor | 从池中创建缓冲区（指定 format, stride）|
| `CreateDmabufBuffer` | Client → Compositor | 创建 DMA-BUF 缓冲区（GPU 零拷贝）|

**缓冲区类型**：
- **共享内存（SHM）**：CPU 渲染（Skia, Cairo），通过 `mmap()` 共享
- **DMA-BUF**：GPU 渲染（OpenGL, Vulkan），零拷贝，通过 `SCM_RIGHTS` 传递 FD

### Window Management

#### Toplevel 窗口

| 消息 | 方向 | 用途 |
|------|------|------|
| `CreateToplevel` | Client → Compositor | 创建顶层窗口 |
| `ToplevelConfigured` | Compositor → Client | 通知窗口尺寸和状态 |
| `SetToplevelState` | Client → Compositor | 请求状态变化（maximize/minimize/fullscreen）|
| `SetToplevelTitle` | Client → Compositor | 设置窗口标题（compositor 渲染）|
| `SetToplevelAppId` | Client → Compositor | 设置 AppId（用于分组和图标）|

**关键约束**：
- 客户端**不能**绘制标题栏（防止钓鱼）
- 客户端**不能**设置窗口位置（由 WM 决定）
- Fullscreen 需要额外的 `capability:fullscreen`

#### Popup 窗口

| 消息 | 方向 | 用途 |
|------|------|------|
| `CreatePopup` | Client → Compositor | 创建 popup（菜单、提示）|
| `PopupConfigured` | Compositor → Client | 通知 popup 最终位置 |
| `PopupDismissed` | Compositor → Client | Popup 被关闭 |

**Popup 定位**：
```rust
PopupPositioner {
    anchor_rect: Rect,     // 父 surface 上的锚点区域
    anchor_edge: Edge,     // Top | Bottom | Left | Right
    gravity: Gravity,      // 朝哪个方向展开
    constraint: Constraint, // FlipX | FlipY | SlideX | SlideY | ResizeX | ResizeY
    offset: (i32, i32),
    size: (i32, i32),
}
```

**Popup 语义**：
- 必须有父 surface（toplevel 或其他 popup）
- 支持嵌套 popup（菜单的子菜单）
- `grab=true` 时，点击外部自动关闭
- 父窗口关闭时，所有子 popup 自动销毁

### 输入事件

#### 键盘输入

| 消息 | 方向 | 用途 |
|------|------|------|
| `KeyboardEnter` | Compositor → Client | 键盘焦点进入 surface |
| `KeyboardLeave` | Compositor → Client | 键盘焦点离开 surface |
| `Key` | Compositor → Client | 按键按下/释放（evdev keycode）|
| `Modifiers` | Compositor → Client | 修饰键状态变化（Shift/Ctrl/Alt）|
| `KeymapFormat` | Compositor → Client | XKB keymap（通过 FD 传递）|
| `RepeatInfo` | Compositor → Client | 按键重复配置 |

**键盘语义**：
- 仅前台 surface 接收键盘事件
- XKB keymap 描述键盘布局（QWERTY, Dvorak, etc.）
- 客户端负责按键重复（compositor 只发送 press/release）

#### 指针输入

| 消息 | 方向 | 用途 |
|------|------|------|
| `PointerEnter` | Compositor → Client | 指针进入 surface |
| `PointerLeave` | Compositor → Client | 指针离开 surface |
| `PointerMotion` | Compositor → Client | 指针移动（surface-local 坐标）|
| `PointerButton` | Compositor → Client | 鼠标按钮按下/释放 |
| `PointerAxis` | Compositor → Client | 滚轮/触摸板滚动 |
| `PointerFrame` | Compositor → Client | 事件批次边界标记 |
| `SetCursor` | Client → Compositor | 设置光标图像 |

**指针语义**：
- 位置精度：`f64`（支持 HiDPI 和亚像素精度）
- 光标图像由客户端提供（compositor 合成到屏幕）
- `serial` 用于关联事件序列，防止陈旧操作

#### 触摸输入

| 消息 | 方向 | 用途 |
|------|------|------|
| `TouchDown` | Compositor → Client | 触摸点按下 |
| `TouchUp` | Compositor → Client | 触摸点抬起 |
| `TouchMotion` | Compositor → Client | 触摸点移动 |
| `TouchCancel` | Compositor → Client | 触摸序列被系统中断 |
| `TouchFrame` | Compositor → Client | 触摸事件批次边界 |
| `TouchShape` | Compositor → Client | 触摸椭圆形状（major/minor）|
| `TouchOrientation` | Compositor → Client | 触摸椭圆旋转角度 |

**触摸语义**：
- 每个 `touch_id` 独立跟踪（支持 10+ 点触控）
- `TouchFrame` 标记手势边界（识别滑动、捏合等）
- `TouchCancel` 后必须丢弃该手势状态

### 输出管理

| 消息 | 方向 | 用途 |
|------|------|------|
| `OutputAdded` | Compositor → Client | 新输出可用（显示器插入）|
| `OutputRemoved` | Compositor → Client | 输出移除（显示器拔出）|
| `OutputGeometryChanged` | Compositor → Client | 输出几何变化（位置/尺寸）|
| `OutputScaleChanged` | Compositor → Client | HiDPI 缩放因子变化 |
| `OutputModeChanged` | Compositor → Client | 分辨率/刷新率变化 |

**输出属性**：
```rust
OutputAdded {
    output_id: OutputId,
    name: String,           // "HDMI-A-1", "eDP-1"
    geometry: Rect,         // 多显示器布局中的位置
    physical_size: (i32, i32),  // 物理尺寸（mm）
    subpixel: SubpixelLayout,   // RGB/BGR 排列
    transform: Transform,   // 旋转/镜像
    scale: i32,             // HiDPI 缩放因子（1, 2, 3, ...）
    modes: Vec<OutputMode>, // 支持的分辨率/刷新率
}
```

### 能力扩展

#### 剪贴板

| 消息 | 方向 | 用途 | 权限要求 |
|------|------|------|----------|
| `ClipboardRequest` | Client → Compositor | 请求读/写剪贴板 | Read: 前台焦点<br>Write: 用户交互后 500ms 内 |

**安全策略**：
- 后台应用**无法读取**剪贴板（防止密码窃取）
- 写入需要在用户交互后 500ms 内（防止静默污染）
- 敏感数据（密码管理器）通过 `sol-vaultd` 走专用通道

#### Drag-and-Drop

| 消息 | 方向 | 用途 |
|------|------|------|
| `StartDrag` | Client → Compositor | 开始拖动（需 InteractionToken）|
| `DragEnter` | Compositor → Client | 拖动进入目标 surface |
| `DragMotion` | Compositor → Client | 拖动在目标内移动 |
| `DragLeave` | Compositor → Client | 拖动离开目标 surface |
| `SetDragActions` | Client → Compositor | 目标响应：接受哪些 MIME 类型 |
| `Drop` | Compositor → Client | 拖动完成，请求数据 |
| `RequestDragData` | Client → Compositor | 请求拖动数据 |
| `SendDragData` | Client → Compositor | 提供拖动数据 |
| `DragCancelled` | Compositor → Client | 拖动被取消 |

**约束**：
- 必须由真实用户交互触发（合成的指针事件无效）
- Drop 接收方可以选择拒绝（按 MIME 类型/来源 AppId）
- 跨应用拖放会显示来源 AppId，接收方可据此做访问控制

#### 屏幕捕获

| 消息 | 方向 | 用途 | 权限要求 |
|------|------|------|----------|
| `RequestCapture` | Client → Compositor | 请求捕获屏幕 | 每次需用户确认 |
| `CaptureGranted` | Compositor → Client | 授权捕获（一次性 token）| - |

**防止滥用**：
- 每次捕获都显示 Shell 原生提示（红色边框 + 倒计时）
- Token 仅单次有效，下次捕获需重新授权
- 后台应用无法捕获屏幕

#### 全局快捷键

| 消息 | 方向 | 用途 | 权限要求 |
|------|------|------|----------|
| `RegisterShortcut` | Client → Compositor | 注册全局快捷键 | 需声明用途，用户授权 |
| `ShortcutGranted` | Compositor → Client | 快捷键授权成功 | - |

**冲突处理**：
- 系统快捷键（Super+Q, Super+L）不可覆盖
- Shell 快捷键优先级高于应用
- 应用间冲突由用户仲裁

## 传输层

### Socket 路径
```
$XDG_RUNTIME_DIR/sol-compositor-0
```

### 消息帧格式
```
[4-byte length (big-endian)] [protobuf message]
```

### 文件描述符传递
通过 `SCM_RIGHTS` 传递：
- 缓冲区 FD（共享内存或 DMA-BUF）
- XKB keymap FD
- 捕获帧 FD

### 认证
通过 `SCM_CREDS` 获取客户端 PID，然后：
```
/proc/{pid}/exe → AppId（通过 sol-securityd 验证）
```

## 能力列表

| 能力 | 默认授予 | 用途 | 约束 |
|------|----------|------|------|
| `WindowToplevel` | ✓ | 创建顶层窗口 | - |
| `Popup` | ✓ | 创建 popup 窗口 | 需有父 surface |
| `ClipboardRead` | ✗ | 读取剪贴板 | 需前台焦点 |
| `ClipboardWrite` | ✗ | 写入剪贴板 | 需用户交互后 500ms 内 |
| `DragAndDrop` | ✓ | 拖放操作 | 需真实用户交互触发 |
| `ScreenCapture` | ✗ | 屏幕截图/录屏 | 每次需用户确认 |
| `GlobalShortcuts` | ✗ | 全局快捷键 | 需声明用途，用户授权 |
| `Fullscreen` | ✗ | 全屏模式 | 需显式授权 |
| `LayerShell` | ✗ | 系统层（状态栏等）| 仅 sol-shell |

## 安全特性

### 防钓鱼
- ❌ 客户端无法绘制标题栏
- ✓ Compositor 控制所有系统 UI（标题、关闭按钮）
- ✓ 应用图标来自 app bundle（不可伪造）

### 防窃听
- ❌ 后台应用无法读剪贴板
- ❌ 后台应用无法截屏
- ✓ 仅前台应用可接收键盘输入
- ✓ 每次截屏显示红色边框提示

### 防输入注入
- ❌ 客户端无法合成输入事件
- ✓ Drag-and-drop 必须由真实用户交互触发
- ✓ 接收方可按来源 AppId 拒绝 drop

### 能力隔离
- ✓ Layer-shell 仅限 sol-shell（普通应用无法伪造系统栏）
- ✓ 全局快捷键需要用户授权
- ✓ 全屏模式需要 Fullscreen 能力

## 与 Wayland 的区别

| 维度 | Wayland | SCP |
|------|---------|-----|
| 能力模型 | 全局协议扩展 | 显式授权的能力 |
| 标题栏 | CSD（客户端绘制）| SSD（服务端绘制，防钓鱼）|
| 剪贴板 | 无限制访问 | 前台/后台区分，时间窗口 |
| 截屏 | 隐式权限 | 每次需用户确认 |
| 身份验证 | 可选 | 必需（PID → AppId）|
| 审计 | 无 | 所有敏感操作记录 |
| 兼容性 | 向后兼容 | 无 Wayland 兼容层（见 ADR-0028）|

## 产品定位

SOL 是 **Linux Family OS**（类似 Android/Chrome OS），而非 Linux 发行版：

| 维度 | Linux 发行版 | SOL (Linux Family) |
|------|--------------|---------------------|
| 兼容性目标 | 运行现有应用 | 定义新应用模型 |
| 协议 | Wayland/X11 | SCP only |
| 应用打包 | .deb/.rpm + 依赖 | .app bundle（vendored）|
| 安全模型 | DAC + 可选 AppArmor | 基于能力的强制模型 |
| UI 一致性 | 工具包依赖 | 框架强制（SolKit）|

## 开发资源

- **完整设计**：[ADR-0027](decisions/ADR-0027-sol-compositor-protocol.md)
- **实现清单**：[scp-implementation-checklist.md](scp-implementation-checklist.md)
- **设计总结**：[scp-design-summary.md](scp-design-summary.md)
- **示例客户端**：`compositor/examples/scp-client.rs`
- **代码位置**：`compositor/src/scp/`

## 下一步

1. **P0 - 核心功能**：缓冲区管理、输入事件、输出管理
2. **P1 - 基础能力**：Popup、触摸、剪贴板、DnD、sol-securityd 集成
3. **P2 - 高级能力**：屏幕捕获、全局快捷键、Fullscreen
4. **P3 - 协议演进**：迁移到 protobuf，版本协商
5. **P4 - 工具和文档**：scp-inspector、SDK 封装、迁移指南
