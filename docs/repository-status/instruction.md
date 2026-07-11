# `src/instruction.rs`：指令解码与执行

## 功能与实现思路

先把 32 位字解码成通用字段，再按 opcode 分派到执行函数；低两位不为 `11` 时改走 16 位压缩指令路径。执行器直接读写 `Cpu` 的寄存器、PC、CSR 和总线，结束时强制 `x0 = 0`。

## 当前状态

部分实现，覆盖：

- RV64I 基础整数、load/store、分支、跳转、LUI/AUIPC 和 W 类操作。
- RV64M 乘除、余数和边界情况。
- RV64A 的 LR/SC 与常见 AMO W/D 变体。
- CSR 读改写、`ecall`、`ebreak`、`sret`、`mret`、`wfi`、`sfence.vma`。
- xv6 路径所需的一组常见 RVC 指令。
- `FENCE/FENCE.I` 作为无操作；针对特定 byte-store 循环的 DRAM 批量填充加速。

## 对外接口

- `Instruction` 的 `opcode/rd/funct3/rs1/rs2/funct7/raw` 字段全部公开。
- `decode(u32) -> Instruction`。
- `execute(&mut Cpu, Instruction) -> Result<(), Exception>`。

## 耦合方式

该模块依赖并直接修改 `cpu::Cpu`，CPU 又调用本模块，形成双向耦合。访存经 `Cpu::translate` 和 `cpu.bus`，CSR 经 `cpu.csr`，错误使用 `Exception`，memset 快速路径依赖 `cfg` 的 DRAM 范围。

## 语义简化和问题

- `ecall` 无条件进入 supervisor trap cause 8，没有依据实际 privilege 选择 U/S/M cause。
- `ecall` 直接调用 supervisor trap 入口，未像其他异常一样在 `STVEC=0` 时返回宿主错误。
- `mret` 只把 PC 设为 `MEPC`，不恢复 `mstatus`；`wfi`、`sfence.vma`、fence 均无副作用。
- LR/SC 不跟踪 reservation，SC 永远成功；LR 还使用 `MemoryAccess::Store` 做地址翻译，可能错误要求写权限。没有多 hart 内存序和 aq/rl 语义。
- 没有显式对齐检查；实际行为取决于设备是否接受未对齐访问。
- 64 位 store/AMO 被拆为两个 32 位设备写；第二半失败时第一半和 MMIO 副作用不会回滚。
- CPU 外层用 PC 是否改变推断 next-PC，零偏移 branch/jump 因而错误顺序前进；执行结果需要显式表达 PC 提交方式。
- RVC 只实现子集；未实现浮点等 xv6 工具链可能生成的扩展，未声明 ISA 配置。
- 解码、执行、立即数工具和优化集中在约 1000 行单文件中。

## 优化方向

优先让执行返回结构化的 PC/访存/trap 结果并修复零偏移控制流；随后按基础 ISA/扩展拆分 decoder 和 executor，引入明确 privilege/ISA feature 状态，实现 reservation、对齐和系统指令语义，并用 riscv-tests/arch-test 和属性测试覆盖解码边界。
