# `tests/rv64i_smoke.rs`：RV64 冒烟测试

## 功能与实现思路

通过外部 RISC-V GCC/objcopy 动态构建 flat binary，验证 CPU 单步执行；另直接访问 `TestBus` 验证 UART 输出捕获。

## 当前状态

- 默认执行 `compiled_addi_smoke_runs_one_step`：验证 `addi`、PC +4 和 x31=42。
- 默认执行 UART 测试：写入 `OK` 后检查缓冲。
- `rv64i_memory_branch_and_x0_contract` 被 `#[ignore]`：设计上验证 x0、栈访存、分支和跳转。

本轮实际运行该忽略测试失败，但不是前述指令链首先失败：成功路径包含 8 条实际执行指令，测试调用 `run_steps(9)`，第 9 步读取零填充并产生 `IllegalInstruction(0)`。测试的 ignore 文案也已落后于当前实现能力。

## 对外接口

无产品接口；测试通过 `support::build_flat_asm`、`TestMachine`、`TestBus` 和 `MemDevice` 组合。

## 耦合方式

依赖外部 `riscv64-elf-gcc`/`objcopy`、固定链接地址 `0x8000_0000`、测试支撑和 CPU `step()` 语义。

## 优化方向

修正步数或引入 guest halt/signature 协议；移除已过时 ignore 并把合同拆成精确测试；为每类指令、异常、未对齐和边界立即数增加表驱动测试；CI 显式安装/缓存工具链。
