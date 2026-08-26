# SOL Compositor Protocol (SCP) 设计总结

我为 SOL 设计了一个权限最小化的 compositor 协议来替代 Wayland。以下是关键设计：

**重要更新（2026-08-26）**：SOL 将**不维护 Wayland 兼容层**。SOL 是 Linux Family OS（类似 Android），而非 Linux 发行版（类似 Ubuntu）。完整理由见 ADR-0028。

## 核心设计原则

### 1. **能力导向的安全模型**
- 每个敏感操作（屏幕截图、剪贴板、全局快捷键）都需要显式授权
- 授权由 `sol-securityd` 签发 token，compositor 验证
- 所有能力使用都记录审计日志

### 2. **身份优先**
- 每个连接必须认证：PID → AppId
- 应用身份由 app bundle 签名验证（不可伪造）
- Surface 和能力都绑定到认证的 AppId

### 3. **服务端装饰（SSD）**
- **客户端无法绘制标题栏**（防止钓鱼攻击）
- Compositor 拥有所有窗口 chrome（标题栏、关闭按钮）
- 客户端只能设置标题文本，不能控制渲染

## 协议层次

```
┌─────────────────────────────────────┐
│  SCP Core (必需协议)                 │
│  - surface 创建/提交                 │
│  - 缓冲区管理                        │
│  - 基本输入事件                      │
└─────────────────────────────────────┘
              ↓
┌─────────────────────────────────────┐
│  Capability Extensions (需授权)      │
│  - WindowToplevel (默认授予)         │
│  - ScreenCapture (每次需用户确认)    │
│  - GlobalShortcuts (需声明用途)      │
│  - ClipboardRead (需前台焦点)        │
│  - ClipboardWrite (需用户交互)       │
│  - LayerShell (仅 sol-shell)         │
└─────────────────────────────────────┘
```

## 关键安全特性

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

## 实现架构

### 代码结构
```
compositor/src/scp/
├── mod.rs          - 模块入口
├── protocol.rs     - 消息定义（ClientMessage / CompositorMessage）
├── state.rs        - ScpState（核心状态机）
├── surface.rs      - Surface / Toplevel 管理
├── capability.rs   - 能力定义和验证逻辑
└── security.rs     - SecurityCoordinator trait（与 sol-securityd 通信）
```

### 关键类型

**ScpState**：核心 compositor 状态
- 管理所有认证的客户端会话
- 验证能力 token
- 跟踪 surface 所有权
- 与 `sol-securityd` 通信

**ClientSession**：认证的客户端会话
- 存储 AppId（由 sol-securityd 验证）
- 跟踪授予的能力及过期时间
- 记录最近用户交互时间（用于时间敏感能力）
- 前台/后台状态（用于隐私保护）

**Capability**：能力枚举
```rust
pub enum Capability {
    WindowToplevel,
    ScreenCapture { scope: CaptureScope },
    GlobalShortcuts,
    ClipboardRead,
    ClipboardWrite,
    LayerShell,  // 仅 sol-shell
    Fullscreen,
}
```

## 迁移路径（已修订 - 见 ADR-0028）

### Phase 1：纯 SCP（立即执行）
- Compositor **仅支持 SCP**
- 移除所有 Smithay/Wayland 依赖
- 开发期通过 winit backend 在宿主 Wayland session 运行
- 所有测试客户端使用原生 SCP 协议

### Phase 2：原生渲染管道
- 移除 Smithay 渲染抽象
- 直接实现 DRM/GBM compositor
- SCP-aware 的 surface 生命周期管理

### Phase 3：生产化
- 正式文档："SOL 不兼容 Wayland 应用"
- 发布第三方开发者迁移指南（GTK→SolKit, Qt→SolKit）
- 通过 sol-runtime SDK 封装所有平台能力

### 产品定位

SOL 是 **Linux Family OS**（类似 Android/Chrome OS），而非 Linux 发行版：

| 维度 | Linux 发行版 | SOL (Linux Family) |
|---|---|---|
| 兼容性目标 | 运行现有应用 | 定义新应用模型 |
| 协议 | Wayland/X11 | SCP only |
| 应用打包 | .deb/.rpm + 依赖 | .app bundle（vendored） |
| 安全模型 | DAC + 可选 AppArmor | 基于能力的强制模型 |
| UI 一致性 | 工具包依赖 | 框架强制 |

## 与现有架构整合

### sol-securityd 职责
```rust
trait SecurityCoordinator {
    fn verify_app_identity(pid: u32) -> Option<AppId>;
    fn evaluate_capability(app_id: &AppId, cap: Capability) -> Decision;
    fn issue_token(app_id: &AppId, cap: Capability) -> Token;
    fn verify_token(token: &Token) -> Option<(AppId, Capability)>;
    fn audit_capability_use(app_id: &AppId, cap: Capability, outcome: Outcome);
}
```

### sol-runtime 封装
应用通过 `sol-app` SDK 使用高层 API，不直接接触 SCP 协议：
```rust
// 应用代码
let window = app.create_window("My Window")?;  // sol-app 处理能力请求
window.show()?;
```

## 示例协议流程

```
1. Client → Compositor: Connect { app_id, pid }
2. Compositor → sol-securityd: verify_app_identity(pid)
3. sol-securityd → Compositor: verified AppId
4. Compositor → Client: Connected { session_id, granted_capabilities }
5. Client → Compositor: CreateSurface { surface_id }
6. Client → Compositor: CreateToplevel { surface_id, token, title }
7. Compositor → Client: ConfigureToplevel { size, decoration_height, ... }
8. Client → Compositor: Commit { surface_id }
9. Compositor: 渲染 frame（包括 compositor 绘制的标题栏）
```

## 文档产出

已创建：
- `docs/decisions/ADR-0027-sol-compositor-protocol.md` - 完整 ADR
- `compositor/src/scp/` - 协议实现骨架（~500 行）
- `compositor/examples/scp-client.rs` - 示例客户端展示协议流程

## 下一步

1. **原生渲染接入**：让 compositor 渲染 SCP surface，并逐步删除 Smithay/Wayland 状态
2. **协议演进**：加入版本协商并将临时 JSON wire format 替换为 protobuf
3. **sol-securityd IPC**：实现与安全守护进程的真实通信（当前是进程内 stub）
4. **Shell 与输入**：补齐 SCP popup/layer-shell/input 生命周期
5. **sol-app SDK**：封装 SCP 为高层 Rust API，隐藏协议细节

当前实现已具备原生 Unix socket、长度前缀帧、`SO_PEERCRED` PID 校验、
`SCM_RIGHTS` buffer FD 传递，以及 connect → surface → toplevel 的端到端测试。

这个设计实现了"权限最小化"目标，并通过放弃 Wayland 兼容（ADR-0028）显著简化了架构，让 SOL 成为真正的新平台而非 Linux 桌面替代品。
