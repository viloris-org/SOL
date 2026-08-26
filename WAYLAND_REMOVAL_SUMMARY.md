# Wayland依赖移除总结

## 已完成的改动（2026-08-26）

### 1. 移除开发工具的Wayland依赖

**sdk/sol-ui/Cargo.toml**:
```diff
-slint = { features = ["backend-winit-wayland", "renderer-winit-software"] }
+slint = { features = ["backend-winit-x11", "renderer-winit-software"] }
```
- Slint UI框架现在使用X11后端而非Wayland
- 不影响最终产品（SOL运行时不依赖Wayland/X11）

**compositor/Cargo.toml**:
```diff
-winit = { features = ["wayland", "x11", "rwh_06"] }
+winit = { features = ["x11", "rwh_06"] }

[dev-dependencies]
-wayland-client = "0.31.15"
```
- 开发模式compositor窗口仅使用X11后端
- 移除测试用的wayland-client依赖

### 2. 编译验证
```bash
cargo check --workspace
# ✅ 成功通过，无错误
```

## 剩余的Wayland依赖及原因

### Compositor (核心实现依赖)
```toml
smithay = { features = ["wayland_frontend"] }
wayland-server = "0.31"
wayland-protocols = "0.32"
wayland-protocols-wlr = "0.3"
```

**原因**: compositor的窗口管理逻辑目前深度集成smithay的Wayland实现：
- 使用`WlSurface`、`xdg_toplevel`等Wayland类型
- 依赖smithay的`CompositorHandler`、`XdgShellHandler`等trait
- Layer shell实现使用wlr-layer-shell协议

**影响**: 这些是实现细节，不影响应用层API（应用使用SCP协议）

### Shell (客户端通信)
```toml
wayland-client = "0.31"
wayland-protocols = "0.32"
wayland-protocols-wlr = "0.3"
```

**原因**: shell当前作为Wayland客户端与compositor通信
**待办**: 需迁移到SCP协议客户端

## 架构符合性

### ✅ 符合ADR-0028
- **应用层**: 无Wayland暴露，纯SCP协议（`compositor/src/scp/`）
- **测试**: 已有纯SCP测试（`compositor/tests/scp_session.rs`）
- **开发工具**: 已移除Wayland依赖，使用X11

### ⚠️ 技术债务
- Compositor实现层仍使用Wayland类型（内部）
- 需要大规模重构以完全移除（见`docs/decisions/wayland-dependency-removal.md`）

## 依赖树对比

### 移除前
```
winit → wayland-client (开发)
slint → wayland-client (UI开发)
compositor tests → wayland-client (测试)
compositor → smithay → wayland-* (核心)
shell → wayland-client (运行时)
```

### 移除后
```
winit → X11 only (开发)
slint → X11 only (UI开发)
compositor tests → 无wayland-client (使用SCP)
compositor → smithay → wayland-* (核心实现仍依赖)
shell → wayland-client (运行时，待迁移)
```

## 下一步工作

完整移除路线图见: `docs/decisions/wayland-dependency-removal.md`

**优先级排序**:
1. ✅ **已完成**: 移除开发工具Wayland依赖
2. **Phase 2**: Compositor内部类型解耦（大重构）
3. **Phase 3**: Shell迁移到SCP客户端
4. **Phase 4**: 完全移除smithay的wayland_frontend

**估算工作量**: Phase 2-4约需数周工作，需要重新实现大量窗口管理逻辑。

## 建议

**当前状态可接受**，原因：
- SCP协议已完整定义且可工作
- Wayland依赖仅存在于实现层（smithay封装）
- 应用开发者看到的是纯SCP API
- 符合"Linux Family OS"定位（应用不直接使用Wayland）

**建议推迟深度重构**，直到：
- Phase 1功能完全稳定
- 有更多SCP客户端应用验证协议完整性
- 确定不需要Wayland作为兼容性过渡方案
