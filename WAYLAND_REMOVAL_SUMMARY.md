# Wayland 依赖移除总结

## 当前结果（2026-08-28）

- `sol-compositor` 已删除 Smithay、Wayland server/protocol、winit、xkbcommon、
  xcursor 和 DRM Smithay adapter 依赖。
- `sol-shell` 已删除 Wayland client/protocol/wlr-layer-shell 依赖，并改用 SCP
  capability + layer-surface 往返。
- `sol-session` 统一传播 `SOL_SCP_SOCKET`，不再设置 `WAYLAND_DISPLAY` 或
  `SOL_WAYLAND_SOCKET`。
- Cargo 默认工作区依赖树及 `Cargo.lock` 中已无 Wayland/Smithay/wlroots 包。
- 旧 Wayland 源码、客户端示例、集成测试和 toolkit 兼容 fixture 已从工作树
  删除，只保留于 Git 历史和历史 ADR。
- `scripts/validate-scp-only.sh` 同时守卫 Cargo 依赖图、manifest、活动 session
  环境变量以及已退役路径，防止兼容层意外回归。

## 验证

```bash
./scripts/validate-scp-only.sh

cargo test -p sol-compositor
cargo test -p sol-session
cargo test -p sol-logind
```

## 仍待实现

依赖移除不等于图形栈完成。原 Smithay 后端同时承担了渲染、输入、输出与
DRM/KMS glue；这些能力需要围绕 SCP-owned state 重新接入。当前可验证的是
协议、权限、对象生命周期和 headless 传输，尚不能宣称已有可见桌面或真实
硬件 session。
