# CPU 与 xv6 加速审查与修复

2026-09-05 已修复加速器单步工作无上限的问题，并处理两项新增发现。

- CPU 原先只观察自己的存储，无法发现设备 DMA 修改 LR/SC 地址。DRAM 现在维护物理页版本，经总线查询；SC 检查版本后消费保留。同页 DMA 写入使保留失效，不同物理页写入不影响该保留。默认 MMIO 不支持保留。
- xv6 加速移入私有子模块。内存、字符串操作每次最多处理 1 MiB，页表遍历和叶子映射分别最多 256 页，超限执行原始指令。`strlen` 扫描有界，`memmove` 缓冲有界，`freewalk` 在任何释放前检查整棵树。
- 加速配置检查 DRAM、内核、全局对象、进程表和函数入口；正式装载器核验固定提交、源码和镜像。加速只在监督模式的 hart 0 生效。
- 已删除根据 usertests 地址提前返回的用户态 `exec` 特例。所有用户态系统调用都执行原始指令并进入内核。

加速测试已改为从 CPU 正常单步入口验证内存复制、调用返回、超大请求回退、配置失败后继续工作，以及其他特权级或硬件线程执行原始指令。删除直接调用私有页表辅助函数、断言固定映射数量及用内部 `walk` 验证内部 `mappage` 的用例；对应辅助方法恢复为私有。

原来的超预算测试中，页表分支会先因未配置加速或地址未对齐而返回，不能证明预算有效。新用例先启用有效加速配置，再检查超大请求是否执行了客体指令并保持目标数据不变。稀疏分配、复制和释放通过完整 xv6 usertests 验证。DMA 保留失效及架构异常测试继续保留，结果见 [验证记录](../docs/repository-status/verification.md)。

标准依据：[RISC-V A 扩展](https://riscv.github.io/riscv-unified-db/manual/html/isa/isa_20240411/chapters/a-st-ext.html)、[监督级架构](https://docs.riscv.org/reference/isa/priv/supervisor.html)。可选内核加速会合并指令，不能证明精确时序或完整 ISA 符合性；限制见 [CPU 文档](../docs/repository-status/cpu.md)。
