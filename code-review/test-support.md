# `tests/support/mod.rs`：测试辅助代码审查记录

> 本文属于代码审查报告（基线：提交 `17ad107` 的当前工作区，2026-08-12），只记录本轮仍成立的审查发现与修改建议，未改动实现代码。总览见 [README](./README.md)。对应的现状文档：[test-support.md](/home/akira/codespace/arvsim/docs/repository-status/test-support.md)。

## 审查范围与总体判断

覆盖正式 `VirtPlatform` 的测试包装、临时汇编构建、fixture 路径、ELF 符号读取、工具探测、UART 等待和错误诊断。运行时设备已从测试目录移入正式库，职责边界明显改善；工具存在性探测仍把可配置字符串直接插入 shell 源码。

共 1 条发现：高危 0、中危 0、低危 1。

## 发现的问题

### 1. `require_tool` 未引用地拼接工具名，空格和 shell 元字符会改变命令

- 位置：[`tests/support/mod.rs:386`](/home/akira/codespace/arvsim/tests/support/mod.rs#L386)
- 分级：低危 · 健壮性

`require_tool` 构造 `sh -c "command -v {tool} ..."`。`tool` 可由 `TOOLPREFIX` 间接控制；空格、分号、重定向或命令替换会被 shell 当作语法而不是文件名。结果可能是假阳性、难以理解的失败，或执行调用方意外放入前缀的命令。该变量属于本地测试配置，所以本条不按远程代码执行风险升级，但接口本身不应解释数据为 shell 源码。

**修改建议：**

可以把值作为位置参数传给固定 shell 程序，而不是拼接进程序文本：

```rust
pub fn require_tool(tool: &str) -> Result<(), Box<dyn Error>> {
    let status = Command::new("sh")
        .args(["-c", "command -v \"$1\" >/dev/null 2>&1", "sh", tool])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("required tool is missing from PATH: {tool}").into())
    }
}
```

若工具名以 `-` 开头，或希望完全避免 shell 的可移植性差异，可在 Rust 中遍历 `PATH` 并按平台检查可执行文件。增加包含空格、分号和不存在工具名的测试，确认不会执行额外命令且不会误报存在。
