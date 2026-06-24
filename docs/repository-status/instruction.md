# `src/instruction.rs`：指令解码和执行

## 设计

- 该模块把取到的 32 位字先解码为基础字段，再按 opcode 执行。
- 压缩指令通过低两位判断后走单独执行路径。
- 执行函数直接操作 `Cpu` 的寄存器、PC、CSR 和总线。

## 实现

- RV64I：实现加载、存储、立即数运算、寄存器运算、分支、`JAL/JALR`、`LUI/AUIPC`。
- RV64I W 类操作：实现 `ADDIW`、`SLLIW`、`SRLIW`、`SRAIW` 和 32 位寄存器运算。
- RV64M：实现乘法、乘高位、除法、余数以及 W 指令变体，并处理除零和有符号溢出边界。
- RV64A：实现 LR/SC 和 AMO swap/add/xor/or/and/min/max/minu/maxu；SC 在单 hart 模型中总是成功。
- Zicsr 和系统指令：实现 CSR 读改写、`ecall`、`ebreak`、`sret`、`mret`、`wfi`、`sfence.vma`。
- RVC：实现 xv6 当前二进制路径需要的常见压缩加载、存储、跳转、分支、栈指针调整和 ALU 指令。
- `FENCE/FENCE.I` 在单 hart 模型中作为保守空操作处理。
- 额外有一个针对 xv6 内存填充循环的批量加速：只识别严格的 DRAM 内字节写循环模式。

## 接口

- `struct Instruction`
  - `opcode`
  - `rd`
  - `funct3`
  - `rs1`
  - `rs2`
  - `funct7`
  - `raw`
- `decode(instruction: u32) -> Instruction`
- `execute(cpu: &mut Cpu, inst: Instruction) -> Result<(), Exception>`

## 限制

- 解码和执行仍集中在一个文件中，没有按扩展拆分。
- 没有声明完整 RISC-V 规范一致性，尚未接入官方 riscv-tests。
- 原子指令按单 hart 场景实现，不覆盖完整多 hart 内存模型。
