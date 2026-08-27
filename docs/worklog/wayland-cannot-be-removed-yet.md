# Wayland依赖为何暂时无法移除

## 实验结果（2026-08-26）

尝试直接移除所有Wayland依赖（smithay的`wayland_frontend` feature + wayland-server/protocols）：

```bash
# 移除后编译结果
cargo check -p sol-compositor
# ❌ 58个编译错误
```

## 核心问题

**Compositor的整个实现架构建立在smithay的Wayland基础设施之上**。

### 依赖的smithay Wayland组件

1. **协议处理器（Handlers）**：
   - `CompositorHandler` - surface生命周期
   - `XdgShellHandler` - toplevel窗口
   - `WlrLayerShellHandler` - shell layer surfaces
   - `ShmHandler` - 共享内存buffer
   - `SeatHandler` - 输入设备
   - `DataDeviceHandler` - 剪贴板/拖放
   - `OutputHandler` - 显示输出
   - `FractionalScaleHandler` - HiDPI支持
   - `InputMethodHandler` - 输入法

2. **协议类型**：
   - `WlSurface` - 表面对象
   - `xdg_toplevel::ResizeEdge` - 窗口调整大小
   - `WlOutput` - 输出设备
   - `Client`, `Resource` - 客户端连接管理

3. **委托宏（Delegate Macros）**：
   ```rust
   delegate_compositor!
   delegate_xdg_shell!
   delegate_layer_shell!
   delegate_data_device!
   delegate_seat!
   delegate_output!
   // ... 等等
   ```
   这些宏生成大量Wayland协议样板代码

4. **渲染集成**：
   - `surface::draw_render_elements` - surface渲染
   - `on_commit_buffer_handler` - buffer提交处理
   - `with_states` - surface状态访问

### 受影响的文件

所有核心compositor文件都深度依赖：
- `state.rs` - 核心状态和所有handler实现
- `window.rs` - 窗口管理（使用`WlSurface`）
- `grabs.rs` - 交互抓取（使用Wayland类型）
- `outputs.rs` - 输出管理（使用`DisplayHandle`, `WlOutput`）
- `main.rs` - 事件循环（使用Wayland display）
- `udev_runtime.rs` - udev后端（使用Wayland display）

## 为什么不是简单替换

### 错误认识
❌ "只是把Wayland类型替换成SCP类型"

### 实际情况
需要**重新实现整个compositor**：

1. **协议处理**：
   - Smithay的handlers不只是类型定义，包含完整的协议语义
   - XDG shell的configure流程
   - Layer shell的定位和exclusive zone计算
   - Popup的约束调整算法

2. **状态管理**：
   - Smithay管理surface状态、buffer附加、damage追踪
   - 客户端连接生命周期
   - 协议对象ID映射

3. **渲染集成**：
   - Surface到render element的转换
   - Damage优化
   - Buffer release时机

4. **输入处理**：
   - Seat和input device管理
   - 焦点管理和事件分发
   - 键盘映射（XKB）管理

## 工作量估算

基于58个编译错误和代码分析：

- **文件数量**：6个核心文件需要大规模重写
- **代码行数**：约5000+行需要重构
- **复杂度**：需要理解并重新实现Wayland协议语义
- **时间估算**：2-4周全职工作

## 已完成的可移除部分

✅ **开发工具层**（已完成）：
- winit不再需要Wayland feature（改用X11）
- slint不再需要Wayland backend（改用X11）
- 测试不再使用wayland-client（改用SCP）

这些改动已完成且有效。

## 技术路径

### 方案A：完全重写（不推荐现在做）
1. 定义SOL内部类型（替代`WlSurface`等）
2. 实现SCP协议处理器（替代smithay handlers）
3. 实现窗口管理逻辑（替代smithay状态机）
4. 实现渲染集成
5. 移除smithay的`wayland_frontend`

**优点**：完全控制，无Wayland依赖  
**缺点**：巨大工作量，高风险，短期无收益

### 方案B：保持现状（推荐）
保留smithay的Wayland基础设施作为**内部实现细节**：
- 应用层使用SCP协议（已实现）
- Compositor内部使用smithay/Wayland（当前状态）
- 两者通过adapter层连接

**优点**：
- 零额外工作
- 稳定可靠
- 符合ADR-0028精神（应用不接触Wayland）

**缺点**：
- 依赖树包含Wayland
- 无法说"完全无Wayland依赖"

### 方案C：渐进迁移（未来可选）
等Phase 2-3稳定后，逐步替换：
1. 先替换一个简单handler（如OutputHandler）
2. 验证可行性
3. 逐步扩展
4. 保留Wayland作为可选feature用于对比测试

## 结论

**现在不能砍掉Wayland依赖**。原因：

1. **技术上不可行**：compositor完全建立在smithay的Wayland基础设施上
2. **工作量巨大**：需要重写整个compositor核心（数周工作）
3. **风险高**：重新实现复杂协议语义容易引入bug
4. **收益低**：应用层已经是纯SCP，内部实现细节不影响产品定位

**当前最佳做法**：
- ✅ 保持Wayland依赖在实现层
- ✅ 应用层使用纯SCP（已做到）
- ✅ 开发工具不依赖Wayland（已完成）
- ✅ 对外宣称"SOL使用SCP协议，不兼容Wayland应用"（真实）

**未来考虑重写的时机**：
- Phase 3完全稳定
- 有专门的开发资源（2-4周）
- 需要深度定制compositor行为
- Smithay项目出现重大问题

## 类比

这类似于：
- **Android**: 应用用Android API，但底层使用Linux syscalls
- **Chrome OS**: 应用用Chrome Apps API，但底层用Linux
- **SOL**: 应用用SCP协议，但compositor内部用smithay/Wayland

内部实现不影响平台定位。
