# SCP Implementation Checklist

## Completed ✓

- [x] ADR-0027: SOL Compositor Protocol design document
- [x] **协议设计细化完成**：输入（键盘/指针/触摸）、缓冲区（SHM/DMA-BUF）、输出管理、Popup 窗口的完整消息定义
- [x] Core protocol message definitions (`protocol.rs`)
- [x] Capability system (`capability.rs`)
  - Default, shell-only, lock-only, and sensitive capabilities
  - Time-based and focus-based constraints, enforced in `state.rs`
- [x] Security coordinator interface (`security.rs`)
  - Stub implementation for Phase 1
  - Hooks for sol-securityd integration
- [x] Surface and toplevel management (`surface.rs`)
  - SCP surface roles (toplevel, popup, layer-shell, lock surface)
  - Server-side decoration height reserved on the client's behalf
- [x] Core state machine (`state.rs`)
  - Session authentication
  - Capability grant tracking
  - Message routing
- [x] Buffer management (`buffer.rs`)
  - SHM pool and buffer creation
  - Buffer FD lifecycle tracking
- [x] Input focus tracking (`input.rs`)
  - Keyboard/pointer/touch state tracking
  - Focus management
  - Event dispatch generators
- [x] Output management (`output.rs`)
  - Output lifecycle (add/remove/configure)
  - Mode, scale, transform handling
- [x] Example client (`examples/scp-client.rs`)
- [x] Compilation verified in an SCP-only workspace
- [x] Capability tokens are opaque, session/app/capability-bound, and verified on use
- [x] Client-local surface IDs are isolated between sessions
- [x] **Per-session outbound event queues** (`event_queue.rs`) — the compositor can
      push events to any client, not only reply to the client that spoke
- [x] **Z-ordered window stack and hit-testing** (`stack.rs`)
- [x] **Session lock** (`session_lock.rs`) — lock surfaces, full input takeover,
      crash-safe abandonment

## Next Steps

### Phase 1: 核心协议 (P0 - 立即执行)

#### 传输层 ✓ (已完成)
- [x] Unix domain socket listener at `$XDG_RUNTIME_DIR/sol-compositor-0`
- [x] `SO_PEERCRED` PID verification before application authentication
- [x] `SCM_RIGHTS` for buffer FD passing (JSON FD integers are ignored)
- [x] 4-byte big-endian length framing with a 1 MiB message limit
- [x] Backend-independent listener used by the current headless SCP service
- [x] Worker-thread I/O with shared SCP state
- [x] Safe stale-socket recovery without replacing live sockets or regular files
- [x] **出站 `SCM_RIGHTS`** — keymap 与剪贴板/拖放管道经带外通道送达客户端
- [x] **eventfd 唤醒** — 另一线程入队事件时唤醒阻塞在 `poll` 的客户端线程
- [x] **背压处理** — 每会话有界队列；停止读取的客户端被断开，而非无限占用合成器内存
- [ ] Move connection I/O into the compositor event loop after the native renderer consumes SCP surfaces

#### 缓冲区管理 ✓ (核心功能已完成)
- [x] 共享内存池 (`CreateShmPool`) - `BufferManager::create_pool`
- [x] 从池创建缓冲区 (`CreateBuffer` with format, stride) - `BufferManager::create_buffer`
- [x] Buffer attach/damage/commit 语义 - `AttachBuffer` + `Commit` 消息处理
- [x] 缓冲区销毁 (`DestroyBuffer`) - `BufferManager::destroy_buffer`
- [x] 缓冲区到 FD 的映射管理 - `BufferManager` 内部 HashMap
- [ ] DMA-BUF 缓冲区支持 (`CreateDmabufBuffer` with planes, modifier) - Phase 2
- [ ] 双缓冲机制（`BufferRelease` 事件）- 需要渲染器集成后实现

#### Surface 生命周期扩展 ✓
- [x] `CreateSurface` / `DestroySurface` 处理，含按角色的级联清理
- [x] `Commit` 原子化 pending 状态
- [x] Surface 到 AppId 映射
- [x] `Damage` 消息定义 - 接收但暂不优化
- [x] `SetInputRegion` / `SetOpaqueRegion` 消息定义
- [x] **Surface 输入区域实际应用到 hit-testing**（显式空区域 = 点击穿透）
- [x] **Frame callback 实际投递**（`send_frame_callbacks` 经事件队列）
- [ ] Damage tracking 渲染优化 - 需要渲染器

#### Toplevel 窗口扩展 ✓
- [x] `CreateToplevel` 处理
- [x] `ToplevelConfigured` 计算（size, decoration_height, states）
- [x] **初始窗口布局**（输出居中 + 级联，`place_toplevel`）
- [x] **`SetToplevelState` 处理**（maximize / minimize / fullscreen / unset），含实际位置更新
- [x] **`SetToplevelTitle` / `SetToplevelAppId` 动态更新**
      （客户端自述 app_id 与已验证身份分开存储，前者不可覆盖后者）
- [x] **`CloseToplevel` 请求 + `ToplevelClosed` 事件**，关闭后焦点顺延到下一窗口
- [x] **Fullscreen 能力验证**（经 `SetToplevelState::Fullscreen` 亦需 capability）
- [x] **焦点激活状态**（获得/失去焦点时重新 configure `activated`）
- [ ] 服务端装饰渲染（标题栏、关闭按钮）- 需要渲染器；目前仅上报保留高度

#### 输入事件 - 键盘 ✓
- [x] 输入状态管理器 (`InputState`) 实现
- [x] `KeyboardEnter` / `KeyboardLeave` 事件生成
- [x] `Key` 事件分发（press/release）
- [x] `Modifiers` 消息
- [x] **XKB keymap 经 memfd + `SCM_RIGHTS` 实际投递**（获得焦点时，先于任何按键事件）
- [x] `RepeatInfo` 投递
- [x] **焦点管理与窗口切换联动**（`set_keyboard_focus` 发送 leave/enter 转换）
- [x] **Escape 在 popup grab 期间被拦截**
- [ ] 实际键盘输入从原生硬件 backend 集成到 `handle_key` - Phase 2

#### 输入事件 - 指针 ✓
- [x] 指针状态管理器 (`InputState`) 实现
- [x] **`PointerEnter` / `PointerLeave` 随光标跨窗口自动切换**
- [x] **`PointerMotion` 分发**（surface-local 坐标，f64 精度）
- [x] **`PointerButton` 分发**，含 focus-follows-click 与窗口抬升
- [x] `PointerAxis` 滚轮/触摸板滚动（discrete + continuous）
- [x] `PointerFrame` 批量事件边界
- [x] `SetCursor` 消息定义
- [ ] 实际指针输入从原生硬件 backend 集成到 InputState - Phase 2
- [ ] 光标 surface 合成渲染 - Phase 2

#### 输入事件 - 触摸 ✓
- [x] 触摸点状态管理器 (`InputState`) 实现
- [x] **`TouchDown` / `TouchUp` / `TouchMotion` 分发**，触摸序列粘附于起始 surface
- [x] `TouchCancel` / `TouchFrame`（按 surface 去重，而非按触点）
- [x] `TouchShape` / `TouchOrientation` 事件生成器（椭圆触摸区域）
- [x] **多点触控跟踪（touch_id → surface 映射）**
- [ ] 实际触摸输入从原生硬件 backend 集成到 InputState - Phase 2

#### 输出管理 ✓ (基础设施完成)
- [x] 输出管理器 (`OutputManager`) 实现
- [x] `OutputAdded` 消息定义（geometry, mode, scale, transform, subpixel）
- [x] `OutputRemoved` 消息定义
- [x] `OutputGeometryChanged` / `OutputScaleChanged` / `OutputModeChanged` 消息定义
- [x] `SurfaceEnterOutput` / `SurfaceLeaveOutput` 消息定义
- [x] Output 变换处理（旋转时自动交换宽高）
- [x] **多输出布局参与窗口定位与 hit-testing**（绝对坐标含 output 原点）
- [ ] 从原生 DRM/KMS backend 集成到 OutputManager - Phase 2
- [ ] Surface 跨输出跟踪自动化（发送 enter/leave）- Phase 2
- [ ] 多显示器布局管理 - Phase 2

### Phase 2: 基础能力 (P1 - 短期)

#### Popup 窗口 ✓
- [x] `CreatePopup` / `DestroyPopup`（需要 `WindowPopup` capability）
- [x] `PopupConfigured` 消息定义（最终位置）
- [x] `PopupDismissed` 消息定义（原因：outside_click, parent_closed, escape）
- [x] PopupPositioner 完整定义（anchor_rect, anchor, gravity, constraint_adjustment, offset, size）
- [x] Popup 定位算法实现
  - [x] Anchor rect + anchor edge 计算
  - [x] Gravity 方向展开
  - [x] Constraint adjustment (flip, slide, resize)
- [x] **`PopupDismissed` 触发逻辑**
  - [x] Outside click 检测（grab 消费该点击，不穿透到下方窗口）
  - [x] Parent closed 级联（内层先于外层收到通知）
  - [x] Escape key 监听（每次关闭最内层）
- [x] **嵌套 popup 支持**（parent 链累积偏移，深度受限以防环）
- [x] **Popup grab 语义**（grab chain；点击链内成员仅收起其子菜单并照常投递）
- [x] **Popup 以 (session, surface) 标识**，客户端本地 ID 不再跨会话互相别名

#### 触摸输入
已合并到"输入事件 - 触摸"部分

#### 剪贴板 ✓
- [x] `SetSelection` / `RequestSelection` 处理（Read/Write 区分）
- [x] 前台焦点检查（Read 权限）
- [x] 用户交互时间窗口检查（Write 权限，500ms）
- [x] **输入 serial 溯源** — 特权请求必须引用合成器真实下发过的输入 serial；
      仅按键、按钮、触摸产生此类 serial，被动指针移动不产生
- [x] MIME 类型协商与校验
- [x] **剪贴板数据传递（管道直传）** — 合成器只中介授权，不经手内容字节
- [x] Owner 断开时清空选区并向其余客户端广播
- [ ] 敏感数据过滤（密码 → sol-vaultd 专用通道）

#### Drag-and-Drop ✓
- [x] `StartDrag` 处理（capability + 交互时间窗口 + serial 校验）
- [x] `DragEnter` / `DragMotion` / `DragLeave` 事件生成（基于 hit-testing）
- [x] `AcceptDrag` 接收方响应，仅当前 drop target 可接受
- [x] `Drop` 数据传递触发；无 target 时自动取消
- [x] `ReceiveDragData` / `RequestDragData` / `DragData` 往返（与剪贴板同一管道机制）
- [x] `DragCancelled` 触发，并通知对端
- [x] 来源/目标 session 访问控制
- [ ] `SetDragActions`（Copy/Move/Ask）协商
- [ ] 拖动图标 surface 合成 - 需要渲染器

#### Session Lock ✓
- [x] `LockSession` / `UnlockSession`（`session-lock` capability，仅 sol-logind）
- [x] `CreateLockSurface` / `AckLockConfigure`，每输出一个全屏 surface
- [x] 加锁瞬间桌面即失去输入，无需等待锁屏首帧
- [x] 崩溃安全 —— locker 退出仅 *abandon*，会话保持锁定
- [x] `SessionLockEngaged` / `SessionLocked` / `SessionLockFinished` /
      `ConfigureLockSurface` / `SessionLockStateChanged`

#### sol-securityd 集成
- [ ] D-Bus IPC 连接
- [ ] `verify_app_identity(pid)` 实现（/proc/{pid}/exe → AppId，替换当前 comm stub）
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
- [x] 锁屏期间捕获阻止

#### 全局快捷键
- [ ] `RegisterShortcut` 处理（binding, justification）
- [ ] 快捷键冲突检测
- [ ] 优先级仲裁（System > Shell > App）
- [ ] Shell 授权对话框协调
- [ ] `ShortcutGranted` 响应
- [ ] 快捷键触发事件分发

#### Fullscreen 能力 ✓
- [x] `capability:fullscreen` 检查（`SetFullscreen` 与 `SetToplevelState` 两条路径）
- [x] `SetToplevelState::Fullscreen` 处理
- [x] 输出选择（指定或当前输出）
- [x] 全屏状态进入/退出，含几何与装饰高度调整

#### 能力管理
- [ ] Token 过期自动清理
- [ ] Token 刷新机制
- [ ] 运行时能力撤销接入 `state.rs`（`revocation.rs` 已就绪）
- [x] 前台/后台状态跟踪（由键盘焦点维护，用于隐私能力）
- [x] 最近用户交互时间跟踪

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
- [x] Retired frontend history and SCP-only boundary documented
- [ ] 能力参考手册

## Testing Plan

### 单元测试
- [x] Token and object-ownership enforcement
- [x] 事件队列（投递顺序、溢出、广播、注销）
- [x] 窗口栈与 hit-testing（Z 序、半开边界、输入区域穿透、级联布局）
- [x] Popup 管理器（grab chain、嵌套、级联关闭、按 session 隔离）
- [x] Session lock 状态机
- [x] 传输原语（`SCM_RIGHTS` 往返、eventfd 信号/清空、poll 就绪与挂断）
- [x] Surface 状态机（经 `scp_input.rs`）
- [ ] 能力评估逻辑（capability.rs）单测
- [ ] Token 生成/验证（security.rs 替换 stub 后）
- [ ] 消息序列化/反序列化（protocol.rs）

### 集成测试
- [x] Connect → create surface → create toplevel (`scp_session.rs`)
- [x] Buffer FD transfer via `SCM_RIGHTS` (`scp_session.rs`)
- [x] **合成器主动事件跨真实 socket 投递**（`scp_events.rs`）：
      另一线程产生的输入唤醒阻塞的客户端线程
- [x] **Keymap memfd 经 `SCM_RIGHTS` 到达且内容可读**
- [x] 键盘焦点转换、frame callback、`ToplevelClosed` 端到端
- [x] 多客户端事件定向（按键不泄漏到非焦点客户端）
- [x] 指针输入往返（`scp_input.rs`）
- [x] Popup 创建、定位与三种关闭原因
- [x] 剪贴板读写（能力/焦点/交互窗口拒绝路径 + 管道双端交付）
- [ ] Drag-and-drop 完整流程端到端
- [ ] 触摸输入往返端到端

### 安全测试
- [x] Token forgery rejection
- [x] Cross-session surface/toplevel/popup isolation
- [x] 未授权能力访问阻止（fullscreen、layer-shell、session-lock、clipboard-read）
- [x] 后台剪贴板读取阻止（前台焦点检查）
- [x] 无用户交互的剪贴板写入/拖动阻止（交互时间窗口 + serial 溯源）
- [x] 锁屏期间输入不可达桌面窗口
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

## Migration from the Retired Frontend

### 移除旧协议依赖 ✓
- [x] 删除旧协议状态和 handler
- [x] compositor 仅暴露 SCP Unix socket
- [x] 重写活动集成测试和参考客户端使用纯 SCP
- [x] 从 Cargo graph 和工作树移除 Smithay/Wayland/wlroots 依赖
- [x] CI 运行 `scripts/validate-scp-only.sh` 防止回归

### 与现有组件集成
- [x] Shell 连接使用 capability-scoped SCP layer surface
- [x] **输入接入点就绪** —— `handle_pointer_*` / `handle_key` / `handle_touch_*`
      是硬件 backend 唯一需要调用的入口
- [x] **渲染视图就绪** —— `build_stack()` 提供自底向上的绘制顺序与绝对几何
- [ ] Compositor 主循环集成 SCP surface 渲染
- [ ] 输入事件从原生硬件 backend 转换为 SCP 消息
- [ ] 渲染管线输出合成到 SCP surface

## Documentation Needs

- [x] SCP 协议设计文档（ADR-0027）
- [x] 协议消息完整定义（键盘、指针、触摸、缓冲区、输出、Popup）
- [ ] SCP 协议规范（for third-party implementors）
- [ ] 安全模型解释（for app developers）
- [x] 退役 frontend 历史与 SCP-only 边界说明
- [ ] 能力参考手册

## 协议消息汇总

**设计完整的消息**（共 60+ 条）：

### 核心协议
- Connection: `Connect`, `Connected`, `Rejected`, `ProtocolError`
- Surface: `CreateSurface`, `DestroySurface`, `AttachBuffer`, `Damage`, `Commit`,
  `BufferRelease`, `SetInputRegion`, `SetOpaqueRegion`
- Buffer: `CreateShmPool`, `CreateBuffer`, `DestroyBuffer`, `CreateDmabufBuffer`
- Frame: `FrameCallback`

### Window Management
- Toplevel: `CreateToplevel`, `ConfigureToplevel`, `SetToplevelState`,
  `SetToplevelTitle`, `SetToplevelAppId`, `CloseToplevel`, `ToplevelClosed`,
  `AckConfigure`, `SetFullscreen`
- Popup: `CreatePopup`, `DestroyPopup`, `ConfigurePopup`, `PopupDismissed`
- Layer shell: `CreateLayerSurface`, `ConfigureLayerSurface`, `SetLayerAnchor`,
  `SetLayerExclusiveZone`, `SetLayerMargin`, `SetLayerKeyboardInteractivity`,
  `SetLayerSize`, `AckLayerConfigure`, `LayerSurfaceClosed`
- Cursor: `SetCursor`

### Input Events
- Keyboard: `KeyboardEnter`, `KeyboardLeave`, `KeyboardKey`, `Modifiers`,
  `KeymapFormat`, `RepeatInfo`
- Pointer: `PointerEnter`, `PointerLeave`, `PointerMotion`, `PointerButton`,
  `PointerAxis`, `PointerFrame`
- Touch: `TouchDown`, `TouchUp`, `TouchMotion`, `TouchCancel`, `TouchFrame`,
  `TouchShape`, `TouchOrientation`

### Output Management
- `OutputAdded`, `OutputRemoved`, `OutputGeometryChanged`, `OutputScaleChanged`,
  `OutputModeChanged`, `OutputChanged`
- `SurfaceEnterOutput`, `SurfaceLeaveOutput`

### Session Lock
- `LockSession`, `CreateLockSurface`, `AckLockConfigure`, `UnlockSession`
- `SessionLockEngaged`, `SessionLocked`, `SessionLockFinished`,
  `ConfigureLockSurface`, `SessionLockStateChanged`

### Capabilities
- General: `RequestCapability`, `CapabilityDecision`
- Clipboard: `SetSelection`, `RequestSelection`, `SelectionOffer`,
  `RequestSelectionData`, `SelectionData`, `SelectionCleared`
- DnD: `StartDrag`, `DragEnter`, `DragMotion`, `DragLeave`, `Drop`, `AcceptDrag`,
  `ReceiveDragData`, `RequestDragData`, `DragData`, `FinishDrag`, `CancelDrag`,
  `DragFinished`, `DragCancelled`
- Screen Capture: `RequestCapture`, `CaptureGranted`
- Shortcuts: `RegisterShortcut`, `ShortcutGranted`
