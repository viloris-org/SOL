# sol-networkd 改进总结

## 完成的改进

### 1. Profile 持久化存储
- **实现位置**: `src/profile/mod.rs`
- **功能**:
  - Profile 自动保存到 `/var/lib/sol-networkd/profiles/*.json`
  - 启动时自动加载所有已保存的配置
  - 支持增删改查操作，所有变更实时持久化
  - 添加了 Serialize/Deserialize 支持

### 2. 完整的以太网设备实现
- **实现位置**: `src/device/ethernet.rs`
- **功能**:
  - 支持 DHCP 和静态 IP 配置
  - 自动配置网络接口（IP、子网掩码、网关）
  - DNS 服务器配置
  - 网络载波检测（检测网线是否插入）
  - 链路速度读取（1000Mbps 等）
  - 从 sysfs 读取 MAC 地址

### 3. 网络配置自动应用
- **实现位置**: `src/device/ethernet.rs`
- **功能**:
  - DHCP 租约获取后自动配置接口
  - 使用 `ip` 命令配置地址和路由
  - 子网掩码自动转换为 CIDR 前缀长度
  - 默认路由自动添加

### 4. 完整的连接流程
- **实现位置**: `src/manager.rs`
- **功能**:
  - WiFi 连接：扫描 → 加密密码 → 创建 Profile → 连接
  - 以太网连接：选择设备 → 应用 IP 配置 → 启动接口
  - VPN 连接：框架已就绪（待完整实现）
  - 连接状态管理（Disconnected → Connecting → Connected）

### 5. WiFi 快速连接 API
- **实现位置**: `src/manager.rs::connect_wifi_quick()`
- **功能**:
  - 单次调用完成 WiFi 连接
  - 自动判断安全类型（Open/WPA2）
  - 自动创建和保存 Profile
  - 密码自动加密存储

### 6. 增强的密钥派生
- **实现位置**: `src/security.rs`
- **功能**:
  - 使用 PBKDF2 从机器 ID 派生主密钥
  - 随机盐存储在 `/var/lib/sol-networkd/salt`
  - AES-256-GCM 加密 WiFi 密码
  - 基于硬件绑定的密钥（使用 /etc/machine-id）

### 7. 网络时间同步 (NTS)
- **实现位置**: `src/nts.rs`
- **功能**:
  - 连接成功后自动同步时间
  - 支持 systemd-timesyncd 和 chrony
  - 默认使用 Cloudflare NTS 服务器
  - 时间同步状态检查

### 8. 扩展的 D-Bus 接口
- **实现位置**: `src/dbus/manager.rs`
- **新增方法**:
  - `scan_wifi()` - 扫描可用 WiFi 网络
  - `connect_wifi(ssid, passphrase)` - 快速连接 WiFi
  - `wifi_signal_strength()` - 获取当前信号强度
  - `active_connection()` - 获取活动连接详情
  - `list_profiles()` - 列出所有保存的配置
  - `set_auto_connect()` - 设置自动连接（待实现）

### 9. 连接信息查询
- **实现位置**: `src/manager.rs::get_active_connection_info()`
- **功能**:
  - 返回当前连接类型（WiFi/Ethernet）
  - 接口名称
  - 连接详情（SSID/链路速度）
  - WiFi 信号强度

## 架构改进

### 数据流
```
用户请求 → D-Bus API → NetworkManager → Device (WiFi/Ethernet/VPN)
                                    ↓
                            ProfileStore (持久化)
                                    ↓
                            SecretStore (加密)
```

### 状态管理
- 网络状态：Disconnected → Connecting → Connected
- 连接成功后自动触发：
  1. 时间同步（NTS）
  2. 连通性检测（Captive Portal）
  3. DNS 配置

### 安全性
- 密码加密存储（AES-256-GCM）
- 基于机器 ID 的密钥派生
- WiFi 凭据不以明文保存

## 使用示例

### 通过 D-Bus 连接 WiFi
```bash
# 扫描网络
busctl call org.sol.Network1 /org/sol/Network1 org.sol.Network1.Manager ScanWifi

# 连接到 WiFi
busctl call org.sol.Network1 /org/sol/Network1 org.sol.Network1.Manager ConnectWifi ss "MyWiFi" "password123"

# 查看当前连接
busctl call org.sol.Network1 /org/sol/Network1 org.sol.Network1.Manager ActiveConnection

# 获取信号强度
busctl call org.sol.Network1 /org/sol/Network1 org.sol.Network1.Manager WiFiSignalStrength
```

### 编程接口
```rust
// 扫描 WiFi
let networks = manager.scan_wifi().await?;

// 快速连接
let profile_id = manager.connect_wifi_quick(
    "MyNetwork".to_string(),
    Some("password".to_string())
).await?;

// 查询连接状态
if let Some(info) = manager.get_active_connection_info().await? {
    println!("Connected via {}: {}", info.connection_type, info.details);
}
```

## 待完善功能

1. **VPN 完整实现** - WireGuard/OpenVPN 连接逻辑
2. **Netlink 事件监听** - 实时网络变化通知
3. **自动连接策略** - 根据优先级/位置自动连接
4. **漫游支持** - WiFi AP 间无缝切换
5. **计量连接管理** - 避免在计量网络上的大流量操作
6. **IPv6 支持** - DHCPv6 和 SLAAC
7. **热点模式** - AP 模式支持

## 测试建议

1. **单元测试**:
   - Profile 持久化读写
   - 密码加密/解密
   - DHCP 租约解析

2. **集成测试**:
   - WiFi 扫描和连接
   - 以太网 DHCP 配置
   - Profile 创建和删除

3. **系统测试**:
   - 在真实硬件上测试 WiFi 连接
   - 以太网热插拔
   - 网络切换场景

## 依赖要求

运行时依赖：
- `iwd` - Intel Wireless Daemon (WiFi 管理)
- `systemd-timesyncd` 或 `chrony` - 时间同步
- `ip` 命令 - 网络配置

构建依赖：
- Rust 1.70+
- 标准 Linux 内核头文件
