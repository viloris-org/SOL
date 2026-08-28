# SOL Login Screen - Quick Start Guide

## 快速开始

### 开发模式（推荐用于测试）

```bash
# 构建
cargo build -p sol-logind

# 运行（模拟用户 + 桩认证）
cargo run -p sol-logind -- --dev
```

**开发模式特性：**
- 使用 3 个模拟用户（John Appleseed, Jane Smith, Administrator）
- 任何密码都能成功登录
- 不需要 root 权限
- 快速迭代开发

### 生产模式（真实认证）

```bash
# 构建
cargo build -p sol-logind --release

# 运行（需要 root 权限）
sudo ./target/release/sol-logind
```

**生产模式特性：**
- 从 /etc/passwd 读取真实系统用户
- 使用 PAM 进行密码验证
- 只显示 UID 1000-65534 的普通用户
- 需要输入正确的系统密码

## 功能演示

### 1. 用户选择
- 点击圆形头像选择用户
- 显示用户全名（从 GECOS 字段）
- 支持键盘导航

### 2. 密码输入
- 默认隐藏密码（显示为点）
- 点击眼睛/锁图标切换可见性
- 支持 Enter 键提交

### 3. 登录
- 点击"Log In"按钮
- 或在密码框中按 Enter
- 成功后显示会话信息

## 测试

```bash
# 运行所有测试
cargo test -p sol-logind

# 运行特定测试
cargo test -p sol-logind -- --test full_login_flow

# 查看测试输出
cargo test -p sol-logind -- --nocapture
```

## 调试

### 启用详细日志
```bash
RUST_LOG=debug cargo run -p sol-logind -- --dev
```

### 查看 PAM 认证细节
```bash
RUST_LOG=sol_logind::auth=trace sudo cargo run -p sol-logind
```

## 架构

```
sol-logind
├── src/
│   ├── main.rs          # 入口点 + 事件循环
│   ├── lib.rs           # 服务协调器
│   ├── ui.rs            # UI 状态机
│   ├── render.rs        # Slint 渲染
│   ├── auth.rs          # PAM 认证
│   └── users.rs         # 用户加载
└── tests/
    └── login_flow.rs    # 集成测试
```

## 当前状态

✅ **Phase 1**: Visual-only - 完成  
✅ **Phase 2**: Visual Rendering - 完成  
✅ **Phase 3**: Real Authentication - 完成（新！）  
📋 **Phase 4**: Enhanced Visuals - 待实现  
📋 **Phase 5**: Session Management - 待实现  

## 下一步

1. **UI 错误显示** - 在界面中显示认证错误
2. **加载状态** - 认证期间显示加载动画
3. **头像图像** - 加载真实的用户头像
4. **会话启动** - 成功后启动 compositor 和 shell

## 已知限制

- 生产模式需要 root 权限（PAM 要求）
- 认证错误只记录到日志，UI 中不显示
- 成功后不实际启动用户会话
- 头像只显示首字母，不加载图像

## 故障排除

### 问题：找不到用户
**解决方案：**
```bash
# 检查用户是否存在
grep "^[^:]*:x:1[0-9][0-9][0-9]" /etc/passwd

# 创建测试用户
sudo useradd -m -s /bin/bash testuser
sudo passwd testuser
```

### 问题：PAM 认证失败
**解决方案：**
```bash
# 检查 PAM 配置
cat /etc/pam.d/login

# 确保以 root 运行
sudo cargo run -p sol-logind
```

### 问题：窗口不显示
**解决方案：**
```bash
# 检查 Wayland 环境变量
echo $WAYLAND_DISPLAY

# 确保 compositor 正在运行
ps aux | grep compositor
```

## 更多信息

- [完整文档](./README.md)
- [实现细节](./IMPLEMENTATION.md)
- [UI 设计规范](./UI-DESIGN.md)
- [Phase 2 总结](./PHASE-2-COMPLETE.md)
- [Phase 3 总结](./PHASE-3-COMPLETE.md)
