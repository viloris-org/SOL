# Wayland依赖移除计划

## 当前状况（2026-08-26）

虽然SOL已经定义了独立的SCP (SOL Compositor Protocol)，但compositor实现仍深度依赖Wayland协议栈。

### 已完成的移除

1. **winit的Wayland feature** - compositor现在只使用X11后端进行开发
2. **sol-ui的Slint Wayland后端** - 改用X11后端
3. **测试依赖wayland-client** - 已从dev-dependencies移除

### 当前剩余的Wayland依赖

#### 1. Compositor核心（`compositor/Cargo.toml`）
```toml
smithay = { features = ["wayland_frontend"] }  # 核心框架
wayland-server = "0.31"                         # 直接依赖
wayland-protocols = "0.32"                      # XDG shell等协议
wayland-protocols-wlr = "0.3"                   # Layer shell协议
```

**使用位置**：
- `state.rs` - 使用`wayland_protocols::xdg::shell`和`wayland_server::protocol`
- `grabs.rs` - 使用`wayland_protocols::xdg::shell::server::xdg_toplevel::ResizeEdge`
- `window.rs`, `outputs.rs` - 使用`wayland_server::protocol`类型
- `main.rs` - 使用smithay的Wayland基础设施

**问题**：compositor的窗口管理逻辑仍然使用Wayland协议的类型（`WlSurface`, `xdg_toplevel`等）。

#### 2. Shell（`shell/Cargo.toml`）
```toml
wayland-client = "0.31"
wayland-protocols = "0.32"
wayland-protocols-wlr = "0.3"
```

**使用位置**：
- `main.rs` - 实现Wayland客户端，使用layer-shell协议
- `client.rs` - 连接到compositor的Wayland socket

**问题**：shell当前作为Wayland客户端连接到compositor，而非使用SCP协议。

## 为什么不能立即移除

### 技术债务

1. **Smithay深度集成**：compositor使用smithay的以下组件：
   - 窗口管理（`CompositorHandler`, `SurfaceData`）
   - 输入处理（`SeatHandler`, 键盘/指针/触摸）
   - 输出管理（`OutputHandler`）
   - XDG shell实现（toplevel, popup）
   - Layer shell实现（shell顶栏/dock）

2. **协议类型耦合**：代码中大量使用Wayland协议类型：
   - `WlSurface` - 表面类型
   - `xdg_toplevel::ResizeEdge` - 调整大小边缘
   - `WlOutput` - 输出设备
   - smithay的各种trait和handler

3. **Shell未迁移到SCP**：shell仍然使用Wayland layer-shell协议与compositor通信。

## 移除路线图

### Phase 1: SCP协议完善（当前）
- [x] 定义SCP协议消息（`scp/protocol.rs`）✅
- [x] 实现基础SCP传输层（`scp/transport.rs`）✅
- [x] 实现能力系统（`scp/capability.rs`）✅
- [ ] **SCP协议完整性验证**
  - [ ] 确保所有Wayland XDG shell功能都有SCP等价物
  - [ ] 确保layer shell功能映射到SCP
  - [ ] 确保输入/输出管理完整

### Phase 2: Compositor内部解耦（待定）
需要将compositor从smithay的Wayland类型迁移到纯SCP实现：

1. **定义内部类型**：
   - 替代`WlSurface`的SOL内部surface类型
   - 替代`xdg_toplevel`的SOL toplevel类型
   - 独立的window/layer/popup管理

2. **重构Window Manager**：
   - `window.rs` - 使用SCP类型而非Wayland类型
   - `grabs.rs` - 使用SCP resize edge enum

3. **重构State**：
   - `state.rs` - 从`CompositorHandler`等smithay trait解耦
   - 使用纯SCP协议状态机

4. **保留的smithay组件**（非Wayland相关）：
   - `backend_winit` / `backend_udev` - 后端抽象
   - `renderer_gl` - OpenGL渲染
   - `backend_libinput` - 输入设备处理
   - 这些是纯系统级抽象，不涉及Wayland协议

### Phase 3: Shell迁移到SCP（待定）
1. **实现SCP客户端库**：
   - 创建`sol-scp-client` crate
   - 包装`scp/transport.rs`和`scp/protocol.rs`

2. **Shell使用SCP**：
   - 替换`wayland-client`使用为`sol-scp-client`
   - 使用SCP layer-surface消息而非wlr-layer-shell
   - 通过SCP socket连接compositor

3. **移除shell的Wayland依赖**：
   ```toml
   # 删除：
   # wayland-client = "0.31"
   # wayland-protocols = "0.32"
   # wayland-protocols-wlr = "0.3"
   
   # 添加：
   # sol-scp-client = { path = "../compositor/scp-client" }
   ```

### Phase 4: Compositor最终清理（待定）
一旦compositor内部完全使用SCP类型：

1. **移除smithay的wayland_frontend feature**：
   ```toml
   smithay = { 
     version = "0.7", 
     default-features = false, 
     features = ["backend_winit", "backend_udev", "renderer_gl"]
     # 移除: "wayland_frontend"
   }
   ```

2. **移除直接的Wayland依赖**：
   ```toml
   # 全部删除：
   # wayland-server = "0.31"
   # wayland-protocols = "0.32"
   # wayland-protocols-wlr = "0.3"
   ```

3. **最终状态**：compositor只保留：
   - smithay的后端和渲染功能（非协议相关）
   - 纯SCP协议实现
   - 无Wayland协议代码

## 风险和考虑

### 1. 工作量巨大
Smithay提供的不仅是协议绑定，还有大量窗口管理逻辑。重新实现需要：
- 完整的surface生命周期管理
- XDG shell语义（toplevel states, configure flow）
- Layer shell语义（anchors, exclusive zones）
- Popup定位算法
- 输入焦点管理

### 2. 兼容性考虑
当前测试（`compositor/tests/sol_session.rs`）仍使用Wayland协议。需要：
- 将所有测试迁移到SCP协议（已有`scp_session.rs`作为模板）
- 确保SCP协议100%覆盖所需功能

### 3. 开发便利性
Smithay的Wayland支持使我们能：
- 在开发时使用现有Wayland工具（如weston-terminal）测试
- 逐步迁移而非big-bang重写

**建议**：在Phase 2之前保留Wayland支持作为可选feature，用于对比测试。

## 优先级评估

根据ADR-0028（No Wayland Compatibility Layer），这是架构一致性要求，但不是紧急阻塞项。

**推荐优先级**：Phase 1完成后 → Phase 2&3 → Phase 4

**当前可接受状态**：
- SCP协议已定义且可工作（见`scp_session.rs`测试）
- Wayland依赖仅存在于实现层，未暴露给应用
- 开发工具链不依赖Wayland（已改用X11）

## 参考
- [ADR-0028](../decisions/0028-drop-wayland-compatibility.md) - 不提供Wayland兼容层的决策
- `compositor/src/scp/protocol.rs` - SCP协议定义
- `compositor/tests/scp_session.rs` - 纯SCP测试示例
