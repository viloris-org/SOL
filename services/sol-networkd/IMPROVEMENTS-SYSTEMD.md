# 基于 systemd-networkd 的网络服务完善

参考 systemd-networkd 的实现模式，对 sol-networkd 进行了以下完善。

## 完善内容

### 1. 扩展的 Netlink 事件处理

**参考**: `systemd/src/network/networkd-manager.c` 的事件订阅机制

**改进**:
- 扩展 `NetlinkEvent` 枚举以支持更多事件类型：
  - `LinkChanged` - 链路属性变更
  - `NewNeighbor` / `DelNeighbor` - 邻居表变更
  - `NewRule` / `DelRule` - 路由规则变更
  - `NewRoute` / `DelRoute` - 增强的路由事件（包含目的地、网关、接口信息）
  - `NewAddress` / `DelAddress` - 增强的地址事件（包含前缀长度）

- 订阅更多 netlink 组：
  ```rust
  RTMGRP_LINK | RTMGRP_IPV4_IFADDR | RTMGRP_IPV6_IFADDR |
  RTMGRP_IPV4_ROUTE | RTMGRP_IPV6_ROUTE | RTMGRP_NEIGH |
  RTMGRP_IPV4_RULE
  ```

**文件**: `services/sol-networkd/src/netlink/mod.rs`

### 2. 设备状态增强

**参考**: `systemd/src/network/networkd-link.h` 的 `Link` 结构

**改进**:
- 扩展 `Device` 结构：
  ```rust
  pub struct Device {
      pub ifindex: u32,           // 接口索引
      pub hw_address: Option<String>,  // MAC 地址
      pub mtu: Option<u32>,       // MTU
      pub carrier: bool,          // 载波状态
      pub ip_addresses: Vec<IpAddr>,  // IP 地址列表
  }
  ```

- 添加设备状态管理方法：
  - `set_state()` - 状态转换，返回是否变更
  - `is_available()` - 检查设备是否可用
  - `is_connected()` - 检查设备是否已连接

- 为 `DeviceState` 添加详细文档注释

**文件**: `services/sol-networkd/src/device/mod.rs`

### 3. 状态文件持久化

**参考**: `systemd/src/network/networkd-state-file.c`

**新增**: `services/sol-networkd/src/state_file.rs`

实现运行时状态持久化，类似 systemd 的 `/run/systemd/netif/state`：

```rust
pub struct StateFile {
    pub operational_state: OperationalState,  // 全局运行状态
    pub carrier_state: CarrierState,          // 载波状态
    pub address_state: AddressState,          // 地址状态
    pub online_state: OnlineState,            // 在线状态
    pub links: Vec<LinkState>,                // 链路状态列表
}
```

**状态枚举**:
- `OperationalState`: Off | NoCarrier | Dormant | DegradedCarrier | Carrier | Degraded | Routable
- `CarrierState`: Off | NoCarrier | Dormant | DegradedCarrier | Carrier
- `AddressState`: Off | Degraded | Routable
- `OnlineState`: Offline | Partial | Online

**功能**:
- 加载/保存状态到 `/run/sol-networkd/state`
- 原子写入（通过临时文件）
- 自动聚合链路状态到全局状态
- JSON 序列化格式

### 4. 配置请求队列

**参考**: `systemd/src/network/networkd-queue.c`

**新增**: `services/sol-networkd/src/queue.rs`

实现配置操作的队列化处理：

```rust
pub enum Request {
    ConfigureAddress { ifindex, address, prefix_len },
    ConfigureRoute { ifindex, destination, gateway },
    SetLinkUp { ifindex },
    SetLinkDown { ifindex },
    ConfigureDns { ifindex, servers },
    ActivateConnection { profile_id, device_id },
    DeactivateConnection { device_id },
}
```

**特性**:
- 异步请求队列
- 支持高优先级请求（`enqueue_front`）
- 批量处理（`process_all`）
- 处理状态跟踪

### 5. 管理器增强

**参考**: `systemd/src/network/networkd-manager.c` 的整体架构

**改进**:

#### 设备索引追踪
```rust
struct NetworkManagerInner {
    devices: HashMap<DeviceId, Device>,
    devices_by_ifindex: HashMap<u32, DeviceId>,  // 新增
    request_queue: RequestQueue,                  // 新增
    state_file: StateFile,                        // 新增
}
```

#### 完整的事件处理
- `handle_link_up()` - 链路上线，更新载波状态
- `handle_link_down()` - 链路下线，设为不可用
- `handle_link_changed()` - 链路属性变更
- `handle_new_address()` - 地址添加，更新设备 IP 列表
- `handle_del_address()` - 地址删除
- `handle_new_route()` / `handle_del_route()` - 路由变更跟踪
- `save_state()` - 事件处理后保存状态

#### 设备状态映射
新增 `device_state_to_operational()` 函数，将设备状态映射到运行状态：
- `Unavailable` → `Off`
- `Disconnected` → `NoCarrier`
- `Preparing/Configuring/NeedAuth` → `Dormant`
- `IpConfig` → `DegradedCarrier`
- `IpCheck` → `Carrier`
- `Active` → `Routable`

**文件**: `services/sol-networkd/src/manager.rs`

## 架构对比

### systemd-networkd
```
Manager (state, rtnl, devices, networks)
  ├─ netlink event loop
  ├─ link state machines
  ├─ configuration queue
  └─ state file persistence
```

### sol-networkd (现在)
```
NetworkManager (state, devices, profiles)
  ├─ netlink monitor (扩展事件)
  ├─ device state tracking (增强)
  ├─ request queue (新增)
  └─ state file (新增)
```

## 与 systemd-networkd 的差异

### 保留的差异（设计决策）

1. **语言**: Rust vs C
   - Rust 提供内存安全和并发保证
   - 使用 `tokio` 进行异步处理

2. **协议**: SCP vs D-Bus
   - SOL 使用自己的 compositor protocol
   - D-Bus 仅用于管理接口

3. **配置格式**: JSON profiles vs .network 文件
   - JSON 更易于程序化操作
   - Profile 存储在 `/var/lib/sol-networkd/profiles/`

4. **WiFi 后端**: iwd vs wpa_supplicant
   - 选择 iwd 以获得更现代的架构

### 未来可能采纳的特性

从 systemd-networkd 中可以进一步借鉴：

1. **网络文件热重载** - `networkd-network.c` 的配置重载机制
2. **QDisc/TClass 管理** - Traffic Control 支持
3. **SR-IOV 配置** - 虚拟化网络支持
4. **DHCP 服务器模式** - 不仅是客户端
5. **LLDP 支持** - 链路层发现协议

## 测试

确保编译通过：
```bash
cargo check -p sol-networkd
```

运行测试（需要添加）：
```bash
cargo test -p sol-networkd
```

## 后续工作

1. **实现请求处理器** - 当前 `run_request_processor()` 已声明但未实现
2. **完善状态持久化** - 在关键操作后调用 `save_state()`
3. **添加集成测试** - 测试完整的事件处理流程
4. **性能优化** - 减少不必要的状态保存
5. **监控接口** - 通过 D-Bus 导出状态信息

## 参考

- systemd-networkd 源码: `/home/rownix/systemd/src/network/`
- 关键文件：
  - `networkd-manager.c` - 主管理器和事件循环
  - `networkd-link.c` - 链路状态机
  - `networkd-queue.c` - 配置队列
  - `networkd-state-file.c` - 状态持久化
  - `networkd-address.c` - 地址管理
  - `networkd-route.c` - 路由管理
