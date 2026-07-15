# `src/instruction.rs`：指令解码与执行

## 功能与实现

模块先把 32 位取指窗口拆成通用字段，再按操作码调用执行函数。若低两位不是 `11`，则按 16 位压缩指令处理。执行代码直接读写 `Cpu` 的寄存器、PC、CSR 和总线，并在指令结束时恢复 `x0=0`。

## 实现状态

当前覆盖：

- RV64I 基础整数、加载/存储、分支、跳转、LUI/AUIPC 和 W 类操作。
- RV64M 乘法、除法、余数及其边界情况。
- RV64A 的 LR/SC 与常见 AMO W/D 变体。
- CSR 读改写、`ecall`、`ebreak`、`sret`、`mret`、`wfi` 和 `sfence.vma`。
- xv6 使用的一组常见 RVC 指令。
- `FENCE/FENCE.I` 作为单硬件线程下的无操作；可选 xv6 加速器启用时，可批量处理严格匹配的字节填充循环。

控制流通过 `Cpu::write_pc` 明确提交下一条 PC，因此零偏移分支和跳转可以正确停在原地址。`ecall` 会根据 U/S/M 特权级产生不同异常；`sret/mret`、`wfi`、`sfence.vma` 和 CSR 指令都检查当前特权级及 TSR、TW、TVM 等限制。CSRRS/CSRRC 的零源操作不会写 CSR，CSRRW 在 `rd=x0` 时不会读取 CSR。

加载、存储、压缩访存和 AMO 会在地址翻译前检查自然对齐。LR 按读取权限访问并建立物理地址保留；SC 检查并消费保留，失败时不写内存。普通存储、其他 AMO 和异常入口会清除保留。`MemDevice::write` 现可接收 `u64`，因此 8 字节存储和 AMO 只进行一次设备写入。

## 公共接口

- `Instruction` 的 `opcode/rd/funct3/rs1/rs2/funct7/raw` 字段全部公开。
- `decode(u32) -> Instruction`。
- `execute(&mut Cpu, Instruction) -> Result<(), Exception>`。

## 依赖关系

该模块直接修改 `cpu::Cpu`，CPU 又会调用本模块，因此两者相互依赖。内存访问经过带宽度的地址翻译和 PMP 检查，CSR 访问先由 CPU 验证，错误统一使用 `Exception`。批量内存填充加速仍依赖 `cfg` 中的默认 DRAM 范围。

## 已知问题

- aq/rl 和多硬件线程内存顺序尚未实现；设备或 DMA 直接改写保留范围时也无法通知 CPU。
- `wfi` 在权限允许时只是无操作；`sfence.vma` 因没有 TLB 而无实际副作用。
- RVC 只实现子集；浮点、向量等扩展未实现，`misa` 也不声明当前 ISA 组合。
- 解码、执行、立即数工具和优化仍集中在一个较大的文件中。

## 改进建议

后续按 I/M/A/C/system 拆分模块，明确支持的 ISA 组合，并用 riscv-arch-test、指令级边界测试和属性测试扩大覆盖。原子扩展还需补充 aq/rl、设备写入通知和多硬件线程内存顺序。
