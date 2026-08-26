# SCP 依赖减少总结

## 目标
减少 SOL Compositor Protocol (SCP) 的外部依赖，用自实现替换可以替换的部分。

## 移除的依赖

### 1. `rand` (0.8)
**用途**: 生成安全随机数用于 capability token  
**替换**: `src/scp/random.rs` - 直接调用 `getrandom(2)` syscall

### 2. `nix` (0.30)
**用途**: Unix socket 操作（SCM_RIGHTS, SO_PEERCRED, pipe, close）  
**替换**: `src/scp/unix_socket.rs` - 直接使用 `libc` 调用
- `recvmsg_with_fds()` - 接收带文件描述符的消息
- `get_peer_credentials()` - 获取对端 PID/UID/GID
- `create_pipe()` - 创建管道
- `close_fd()` - 关闭文件描述符

### 3. `memfd` (0.6)
**用途**: 创建密封的内存文件描述符  
**替换**: `src/scp/memfd.rs` - 直接调用 `memfd_create(2)` syscall
- `create()` - 创建 memfd
- `seal_readonly()` - 密封为只读

### 4. `toml` (0.8)
**用途**: 解析应用 manifest 文件  
**替换**: `src/scp/toml_parser.rs` - 基础 TOML 解析器
- 支持基本键值对、布尔值、整数、字符串
- 支持 `[section]` 和 `[section.subsection]` 嵌套表
- 足以处理 SOL manifest 格式

## 保留的依赖

### `serde` + `serde_json`
序列化/反序列化协议消息。这是零成本抽象且广泛使用，自实现收益不大。

### `thiserror`
错误处理宏。编译时依赖，不影响运行时二进制大小。

### `libc`
系统调用接口。无法避免，是所有自实现的基础。

## 影响

### 依赖减少
- **移除**: 4 个运行时依赖 (rand, nix, memfd, toml)
- **保留**: 3 个必要依赖 (serde, serde_json, libc) + 1 个编译时依赖 (thiserror)
- **新增**: 5 个自实现模块，总计约 500 行代码

### 测试结果
所有测试通过（38 个测试），包括：
- 随机数生成测试
- Unix socket 凭证测试
- memfd 创建和密封测试
- TOML 解析测试（基础和嵌套）
- SCP 协议完整性测试

### 编译验证
```bash
$ cargo build -p sol-compositor
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 5.82s

$ cargo test -p sol-compositor --lib
test result: ok. 38 passed; 0 failed; 0 ignored
```

## 新增文件

1. `compositor/src/scp/random.rs` - 密码学安全随机数生成
2. `compositor/src/scp/unix_socket.rs` - Unix socket 辅助函数
3. `compositor/src/scp/memfd.rs` - 内存文件描述符支持
4. `compositor/src/scp/toml_parser.rs` - 最小 TOML 解析器

## 修改的文件

- `compositor/Cargo.toml` - 移除 4 个依赖
- `compositor/src/scp/mod.rs` - 添加新模块导出
- `compositor/src/scp/security.rs` - 使用 `random::fill_bytes()`
- `compositor/src/scp/transport.rs` - 使用 `unix_socket::*`
- `compositor/src/scp/state.rs` - 使用 `unix_socket::close_fd()` 和 `create_pipe()`
- `compositor/src/scp/buffer.rs` - 使用 `unix_socket::close_fd()`
- `compositor/src/scp/surface.rs` - 使用 `unix_socket::close_fd()`
- `compositor/src/scp/keymap.rs` - 使用 `memfd::create()` 和 `seal_readonly()`
- `compositor/src/scp/manifest.rs` - 使用 `toml_parser::parse()`

## 优势

1. **减少依赖树**: 移除了 4 个 crate 及其传递依赖
2. **更小的二进制**: 减少链接的代码量
3. **更快的编译**: 更少的依赖需要编译
4. **更好的控制**: 完全控制关键安全功能的实现
5. **平台特定优化**: 直接针对 Linux 优化（SCP 只支持 Linux）

## 权衡

- **维护成本**: 需要维护自实现的代码
- **功能范围**: TOML 解析器只支持 manifest 需要的子集
- **跨平台**: 自实现直接依赖 Linux syscalls（符合 SOL 的 Linux-only 定位）
