# arvsim 仓库状态总览

本文档集基于 2026-07-10 的工作树与提交 `2f64e7a`。分析对象包括正式 crate、命令行入口、测试支撑和脚本；`target/` 仅视为构建产物，不作为源模块分析。

## 总体判断

arvsim 是一个无第三方 Rust 依赖、单 hart、解释执行式的 RV64 模拟器原型。库层已经具备 RV64I/M/A、部分 RVC、CSR、监督模式 trap、Sv39、定时器、外部中断接入点，以及 flat/ELF64 镜像装载；测试层另外实现了 UART、简化 PLIC 和 virtio-blk，目标是驱动特定版本的 xv6-riscv。

当前实现更接近“面向 xv6 验收的功能原型”，尚不是通用、规范一致的 RISC-V 虚拟机。CLI 已具备镜像格式、步数、调试和基础平台配置，`Cpu::run()` 也已统一复用 `step()`；主要边界转为正式平台缺少 CLINT、PLIC、virtio，CPU 内含大量绑定固定 xv6 地址和数据布局的快速路径，且特权级、CSR、MMU、异常与原子语义仍为简化模型。

## 状态标记

| 状态 | 含义 |
| --- | --- |
| 已实现 | 主路径有代码且默认测试覆盖核心行为 |
| 部分实现 | 可支撑当前目标，但语义、覆盖或通用性不完整 |
| 测试专用 | 只存在于 `tests/`，正式库和 CLI 无法直接使用 |
| 占位 | 模块已导出但没有实现 |

## 模块索引

| 模块 | 当前状态 | 说明 |
| --- | --- | --- |
| [仓库与 crate 边界](./architecture.md) | 部分实现 | 源码、测试平台与脚本的总体关系 |
| [`src/lib.rs`](./lib.md) | 已实现 | crate 顶层模块导出 |
| [`src/main.rs`](./main.md) | 已实现 | flat/ELF CLI、运行控制和基础平台配置 |
| [`src/cfg.rs`](./cfg.md) | 已实现 | 固定内存布局常量 |
| [`src/trap.rs`](./trap.md) | 部分实现 | 异常枚举，不含统一 trap/interrupt 模型 |
| [`src/bus.rs`](./bus.md) | 已实现 | 区域式 MMIO 总线与设备 trait |
| [`src/dram.rs`](./dram.md) | 已实现 | 128 MiB 小端平坦内存 |
| [`src/uart.rs`](./uart.md) | 部分实现 | 仅发送和固定状态的简化 UART |
| [`src/csr.rs`](./csr.md) | 部分实现 | 数组式 CSR 与 S-mode 别名 |
| [`src/instruction.rs`](./instruction.md) | 部分实现 | RV64I/M/A、部分 RVC 和系统指令 |
| [`src/loader.rs`](./loader.md) | 已实现 | flat binary 与 ELF64 RISC-V 装载 |
| [`src/cpu.rs`](./cpu.md) | 部分实现 | 执行循环、Sv39、trap/interrupt 与 xv6 快速路径 |
| [`src/clint.rs`](./clint.md) | 占位 | 空模块 |
| [`src/plic.rs`](./plic.md) | 占位 | 空模块 |
| [`tests/support/mod.rs`](./test-support.md) | 测试专用 | xv6 机器、PLIC、virtio、UART 和构件工具 |
| [`tests/cli.rs`](./cli-tests.md) | 已实现 | CLI 帮助、正常停止和错误退出进程测试 |
| [`tests/rv64i_smoke.rs`](./rv64i-smoke.md) | 部分实现 | 基础 CPU/UART 集成测试 |
| [`tests/xv6_fixture.rs`](./xv6-fixture.md) | 测试专用 | xv6 启动和用户态验收合同 |
| [`scripts/`](./scripts.md) | 已实现 | xv6 构建、测试编排和交互启动 |

## 关键耦合

```mermaid
flowchart LR
  CLI["src/main.rs"] --> CPU["cpu"]
  CLI --> LOADER["loader"]
  CLI --> BUS["bus"]
  CLI --> DRAM["dram"]
  CLI --> UART["uart"]
  CPU --> INST["instruction"]
  CPU --> CSR["csr"]
  CPU --> BUSIF["MemDevice"]
  INST --> CPU
  INST --> CSR
  INST --> BUSIF
  BUS --> DRAM
  BUS --> UART
  CPU --> TRAP["trap::Exception"]
  INST --> TRAP
  BUS --> TRAP
  TEST["tests/support"] --> CPU
  TEST --> BUSIF
  TEST --> XV6["xv6 kernel/fs image"]
```

`instruction` 直接修改 `Cpu` 公共字段，形成双向强耦合；CPU 通过 `MemDevice` 与具体机器解耦，但中断也被压入同一 trait。正式 CLI 使用通用 `Bus`，xv6 测试则绕过它，把完整测试机器实现成单个 `TestBus`。因此“测试中能运行 xv6”不等于“正式库已提供 xv6 平台”。

## 验证快照

- `cargo test`：通过；共 25 个默认执行测试通过，5 个测试被忽略。
- `cargo test --test rv64i_smoke -- --ignored`：失败；被忽略合同固定执行 9 步，但程序成功路径只有 8 条指令，第 9 步取到零字并触发 `IllegalInstruction(0)`。
- 本轮未重跑耗时很长的 4 个 xv6 忽略测试；它们依赖外部工具链和已构建 fixture。

详见 [验证记录](./verification.md) 与 [不完善之处和优化方向](./gaps-and-roadmap.md)。
