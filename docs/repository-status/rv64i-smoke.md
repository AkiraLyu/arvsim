# `tests/rv64i_smoke.rs`：RV64 冒烟测试

## 功能与实现

测试通过外部 RISC-V GCC 和 `objcopy` 生成裸二进制镜像，用来检查机器单步执行和一段可以明确判断结束的 RV64I 程序。其余测试直接访问 `TestBus` 或 `TestMachine`，检查 UART 输出、访问边界和设备复位。

## 实现状态

5 个测试全部默认执行并通过：

- `compiled_addi_smoke_runs_one_step` 验证 `addi`、PC +4 和 x31=42。
- UART 测试写入 `OK` 后检查测试缓冲。
- 机器复位测试确认 UART、输入队列和 MMIO 日志等易失设备状态被清除，同时 RAM 内容保持不变。
- 总线测试检查非法宽度、溢出地址、8 字节 RAM 读写，以及失败的 UART 宽访问不会产生输出。
- `rv64i_memory_branch_and_x0_contract` 检查 x0、栈内存访问、分支和跳转。测试先配置 `MTVEC`，程序把 42 写入结果地址后执行 `ebreak`；宿主最多执行 32 步，并检查 PC 已进入机器模式异常入口且 `MCAUSE=3`。

## 公共接口

本文件不提供库接口。测试使用 `support::build_flat_asm`、`TestMachine`、`TestBus` 和 `MemDevice`。

## 依赖关系

依赖外部 `riscv64-elf-gcc`/`objcopy`、固定链接地址 `0x8000_0000`、测试辅助模块和 `Machine::step()` 语义。

## 改进建议

当前测试把多类指令放在同一个程序中，失败时不容易定位。零偏移控制流、特权级异常和未对齐访存已有库单元测试，但仍应分别覆盖各类内存访问和立即数边界，并在持续集成中安装或缓存外部工具链。测试辅助代码已经通过 `Machine::step()` 统一推进设备和 CPU。
