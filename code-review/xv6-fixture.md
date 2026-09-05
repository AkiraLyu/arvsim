# xv6 启动与验收精简

启动流程原先为了完整性测试而要求裸二进制、usertests ELF、`main` 和 `_entry` 符号，还将内核先装入临时内存再重复装载。这些要求超出了实际启动需要，现已删除。

运行只使用内核 ELF 和磁盘镜像。加速模式保留版本、源码状态、校验值及所需内核符号检查；关闭加速后可以直接运行打包镜像。默认测试调用实际构建入口，不再维护专供测试调用的 `validate/require_files` 接口。

保留 shell 基础程序和完整 usertests 两个验收用例。独立启动与快速 usertests 已被上述用例包含，删除后不减少相应功能覆盖。完整结果见 [验证记录](../docs/repository-status/verification.md)。
