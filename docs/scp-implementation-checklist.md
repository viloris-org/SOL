# SCP Implementation Checklist

## Completed ✓

- [x] ADR-0027: SOL Compositor Protocol design document
- [x] **协议设计细化完成**：输入（键盘/指针/触摸）、缓冲区（SHM/DMA-BUF）、输出管理、Popup 窗口的完整消息定义
- [x] Core protocol message definitions (`protocol.rs`)
- [x] Capability system (`capability.rs`)
  - Default, shell-only, and sensitive capabilities
  - Time-based and focus-based constraints
- [x] Security coordinator interface (`security.rs`)
  - Stub implementation for Phase 1
  - Hooks for sol-securityd integration
- [x] Surface and toplevel management (`surface.rs`)
  - SCP surface roles (toplevel, popup, layer-shell)
  - Server-side decoration support
- [x] Core state machine (`state.rs`)
  - Session authentication
  - Capability grant tracking
  - Message routing
- [x] Buffer management (`buffer.rs`)
  - SHM pool and buffer creation
  - Buffer FD lifecycle tracking
- [x] Input event infrastructure (`input.rs`)
  - Keyboard/pointer/touch state tracking
  - Focus management
  - Event dispatch generators
- [x] Output management (`output.rs`)
  - Output lifecycle (add/remove/configure)
  - Mode, scale, transform handling
- [x] Example client (`examples/scp-client.rs`)
- [x] Compilation verified (builds alongside the transitional Smithay backend)
- [x] Capability tokens are opaque, session/app/capability-bound, and verified on use
- [x] Client-local surface IDs are isolated between sessions

## Next Steps

### Phase 1: 核心协议 (P0 - 立即执行)

#### 传输层 ✓ (已完成)
- [x] Unix domain socket listener at `$XDG_RUNTIME_DIR/sol-compositor-0`
- [x] `SO_PEERCRED` PID verification before application authentication
- [x] `SCM_RIGHTS` for buffer FD passing (JSON FD integers are ignored)
- [x] 4-byte big-endian length framing with a 1 MiB message limit
- [x] Backend-independent listener enabled in winit, udev, and headless modes
- [x] Worker-thread I/O with shared SCP state
- [x] Safe stale-socket recovery without replacing live sockets or regular files
- [ ] Move connection I/O into the compositor event loop after the native renderer consumes SCP surfaces

#### 缓冲区管理 ✓ (核心功能已完成)
- [x] 共享内存池 (`CreateShmPool`) - `BufferManager::create_pool`
- [x] 从池创建缓冲区 (`CreateBuffer` with format, stride) - `BufferManager::create_buffer`
- [x] Buffer attach/damage/commit 语义 - `AttachBuffer` + `Commit` 消息处理
- [x] 缓冲区销毁 (`DestroyBuffer`) - `BufferManager::destroy_buffer`
- [x] 缓冲区到 FD 的映射管理 - `BufferManager` 内部 HashMap
- [ ] DMA-BUF 缓冲区支持 (`CreateDmabufBuffer` with planes, modifier) - Phase 2
- [ ] 双缓冲机制（`BufferRelease` 事件）- 需要渲染器集成后实现

#### Surface 生命周期扩展 ✓ (核心完成)
- [x] `CreateSurface` / `DestroySurface` 处理
- [x] `Commit` 原子化 pending 状态
- [x] Surface 到 AppId 映射
- [x] `Damage` 消息定义 - 接收但暂不优化
- [x] `SetInputRegion` / `SetOpaqueRegion` 消息定义 - 接收但暂不应用
- [ ] Surface 输入区域实际应用到 hit-testing - Phase 2
- [ ] Damage tracking 渲染优化 - Phase 2

#### Toplevel 窗口扩展
- [x] `CreateToplevel` 处理
- [x] `ToplevelConfigured` 计算（size, decoration_height, states）
- [ ] `SetToplevelState` 处理（maximize, minimize, fullscreen）
- [ ] `SetToplevelTitle` / `SetToplevelAppId` 动态更新
- [ ] 服务端装饰渲染（标题栏、关闭按钮）
- [ ] Fullscreen 能力验证

#### 输入事件 - 键盘 ✓ (基础设施完成)
- [x] 输入状态管理器 (`InputState`) 实现
- [x] `KeyboardEnter` / `KeyboardLeave` 事件生成器
- [x] `Key` 事件分发器（press/release，evdev keycodes）
- [x] `Modifiers` 消息定义
- [x] `KeymapFormat` 消息定义（XKB keymap 通过 FD 传递）
- [x] `RepeatInfo` 消息定义
- [ ] 实际键盘输入从 libinput/winit 集成到 InputState - Phase 2
- [ ] 焦点管理与窗口切换联动 - Phase 2

#### 输入事件 - 指针 ✓ (基础设施完成)
- [x] 指针状态管理器 (`InputState`) 实现
- [x] `PointerEnter` / `PointerLeave` 事件生成器
- [x] `PointerMotion` 事件分发器（surface-local 坐标，f64 精度）
- [x] `PointerButton` 事件分发器（evdev button codes）
- [x] `PointerAxis` 滚轮/触摸板滚动（discrete + continuous）
- [x] `PointerFrame` 批量事件边界
- [x] `SetCursor` 消息定义
- [ ] 实际指针输入从 libinput/winit 集成到 InputState - Phase 2
- [ ] 光标 surface 合成渲染 - Phase 2

#### 输入事件 - 触摸 ✓ (基础设施完成)
- [x] 触摸点状态管理器 (`InputState`) 实现
- [x] `TouchDown` / `TouchUp` / `TouchMotion` 事件生成器
- [x] `TouchCancel` / `TouchFrame` 事件生成器
- [x] `TouchShape` / `TouchOrientation` 事件生成器（椭圆触摸区域）
- [ ] 实际触摸输入从 libinput 集成到 InputState - Phase 2

#### 输出管理 ✓ (基础设施完成)
- [x] 输出管理器 (`OutputManager`) 实现
- [x] `OutputAdded` 消息定义（geometry, mode, scale, transform, subpixel）
- [x] `OutputRemoved` 消息定义
- [x] `OutputGeometryChanged` / `OutputScaleChanged` / `OutputModeChanged` 消息定义
- [x] `SurfaceEnterOutput` / `SurfaceLeaveOutput` 消息定义
- [x] Output 变换处理（旋转时自动交换宽高）
- [ ] 从 winit/DRM 实际输出集成到 OutputManager - Phase 2
- [ ] Surface 跨输出跟踪自动化 - Phase 2
- [ ] 多显示器布局管理 - Phase 2

### Phase 2: 基础能力 (P1 - 短期)

#### Popup 窗口 ✓ (协议完成，实现待集成)
- [x] `CreatePopup` 消息定义（PopupPositioner 解析）
- [x] `PopupConfigured` 消息定义（最终位置）
- [x] `PopupDismissed` 消息定义（原因：outside_click, parent_closed, escape）
- [x] PopupPositioner 完整定义（anchor_rect, anchor, gravity, constraint_adjustment, offset, size）
- [ ] Popup 定位算法实现
  - [ ] Anchor rect + anchor edge 计算
  - [ ] Gravity 方向展开
  - [ ] Constraint adjustment (flip, slide, resize)
- [ ] `PopupDismissed` 触发逻辑
  - [ ] Outside click 检测
  - [ ] Parent closed 级联
  - [ ] Escape key 监听
- [ ] 嵌套 popup 支持
- [ ] Popup grab 语义（点击外部关闭）

#### 触摸输入
已合并到"输入事件 - 触摸"部分
- [ ] `TouchCancel` 系统中断处理
- [ ] `TouchFrame` 手势边界标记
- [ ] `TouchShape` / `TouchOrientation` 高级手势数据
- [ ] 多点触控跟踪（touch_id 管理）

#### 剪贴板
- [ ] `ClipboardRequest` 处理（Read/Write 区分）
- [ ] 前台焦点检查（Read 权限）
- [ ] 用户交互时间窗口检查（Write 权限，500ms）
- [ ] MIME 类型协商
- [ ] 剪贴板数据传递（FD 或内联）
- [ ] 敏感数据过滤（密码 → sol-vaultd 专用通道）

#### Drag-and-Drop
- [ ] `StartDrag` 处理（InteractionToken 验证）
- [ ] `DragEnter` / `DragMotion` / `DragLeave` 事件生成
- [ ] `SetDragActions` 接收方响应（Copy/Move/Ask）
- [ ] `Drop` 数据传递触发
- [ ] `RequestDragData` / `SendDragData` 往返
- [ ] `DragCancelled` 触发（Escape/rejected）
- [ ] 拖动图标 surface 合成
- [ ] 来源 AppId 访问控制

#### sol-securityd 集成
- [ ] D-Bus IPC 连接
- [ ] `verify_app_identity(pid)` 实现（/proc/{pid}/exe → AppId）
- [ ] `evaluate_capability(app_id, cap)` 实现
- [ ] HMAC token 生成/验证（替换 stub）
- [ ] 能力过期时间跟踪
- [ ] 审计日志写入
- [ ] Shell 用户授权对话框协调

### Phase 3: 高级能力 (P2 - 中期)

#### 屏幕捕获
- [ ] `RequestCapture` 处理（scope: window/output/workspace）
- [ ] Shell 用户授权对话框协调
- [ ] 一次性 token 签发
- [ ] `CaptureGranted` 响应（buffer format）
- [ ] 捕获帧渲染（包含/排除光标）
- [ ] 红色边框 + 倒计时 UI（通过 Shell）
- [ ] 后台应用捕获阻止

#### 全局快捷键
- [ ] `RegisterShortcut` 处理（binding, justification）
- [ ] 快捷键冲突检测
- [ ] 优先级仲裁（System > Shell > App）
- [ ] Shell 授权对话框协调
- [ ] `ShortcutGranted` 响应
- [ ] 快捷键触发事件分发

#### Fullscreen 能力
- [ ] `capability:fullscreen` 检查
- [ ] `SetToplevelState::Fullscreen` 处理
- [ ] 输出选择（指定或当前输出）
- [ ] 全屏状态进入/退出

#### 能力管理
- [ ] Token 过期自动清理
- [ ] Token 刷新机制
- [ ] 运行时能力撤销
- [ ] 前台/后台状态跟踪（用于隐私能力）
- [ ] 最近用户交互时间跟踪

### Phase 4: 协议演进

#### Protobuf 迁移
- [ ] 定义 scp.proto schema
- [ ] 生成 Rust protobuf 代码
- [ ] 替换 serde JSON 序列化
- [ ] 版本协商（Connect 消息中的 protocol_version 字段）
- [ ] 向后兼容性测试

### Phase 5: 工具和文档

#### 协议工具
- [ ] `scp-inspector` - 协议调试工具（类似 `weston-info`）
- [ ] `scp-logger` - 消息日志记录器
- [ ] `scp-fuzzer` - 协议模糊测试
- [ ] SCP trace capture and replay tool

#### SDK 封装 (`sol-app`)
- [ ] 高层窗口 API（隐藏 SCP 协议细节）
- [ ] Rust bindings
- [ ] C FFI bindings
- [ ] Python bindings（可选）

#### 文档
- [ ] SCP 协议规范（for third-party implementors）
- [ ] 安全模型解释（for app developers）
- [ ] Wayland → SCP 迁移指南
- [ ] 能力参考手册

## Testing Plan

### 单元测试
- [x] Token and object-ownership enforcement
- [ ] 能力评估逻辑（capability.rs）
- [ ] Token 生成/验证（security.rs 替换 stub 后）
- [ ] Surface 状态机（surface.rs）
- [ ] 消息序列化/反序列化（protocol.rs）

### 集成测试
- [x] Connect → create surface → create toplevel
- [x] Buffer FD transfer via `SCM_RIGHTS`
- [ ] 键盘输入事件往返
- [ ] 指针输入事件往返
- [ ] Popup 创建和定位
- [ ] 剪贴板读写
- [ ] Drag-and-drop 完整流程

### 安全测试
- [x] Token forgery rejection
- [x] Cross-session surface/toplevel isolation
- [ ] 未授权能力访问阻止
- [ ] 后台剪贴板读取阻止
- [ ] 无用户交互的拖动阻止
- [ ] 权限提升尝试检测

### 性能测试
- [ ] 消息吞吐量（连续提交）
- [ ] 输入延迟（按键到应用响应）
- [ ] 多客户端并发
- [ ] 大缓冲区传递（4K 截图）

### 模糊测试
- [ ] 畸形消息处理
- [ ] 无效 surface ID 引用
- [ ] 越界缓冲区访问
- [ ] 能力 token 重放攻击

## Migration from Current State

### 移除 Wayland 依赖（Phase 2+）
- [ ] 清理 `state.rs` 中的 Smithay 协议状态
- [ ] 替换 Wayland backend 为纯 SCP backend
- [ ] 重写所有测试客户端使用纯 SCP
- [ ] 移除 `smithay` crate 依赖

### 与现有组件集成
- [ ] Compositor 主循环集成 SCP surface 渲染
- [ ] 输入事件从 winit/libinput 转换为 SCP 消息
- [ ] 渲染管线输出合成到 SCP surface
- [ ] Shell 连接使用 SCP LayerShell

## Documentation Needs

- [x] SCP 协议设计文档（ADR-0027）
- [x] 协议消息完整定义（键盘、指针、触摸、缓冲区、输出、Popup）
- [ ] SCP 协议规范（for third-party implementors）
- [ ] 安全模型解释（for app developers）
- [ ] Wayland → SCP 迁移指南
- [ ] 能力参考手册

## 协议消息汇总

**设计完整的消息**（共 50+ 条）：

### 核心协议
- Connection: `Connect`, `Connected`
- Surface: `CreateSurface`, `DestroySurface`, `AttachBuffer`, `Damage`, `Commit`, `BufferRelease`
- Buffer: `CreateShmPool`, `CreateBuffer`, `CreateDmabufBuffer`

### Window Management
- Toplevel: `CreateToplevel`, `ToplevelConfigured`, `SetToplevelState`, `SetToplevelTitle`, `SetToplevelAppId`
- Popup: `CreatePopup`, `PopupConfigured`, `PopupDismissed`
- Cursor: `SetCursor`

### Input Events
- Keyboard: `KeyboardEnter`, `KeyboardLeave`, `Key`, `Modifiers`, `KeymapFormat`, `RepeatInfo`
- Pointer: `PointerEnter`, `PointerLeave`, `PointerMotion`, `PointerButton`, `PointerAxis`, `PointerFrame`
- Touch: `TouchDown`, `TouchUp`, `TouchMotion`, `TouchCancel`, `TouchFrame`, `TouchShape`, `TouchOrientation`

### Output Management
- `OutputAdded`, `OutputRemoved`, `OutputGeometryChanged`, `OutputScaleChanged`, `OutputModeChanged`
- `SurfaceEnterOutput`, `SurfaceLeaveOutput`

### Capabilities
- General: `RequestCapability`
- Clipboard: `ClipboardRequest`
- DnD: `StartDrag`, `DragEnter`, `DragMotion`, `DragLeave`, `Drop`, `SetDragActions`, `RequestDragData`, `SendDragData`, `DragCancelled`
- Screen Capture: `RequestCapture`, `CaptureGranted`
- Shortcuts: `RegisterShortcut`, `ShortcutGranted`
