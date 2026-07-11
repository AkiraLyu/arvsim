# `tests/rv64i_smoke.rs`：RV64 冒烟测试

## 功能与实现思路

通过外部 RISC-V GCC/objcopy 动态构建 flat binary，验证 CPU 单步执行和一段带明确完成协议的 RV64I 控制流；另直接访问 `TestBus` 验证 UART 输出捕获。

## 当前状态

3 个测试全部默认执行并通过：

- `compiled_addi_smoke_runs_one_step` 验证 `addi`、PC +4 和 x31=42。
- UART 测试写入 `OK` 后检查测试缓冲。
- `rv64i_memory_branch_and_x0_contract` 验证 x0、栈访存、分支和跳转；guest 把 42 写入 signature 并执行 `ebreak`，宿主最多执行 32 步并要求明确看到断点，不再依赖固定成功步数。

## 对外接口

无产品接口；测试通过 `support::build_flat_asm`、`TestMachine`、`TestBus` 和 `MemDevice` 组合。

## 耦合方式

依赖外部 `riscv64-elf-gcc`/`objcopy`、固定链接地址 `0x8000_0000`、测试支撑和 CPU `step()` 语义。

## 优化方向

现有合同仍把多类指令串在同一 guest 中，失败定位不够精确；应为零偏移控制流、每类访存/异常、未对齐和边界立即数增加表驱动测试，并在 CI 显式安装或缓存外部工具链。测试 helper 也应改走 `Machine::step()`，为未来设备时钟保留一致入口。
