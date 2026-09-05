# 测试辅助精简

已删除 `tests/support/mod.rs` 的通用机器包装、透传方法、重复运行循环、路径包装和 shell 工具探测。汇编构建只供 RV64 程序测试使用，UART 等待只供 xv6 验收使用，分别留在对应文件。

工具直接作为 `Command` 的可执行文件启动，不再解释为 shell 源码；原来专门验证 `require_tool` 的测试随该函数一起删除。CPU、设备、ELF 装载和 xv6 启动继续复用正式库。

保留的测试要求与分层见 [测试组织](../docs/repository-status/test-support.md)。
