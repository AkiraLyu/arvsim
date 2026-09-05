# RV64 程序与平台集成测试

`tests/rv64i_smoke.rs` 保留两个默认用例：

- 编译 RV64 汇编程序，经正式装载器和平台执行，检查内存读写、分支、`x0` 及 `ebreak` 异常。
- 复位整机后，RAM 内容保留，UART 的输入和输出状态清除。

独立 `addi`、UART 寄存器、Virtio 队列大小和总线宽度检查已有对应模块测试，不再在集成层重复。编译工具通过 `Command` 直接执行，链接地址使用 `cfg::DRAM_BASE`。测试产物放在 Cargo 提供的临时目录。

测试仍依赖 RISC-V GCC 和 objcopy，覆盖范围不等于完整 ISA 符合性验证。
