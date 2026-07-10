# `src/clint.rs`：CLINT 占位模块

## 功能与当前状态

文件为空，仅通过 `src/lib.rs` 暴露模块名，没有类型、常量、寄存器或测试，状态为占位。

## 对外接口

只有 `arvsim::clint` 模块路径，没有可调用项。

## 耦合方式

当前无代码耦合。时间推进和 supervisor timer compare 被直接实现在 `Cpu::tick/timer_is_pending` 中，没有 MMIO CLINT。

## 优化方向

明确目标平台采用 CLINT 还是 ACLINT；实现 `mtime/mtimecmp` 或相应设备寄存器，通过统一时钟推进并提交类型化 timer interrupt；CPU 只消费中断控制器状态，不直接模拟平台计时设备。
