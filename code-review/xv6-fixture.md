# `tests/xv6_fixture.rs`：xv6 验收测试审查记录

> 本文属于代码审查报告（基线：提交 `17ad107` 的当前工作区，2026-08-12），只记录本轮仍成立的审查发现与修改建议，未改动实现代码。总览见 [README](./README.md)。对应的现状文档：[xv6-fixture.md](/home/akira/codespace/arvsim/docs/repository-status/xv6-fixture.md)。

## 审查范围与总体判断

覆盖 fixture 完整性、缺失文件模式、UART 进度判断、预算解析、基础程序和 usertests 合同。跳过与强制存在模式已经区分，长测试预算和失败标记也更明确；默认完整性测试仍没有验证一个运行时必需的 ELF 文件内容。

共 1 条发现：高危 0、中危 0、低危 1。

## 发现的问题

### 1. “artifacts are well formed” 只检查 usertests 路径存在，不验证其 ELF 或符号

- 位置：[`tests/xv6_fixture.rs:55`](/home/akira/codespace/arvsim/tests/xv6_fixture.rs#L55)、[`tests/xv6_fixture.rs:71`](/home/akira/codespace/arvsim/tests/xv6_fixture.rs#L71)、[`tests/support/mod.rs:278`](/home/akira/codespace/arvsim/tests/support/mod.rs#L278)、[`tests/support/mod.rs:295`](/home/akira/codespace/arvsim/tests/support/mod.rs#L295)
- 分级：低危 · 测试可信度

`require_xv6_fixture` 只要求 `_usertests` 路径存在；默认执行的“well formed”测试只用 `readelf` 检查 kernel ELF，并验证 `kernel.bin` 与 `fs.img` 非空。空文件、错误架构 ELF 或缺少 `exec` 符号的 `_usertests` 仍会让默认完整性测试通过，直到 ignored 行为测试调用 `xv6_machine()` 和 `nm` 时才失败。

这使 `ARVSIM_REQUIRE_XV6_FIXTURE=1` 的通过结果弱于名称和脚本文档暗示的合同。

**修改建议：**

```rust
let usertests = support::xv6_usertests_elf();
let usertests_path = usertests
    .to_str()
    .ok_or_else(|| std::io::Error::other("non-UTF-8 fixture path"))?;
let user_header = support::run(
    Command::new(&readelf)
        .env("LC_ALL", "C")
        .args(["-h", usertests_path]),
)?;
let user_header = String::from_utf8_lossy(&user_header.stdout);
assert!(user_header.contains("Machine:") && user_header.contains("RISC-V"));
```

再通过现有符号解析辅助或 `nm` 断言 kernel 与 usertests 的全部必需符号存在，并校验 `fixture.env` 中实际 commit/entry 与当前文件一致。这样默认完整性检查可以在长时间启动前给出精确失败原因。
