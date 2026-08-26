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

#### 缓冲区管理
- [ ] 共享内存池 (`CreateShmPool`)
- [ ] 从池创建缓冲区 (`CreateBuffer` with format, stride)
- [ ] DMA-BUF 缓冲区支持 (`CreateDmabufBuffer` with planes, modifier)
- [ ] Buffer attach/damage/commit 语义
- [ ] 双缓冲机制（`BufferRelease` 事件）
- [ ] 缓冲区到 FD 的映射管理

#### Surface 生命周期扩展
- [x] `CreateSurface` / `DestroySurface` 处理
- [x] `Commit` 原子化 pending 状态
- [x] Surface 到 AppId 映射
- [ ] Surface 输入区域和不透明区域设置
- [ ] Damage tracking 优化

#### Toplevel 窗口扩展
- [x] `CreateToplevel` 处理
- [x] `ToplevelConfigured` 计算（size, decoration_height, states）
- [ ] `SetToplevelState` 处理（maximize, minimize, fullscreen）
- [ ] `SetToplevelTitle` / `SetToplevelAppId` 动态更新
- [ ] 服务端装饰渲染（标题栏、关闭按钮）
- [ ] Fullscreen 能力验证

#### 输入事件 - 键盘
- [ ] `KeyboardEnter` / `KeyboardLeave` 事件生成
- [ ] `Key` 事件分发（press/release，evdev keycodes）
- [ ] `Modifiers` 状态跟踪（XKB bitfields）
- [ ] XKB keymap 通过 FD 传递 (`KeymapFormat`)
- [ ] 按键重复配置 (`RepeatInfo`)
- [ ] 焦点管理（前台窗口接收输入）

#### 输入事件 - 指针
- [ ] `PointerEnter` / `PointerLeave` 事件生成
- [ ] `PointerMotion` 事件分发（surface-local 坐标，f64 精度）
- [ ] `PointerButton` 事件分发（evdev button codes）
- [ ] `PointerAxis` 滚轮/触摸板滚动（discrete + continuous）
- [ ] `PointerFrame` 批量事件边界
- [ ] `SetCursor` 光标图像设置
- [ ] 光标 surface 合成

#### 输出管理
- [ ] `OutputAdded` 事件（geometry, mode, scale, transform, subpixel）
- [ ] `OutputRemoved` 事件
- [ ] `OutputGeometryChanged` / `OutputScaleChanged` / `OutputModeChanged`
- [ ] `SurfaceEnterOutput` / `SurfaceLeaveOutput` 跟踪
- [ ] 多显示器布局管理
- [ ] HiDPI 缩放提示

### Phase 2: 基础能力 (P1 - 短期)

#### Popup 窗口
- [ ] `CreatePopup` 处理（PopupPositioner 解析）
- [ ] Popup 定位算法
  - [ ] Anchor rect + anchor edge 计算
  - [ ] Gravity 方向展开
  - [ ] Constraint adjustment (flip, slide, resize)
- [ ] `PopupConfigured` 最终位置计算
- [ ] `PopupDismissed` 触发
  - [ ] Outside click 检测
  - [ ] Parent closed 级联
  - [ ] Escape key 监听
- [ ] 嵌套 popup 支持
- [ ] Popup grab 语义（点击外部关闭）

#### 触摸输入
- [ ] `TouchDown` / `TouchUp` / `TouchMotion` 事件分发
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
