# Lyra Bug Fix Summary - 2026-08-29

## 问题描述 (Problem Description)

Lyra shell 存在一个严重的解析器bug，导致带参数的命令无法正常工作：

```bash
λ ~/Projects/SOL (main) 〉cd docs
Error: Undefined command: docs
```

用户报告：`cd docs` 命令失败，显示 "Undefined command: docs"。

## 根本原因 (Root Cause)

解析器在处理命令参数时，错误地将参数当作新命令来解析：

1. `parse_call()` 方法在解析参数时调用 `parse_primary()`
2. `parse_primary()` 遇到标识符时会递归调用 `parse_call()`
3. 这导致 "docs" 被当作命令而不是 `cd` 的参数

## 解决方案 (Solution)

创建了专用的 `parse_arg()` 方法来解析命令参数：

### 主要改进：
1. **标识符作为字符串参数**：不再将标识符解析为命令调用
2. **路径支持**：智能组合连续的路径相关token（`/`, `.`, `-`, 标识符, 数字）
3. **保持兼容性**：继续支持其他参数类型（字符串、数字、变量、列表、记录）

### 文件修改：
- `lyra/src/parser/mod.rs`
  - 修改 `parse_call()` 使用 `parse_arg()` 而不是 `parse_primary()`
  - 新增 `parse_arg()` 方法，支持路径解析

## 测试覆盖 (Test Coverage)

新增集成测试 `lyra/tests/test_cd_command.rs`：
- ✅ `test_cd_with_argument` - 简单目录名如 "docs"
- ✅ `test_cd_with_path` - 绝对路径如 "/home/user/projects"
- ✅ `test_ls_with_argument` - 其他命令带参数

**测试结果**：
- 所有原有测试通过 (25 个单元测试)
- 所有新测试通过 (3 个集成测试)
- **总计：28 个测试全部通过** ✓

## 影响范围 (Impact)

这个修复解决了一个关键的可用性问题，现在用户可以：

✅ 切换目录：`cd docs`, `cd /path/to/dir`, `cd ..`
✅ 列出特定目录：`ls src`, `ls /home`
✅ 运行任何带文件/路径参数的命令
✅ 使用相对和绝对路径
✅ 使用包含数字、点、连字符的路径

## 示例 (Examples)

### 简单目录名：
```bash
λ ~/Projects/SOL 〉cd docs
λ ~/Projects/SOL/docs 〉pwd
/home/user/Projects/SOL/docs
```

### 绝对路径：
```bash
λ ~ 〉cd /home/user/Projects
λ ~/Projects 〉pwd
/home/user/Projects
```

### 其他命令：
```bash
λ ~ 〉ls docs
README.md
architecture.md
...
```

### 复杂路径：
```bash
λ ~ 〉cd ./src/completion
λ ~/src/completion 〉ls
completer.rs
command.rs
file.rs
git.rs
mod.rs
```

## 相关文档 (Related Documentation)

- `lyra/BUGFIX-2026-08-29.md` - 详细技术说明
- `lyra/IMPLEMENTATION_SUMMARY.md` - 更新了实现总结
- `lyra/tests/test_cd_command.rs` - 新增的测试

## 总结 (Summary)

这是一个**关键修复**，使 Lyra shell 的基本命令能够正常工作。修复是在解析器层面完成的，不影响求值器或内建命令实现。所有接受文件路径或目录名的命令都会自动受益于这个修复。

**状态**：✅ 已完成并通过所有测试
**版本**：Lyra v0.1.0
**日期**：2026-08-29
