# 当前仓库实现说明

本文档记录当前代码仓库的模块内容、作用、实现进度和待办事项。项目目标是构建一个能够完整运行 MIT xv6-riscv 的 RISC-V 模拟器；本文只描述现状和后续工作，不代表这些能力已经全部实现。

## 总体状态

当前仓库已经具备一个最小 Rust 模拟器骨架：

- 可以从命令行加载 flat binary 到 DRAM 起始地址 `0x80000000`。
- 有 CPU、Bus、DRAM、UART、CSR、异常和指令解码/执行模块。
- 有基础单元测试、RISC-V 小程序 smoke test、xv6 fixture 构建脚本，以及完整 xv6 运行契约测试。
- xv6 fixture 可以构建出 `kernel/kernel`、`kernel/kernel.bin` 和 `fs.img`。

当前还不能运行 xv6。直接运行 xv6 契约测试会在 `0x80000000` 第一条指令处遇到 `IllegalInstruction`，这是预期结果，因为 xv6 需要 RV64GC、CSR 指令、特权态切换、trap/timer、页表、PLIC、virtio 等能力。

## 目录结构

```text
src/      模拟器主体代码
tests/    集成测试和 xv6 运行契约测试
scripts/  xv6 fixture 构建和 testbench 入口脚本
docs/     项目文档
```

## 核心模块

### `src/main.rs`

内容：

- 解析命令行参数。
- 创建 DRAM 并加载输入 binary。
- 创建 UART。
- 创建 Bus 并挂载 DRAM、UART。
- 创建 CPU，reset 后进入 `run()`。

作用：

- 当前模拟器的可执行入口。
- 用于直接执行一个 flat binary。

当前进度：

- 已完成最小运行入口。
- 仅支持一个 binary 参数。
- 默认加载位置由 `Dram::load()` 决定，即 DRAM 起始处。

待办：

- 支持 ELF loader，按 program header 加载 xv6 kernel。
- 支持加载磁盘镜像 `fs.img` 并连接 virtio block device。
- 支持运行参数，例如最大步数、关闭 debug、trace 输出路径、设备配置。
- 支持更明确的退出条件，避免只能无限循环或靠异常停止。

### `src/lib.rs`

内容：

- 将 `bus`、`cfg`、`clint`、`cpu`、`csr`、`dram`、`instruction`、`plic`、`trap`、`uart` 暴露为库模块。

作用：

- 让集成测试和未来工具可以复用模拟器内部模块。

当前进度：

- 已完成模块导出。

待办：

- 后续可以增加更高层的 `Machine` 或 `Emulator` API，统一 CPU、Bus、设备和镜像加载。

### `src/cfg.rs`

内容：

- 定义 DRAM 和 UART 的基本地址常量：
  - `DRAM_BASE = 0x80000000`
  - `DRAM_SIZE = 128 MiB`
  - `CPU_START_ADDR = DRAM_BASE`
  - `UART_BASE = 0x10000000`

作用：

- 集中保存当前模拟器的机器布局常量。
- 与 xv6/QEMU `virt` 的 DRAM 和 UART 地址保持一致。

当前进度：

- 已覆盖 DRAM 和 UART。

待办：

- 增加 CLINT、PLIC、virtio-mmio 地址常量。
- 增加 CPU 数、hart id、mtime 频率、页大小等机器参数。
- 将硬编码地址统一迁移到该模块。

### `src/bus.rs`

内容：

- 定义 `MemDevice` trait：
  - `read(addr, size)`
  - `write(addr, value, size)`
- 定义 `Bus`，用 `BTreeMap` 按 base address 管理 RAM 和 UART 设备。
- 实现地址读写分发。

作用：

- CPU 和外设/内存之间的访问抽象。
- 让 DRAM、UART 等设备通过统一接口挂载到 CPU。

当前进度：

- 已支持挂载 DRAM 和 UART。
- 已有基础读写单元测试。

待办：

- 增加设备地址范围检查，避免只按“最后一个小于等于地址的 base”推断设备。
- 扩展设备类型，不应只有 `ram_map` 和 `uart_map` 两类。
- 支持 CLINT、PLIC、virtio-mmio。
- 明确访问宽度、对齐和异常语义。
- 对只读寄存器、写只读寄存器、非法 MMIO 地址返回更准确的异常。

### `src/dram.rs`

内容：

- 定义 `Dram`：
  - `dram: Vec<u8>`
  - `base: u64`
- 支持从文件读取 binary 并拷贝到 DRAM 起始地址。
- 实现小端序读写。
- 提供基础读写和加载测试。

作用：

- 模拟物理内存。
- 当前 binary loader 的承载对象。

当前进度：

- 已实现 flat binary 加载。
- 已实现 1/2/4/8 字节风格的任意 size 小端读写。
- 已检查越界访问。

待办：

- 增加对齐检查，返回 misaligned 异常。
- 明确允许的访问 size，只接受 1/2/4/8 等合法宽度。
- 支持 ELF segment 加载。
- 支持 guest 物理地址和 host 内存切片之间更高效的映射。
- 支持 DMA 访问，供 virtio 设备读写 guest memory。

### `src/uart.rs`

内容：

- 定义简化版 UART。
- 支持读取：
  - `RBR` offset `0x00`：始终返回 0。
  - `LSR` offset `0x05`：始终返回 bit5，即发送保持寄存器空。
- 支持写入：
  - `THR` offset `0x00`：将低 8 bit 输出到 host stdout。

作用：

- 提供最小串口输出能力。
- xv6 kernel `printf` 最终需要通过 UART 输出。

当前进度：

- 已能输出字符。
- 已有 UART 状态和写入测试。

待办：

- 实现 16550a 初始化相关寄存器：`IER`、`FCR/ISR`、`LCR`、baud latch 等。
- 支持输入队列和接收 ready bit。
- 支持中断状态和 PLIC 联动。
- 避免直接写 host stdout，改为可配置输出 sink，方便测试捕获。
- 对寄存器访问宽度和非法 offset 做更准确处理。

### `src/cpu.rs`

内容：

- 定义 `Cpu`：
  - 32 个通用寄存器。
  - `pc`。
  - `bus`。
  - `csr`。
- 初始化时设置 `pc = 0x80000000`，`sp = DRAM_END`。
- 提供 `reset()`、`step()`、`run()`。
- `fetch()` 每次读取 4 字节。
- `execute()` 解码并调用 `instruction::execute()`，根据 PC 是否变化决定是否 `pc += 4`。
- debug 模式下打印 PC、寄存器和 CSR。

作用：

- 模拟 CPU 取指、译码、执行和 PC 更新。
- `step()` 是 testbench 有限步运行的基础。

当前进度：

- 已实现最小 fetch/decode/execute 骨架。
- 已能运行非常简单的 RV64I `addi` smoke 程序。

待办：

- 确保 `x0` 恒为 0。
- 完整实现 RV64I 指令，包括 branch、jump、load/store 立即数、LUI/AUIPC、shift、比较等。
- 实现 RV64M/A/C 等 xv6 需要的扩展，尤其 compressed 指令、`mul`、原子指令。
- 实现 CSR 指令、特权态、`mret/sret`、异常和中断入口。
- 实现 `satp`、页表转换、用户态/内核态内存访问权限。
- 增加停止条件、最大步数、trace 控制。
- 当前 `DEBUG` 固定为 `true`，运行真实系统会产生大量输出，应改为配置项。

### `src/instruction.rs`

内容：

- 定义 `Instruction`，解析 opcode、rd、funct3、rs1、rs2、funct7、raw。
- 支持执行少量指令：
  - R-type：`ADD`、`SUB`
  - I-type：`ADDI`
  - Load：`LB`、`LH`、`LW`、`LD`
  - Store：按 opcode 分发到 Bus 写入
- 提供 `ADDI` 解码测试。

作用：

- 当前 ISA 解码和执行模块。

当前进度：

- 只实现了极小子集。
- 可以支撑最简单的 smoke test。

待办：

- 修正 load/store 立即数解码：当前把 `rs2` 字段当作立即数使用。
- 修正 store 源寄存器和值/size 传参：当前 Store 路径还不是 RISC-V S-type 语义。
- 增加符号扩展和零扩展规则，例如 `LB/LH/LW` 与 `LBU/LHU/LWU`。
- 实现所有 RV64I 基础指令。
- 实现 `Zicsr`、`M`、`A`、`C` 扩展。
- 设计分层 decoder，避免执行函数继续膨胀。
- 增加官方/自建指令级测试。

### `src/csr.rs`

内容：

- 定义一批 machine/supervisor CSR 地址常量。
- 定义 `mstatus/sstatus`、`mip/sip` 相关 mask。
- `Csr` 内部用 4096 个 `u64` 保存 CSR。
- 对 `SIE`、`SIP`、`SSTATUS` 做了与 machine CSR 的简化映射。
- 支持 `load()`、`store()`、`dump_csr()`。

作用：

- 保存和访问 CSR 状态。
- 为后续特权态、trap、timer、页表等能力提供基础。

当前进度：

- 已有 CSR 存储骨架和部分 supervisor alias 行为。
- 当前 CPU 指令执行尚未真正接入 CSR 指令。

待办：

- 实现 CSR 指令：`csrrw`、`csrrs`、`csrrc` 及 immediate 变体。
- 补齐 xv6 启动需要的 CSR：`mhartid`、`mstatus`、`mepc`、`medeleg`、`mideleg`、`sie`、`satp`、`pmpaddr0`、`pmpcfg0`、`menvcfg`、`stimecmp`、`time` 等。
- 实现 CSR 权限、只读位、WARL/WPRI 行为。
- 实现 trap 时 CSR 更新规则。
- 实现中断 pending/enable/delegation 语义。

### `src/trap.rs`

内容：

- 定义 `Exception` enum，覆盖指令地址、访问错误、非法指令、断点、load/store misaligned、ecall、page fault 等异常类型。

作用：

- 当前模块间传递异常的公共类型。

当前进度：

- 已有异常枚举。
- 部分模块会返回 `LoadAccessFault`、`StoreAMOAccessFault`、`IllegalInstruction`。

待办：

- 增加异常编号、interrupt cause 编码和 privilege 信息。
- 将异常真正接入 trap 处理流程，而不是只打印错误。
- 实现 `ecall`、page fault、misaligned、interrupt 的精确行为。
- 区分同步异常和异步中断。

### `src/clint.rs`

内容：

- 当前为空文件。

作用：

- 预留 CLINT/timer 相关模块。

当前进度：

- 未实现。

待办：

- 明确 xv6 当前版本是否依赖 CLINT MMIO，或主要依赖 `time/stimecmp` CSR。
- 实现 machine timer/software interrupt 相关寄存器或等价行为。
- 与 CSR `mip/sip`、timer interrupt、调度器时钟中断联动。

### `src/plic.rs`

内容：

- 当前为空文件。

作用：

- 预留 PLIC 平台级中断控制器模块。

当前进度：

- 未实现。

待办：

- 实现 QEMU `virt` PLIC 地址空间：
  - priority
  - pending
  - enable
  - threshold
  - claim/complete
- 支持 UART IRQ 10 和 virtio IRQ 1。
- 与 CPU interrupt pending/delegation 逻辑联动。

## 测试模块

### `tests/support/mod.rs`

内容：

- 提供测试用 `TestBus` 和 `TestBusState`。
- 支持测试内存、UART 输出捕获、UART 输入队列、MMIO 访问日志。
- 支持编译小型 RISC-V 汇编为 flat binary。
- 提供 `TestMachine`，封装 CPU 和测试 bus。
- 提供 xv6 fixture 路径和加载工具。

作用：

- 是当前 testbench 的公共支撑层。
- 让测试可以有限步运行 CPU 并检查 UART 输出。

当前进度：

- 已能支撑 RV64 smoke test 和 xv6 契约测试。

待办：

- 支持 virtio 磁盘镜像注入。
- 支持更完整的测试设备模型。
- 支持 trace、快照和失败时上下文 dump。
- 将测试 bus 与正式 bus/设备模型逐渐对齐，避免测试和真实运行行为分叉。

### `tests/rv64i_smoke.rs`

内容：

- 编译并运行一个最小 `addi` 程序。
- 测试 UART 输出捕获模型。
- 预留 ignored 的 RV64I 内存、分支和 `x0` 契约测试。

作用：

- 验证工具链、flat binary 加载、CPU 单步和最小指令执行路径。

当前进度：

- 默认测试通过。
- ignored 契约测试用于标记后续 ISA 进度。

待办：

- 扩展为更系统的 RV64I 指令测试。
- 增加边界条件，例如符号扩展、溢出、对齐异常。
- 引入 riscv-tests 或自建指令生成器。

### `tests/xv6_fixture.rs`

内容：

- 检查 xv6 fixture 产物是否存在且格式正确。
- 定义完整 xv6 运行契约测试：
  - 启动到 shell。
  - 执行基础用户程序。
  - 运行 `usertests -q`。
  - 运行完整 `usertests`。

作用：

- 作为项目最终目标的验收测试。
- 明确“完整运行 xv6”的可观察标准：完整 `usertests` 输出 `ALL TESTS PASSED`，且没有 panic/FAILED 等失败标记。

当前进度：

- artifact 检查默认通过。
- xv6 运行契约默认 ignored，因为当前模拟器还不具备所需能力。

待办：

- 随着模拟器实现推进，逐个解除 ignored。
- 增加更细粒度的启动阶段断言，例如进入 `main()`、开启分页、初始化 virtio。
- 增加失败输出截断和 trace 文件保存，方便定位。

## 脚本

### `scripts/build_xv6_fixture.sh`

内容：

- 检查依赖工具。
- 获取或刷新 `mit-pdos/xv6-riscv`。
- 使用 `TOOLPREFIX=riscv64-elf-` 构建 `kernel/kernel` 和 `fs.img`。
- 使用 `objcopy` 生成 `kernel/kernel.bin`。
- 写出 `fixture.env` 记录 commit、入口地址和产物路径。

作用：

- 为 testbench 准备 xv6 真实构建产物。

当前进度：

- 已在当前 Arch Linux 环境验证可用。
- 不会自动执行需要 root 权限的安装命令。

待办：

- 支持固定 commit/tag，减少上游变化带来的测试波动。
- 校验 xv6 commit 和本地 fixture 是否匹配预期。
- 可选构建 QEMU 对照日志，用于比较模拟器行为。

### `scripts/run_testbench.sh`

内容：

- 提供统一测试入口：
  - 默认运行稳定测试。
  - `--with-xv6-fixture` 构建 fixture 后运行稳定测试。
  - `--future-contracts` 运行 ignored 未来契约。
  - `--xv6-contracts` 只运行 xv6 完整运行契约。

作用：

- 简化日常验证和 xv6 目标验证。

当前进度：

- 已可用。

待办：

- 增加更细的参数，例如只跑 quick usertests 或 full usertests。
- 增加超时控制和日志输出路径。
- 在 CI 中区分稳定测试和预期失败的未来契约。

## 文档

### `docs/testbench.md`

内容：

- 说明 testbench 分层、xv6 机器契约、运行命令、完整 xv6 覆盖范围和工具依赖。

作用：

- 解释为什么测试以 xv6 为最终目标组织。

当前进度：

- 已覆盖当前 testbench 设计。

待办：

- 随模拟器能力变化同步更新。
- 补充每个 xv6 阶段失败时的定位指南。

### `docs/repository-status.md`

内容：

- 本文档。

作用：

- 记录当前仓库模块级实现状态、作用和待办。

当前进度：

- 覆盖当前 `src/`、`tests/`、`scripts/`、`docs/`。

待办：

- 每次模块职责或实现进度变化时同步维护。

## 面向 xv6 的总体待办

为了让 `xv6_runs_full_usertests_suite` 通过，至少需要补齐以下大块能力：

1. ISA：
   - RV64I 完整基础指令。
   - RV64M、RV64A、compressed 指令。
   - CSR 指令和 fence 相关行为。
2. 特权态：
   - machine/supervisor/user mode。
   - `mret/sret`。
   - trap delegation。
   - `ecall` 和异常入口。
3. 内存系统：
   - `satp`。
   - Sv39 页表转换。
   - 权限检查。
   - page fault。
4. 中断和计时：
   - timer。
   - `time/stimecmp`。
   - `mip/sip/sie/mie`。
   - 外部中断注入。
5. 设备：
   - 16550a UART 完整寄存器和输入/中断。
   - PLIC。
   - virtio-mmio block device。
   - `fs.img` 作为块设备后端。
6. 工程化：
   - 可控 debug/trace。
   - 最大步数和退出条件。
   - 失败现场 dump。
   - 更完整的指令和系统测试。

## 当前验证命令

稳定测试：

```sh
cargo test
```

带 xv6 fixture 的稳定测试：

```sh
scripts/run_testbench.sh --with-xv6-fixture
```

xv6 完整运行契约测试：

```sh
scripts/run_testbench.sh --xv6-contracts
```

最后一个命令当前预期失败，用来衡量距离完整运行 xv6 的差距。
