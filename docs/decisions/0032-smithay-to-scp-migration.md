# ADR-0032: Smithay到SCP的完全迁移

## 状态
已接受 (2026-08-26)

## 背景

当前compositor同时维护两套协议栈：
1. **Smithay/Wayland协议栈** (`state.rs`中的各种Handler实现)
2. **SCP协议栈** (`scp/`目录，~7500行代码)

这导致：
- 代码冗余和维护负担
- 协议语义不一致（Wayland的安全模型 vs SCP的capability模型）
- 与ADR-0028（"No Wayland Compatibility"）冲突
- 与产品定位冲突（Linux Family OS，非传统Linux发行版）

## 决策

**彻底移除Smithay/Wayland协议层，SCP成为唯一客户端协议。**

迁移分四个阶段：

### Phase 1: 渲染和输入解耦
**目标**：将渲染/输入从Smithay协议中分离

- 保留: `smithay::backend::renderer`, `smithay::backend::winit` (仅作为开发时的render backend)
- 移除: `smithay::wayland::*` 所有协议handler
- 创建: `compositor/src/render/` - 渲染抽象层
- 创建: `compositor/src/input/` - 输入抽象层

### Phase 2: 状态管理统一
**目标**：`SolState`只包含SCP状态

- 移除: `state.rs`中所有`impl *Handler for SolState`
- 移除: `CompositorState`, `XdgShellState`, `SeatState`等Smithay状态
- 统一到: `scp::ScpState`作为唯一真相源
- 重构: `window.rs`直接使用SCP surface而非Wayland surface

### Phase 3: 后端适配
**目标**：后端直接驱动SCP状态

- `main.rs`: 事件循环直接调用`ScpState::handle_*`
- `winit`: 输入事件 → SCP input消息
- `udev`: DRM/GBM/libinput → SCP消息
- 移除: `wayland_server::Display`, `ListeningSocket`

### Phase 4: 依赖清理
**目标**：最小化外部依赖

```toml
# 保留 (渲染后端)
smithay = { version = "0.7", default-features = false, features = ["backend_winit", "renderer_gl"] }
# 或考虑迁移到纯wgpu

# 移除
wayland-server = "❌"
wayland-protocols = "❌"
wayland-protocols-wlr = "❌"
```

## 技术细节

### 渲染管道
```
SCP Client Buffer (shmem/dmabuf)
    ↓
ScpState::buffer_manager
    ↓
Compositor Render Pipeline (wgpu/smithay-renderer)
    ↓
winit Window / DRM/GBM
```

### 输入管道
```
winit Event / libinput Event
    ↓
Input Coordinator (compositor/src/input/)
    ↓
ScpState::input_state
    ↓
SCP Input Messages → Client
```

### 关键抽象

**Renderer Trait** (`compositor/src/render/mod.rs`):
```rust
pub trait Renderer {
    fn render_surface(&mut self, surface: &ScpSurface, location: Point) -> Result<()>;
    fn begin_frame(&mut self) -> Result<Frame>;
    fn commit_frame(&mut self, frame: Frame) -> Result<()>;
}
```

**Input Coordinator** (`compositor/src/input/mod.rs`):
```rust
pub struct InputCoordinator {
    keyboard: KeyboardState,
    pointer: PointerState,
    touch: TouchState,
}

impl InputCoordinator {
    pub fn handle_backend_event(&mut self, event: BackendEvent) -> Vec<ScpInputMessage>;
}
```

## 迁移顺序

1. ✅ **Phase 0 (已完成)**: SCP协议实现 (`scp/`目录)
2. **Phase 1.1**: 创建渲染抽象层 (本周)
3. **Phase 1.2**: 创建输入抽象层 (本周)
4. **Phase 2.1**: SCP状态作为primary state (下周)
5. **Phase 2.2**: 移除Wayland handlers (下周)
6. **Phase 3**: 后端适配 (2周)
7. **Phase 4**: 依赖清理和性能优化 (1周)

## 风险与缓解

### 风险1: 渲染管道中断
- **缓解**: 先创建抽象层，逐步迁移，保持两套系统并行运行直到新系统稳定
- **回滚**: Git分支隔离，随时可回退

### 风险2: 输入延迟增加
- **缓解**: Input Coordinator设计为零拷贝，直接转换事件格式
- **监控**: 添加input latency metrics

### 风险3: 客户端兼容性
- **影响**: 所有客户端必须更新到纯SCP
- **缓解**: 目前没有生产客户端，测试客户端同步更新

## 成功标准

- [ ] `cargo build --workspace`无Wayland依赖
- [ ] `cargo run -p sol-compositor` 启动纯SCP会话
- [ ] SCP测试客户端完整往返（surface创建、buffer attach、input、configure）
- [ ] 性能不低于当前baseline (60fps, <16ms frame time)
- [ ] `compositor/tests/sol_session.rs`全部通过

## 参考

- ADR-0028: No Wayland Compatibility
- CLAUDE.md: "Platform security - Capability model enforced by the OS"
- [Android's SurfaceFlinger](https://source.android.com/docs/core/graphics/surfaceflinger-windowmanager) - 类似的"非X11/Wayland"架构
