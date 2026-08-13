# `tests/rv64i_smoke.rs`：RV64 冒烟测试

## 功能与实现

测试通过外部 RISC-V GCC 和 `objcopy` 生成裸二进制镜像，用来检查机器单步执行和一段可以明确判断结束的 RV64I 程序。其余测试通过 `TestMachine` 中的正式 `VirtPlatform` 地址空间检查 UART 输出、访问边界、virtio 队列参数和设备复位。

## 实现状态

6 个测试全部默认执行并通过：

- `compiled_addi_smoke_runs_one_step` 验证 `addi`、PC +4 和 x31=42。
- UART 测试先执行带 DLAB 的波特率除数写入，再写入 `OK`，确认除数字节不会混入测试输出。
- virtio 测试确认零、非二次幂及超上限队列长度均返回存储访问错误。
- 机器复位测试确认正式 UART 输入输出及 PLIC/virtio 易失状态由平台复位，同时 RAM 内容保持不变。
- 总线测试检查非法宽度、溢出地址、8 字节 RAM 读写，以及失败的 UART 宽访问不会产生输出。
- `rv64i_memory_branch_and_x0_contract` 在 1 MiB 测试 RAM 中检查实际栈顶、x0、栈内存访问、分支和跳转。测试先配置 `MTVEC`，把由 Rust 常量生成的结果地址注入汇编，程序写入 42 后执行 `ebreak`；宿主最多执行 32 步，并检查 PC 已进入机器模式异常入口且 `MCAUSE=3`。

## 公共接口

本文件不提供库接口。测试使用 `support::build_flat_asm` 和包装正式 `VirtMachine` 的 `TestMachine`。

## 依赖关系

依赖 `TOOLPREFIX` 选择的外部 RISC-V GCC/objcopy、固定链接地址 `0x8000_0000`、测试辅助模块和 `Machine::step()` 语义。

## 改进建议

当前测试把多类指令放在同一个程序中，失败时不容易定位。零偏移控制流、特权级异常和未对齐访存已有库单元测试，但仍应分别覆盖各类内存访问和立即数边界，并在持续集成中安装或缓存外部工具链。测试辅助代码已经通过 `Machine::step()` 统一推进设备和 CPU。
