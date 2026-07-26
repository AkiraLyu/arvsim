# `src/main.rs`：命令行入口

## 功能与实现

命令行程序解析运行参数、创建 `Platform`、装载裸二进制或 ELF 镜像、按配置挂载 UART，然后创建并运行 `Machine`。参数解析不依赖第三方 Rust 库，错误通过 `ExitCode` 返回。

## 实现状态

当前提供：

- 跳过 `argv[0]`，将唯一的位置参数作为目标程序的镜像路径。
- 支持 `-h/--help`、未知参数检查、缺值检查和重复镜像检查。
- `--format auto|flat|elf` 支持自动识别 ELF 或强制格式。
- `--platform bare|uart` 选择纯 DRAM 或 DRAM + UART 平台。
- `--dram-base`、`--dram-size`、`--uart-base` 和 `--entry` 可覆盖平台参数和入口地址。
- `--max-steps` 默认限制为 1,000,000 步，也可设置 `unlimited`。
- `--debug off|pc|full` 控制无跟踪、PC 跟踪或完整寄存器/CSR 跟踪。
- 装载失败、平台配置错误和 CPU 返回给宿主的执行错误会返回非零退出码；达到指定步数上限则正常退出。当前 CPU 会把架构异常全部送入目标程序的异常入口，因此执行错误分支主要为后续扩展保留。
- UART 与 DRAM 区域重叠、地址溢出、入口超出 DRAM，以及入口未按 2 字节对齐会在组装前被拒绝；初始栈指针按 16 字节对齐。

## 命令行接口

```text
Usage: arvsim [OPTIONS] <IMAGE>

Options:
  --format <auto|flat|elf>
  --platform <bare|uart>
  --dram-base <ADDR>
  --dram-size <SIZE>
  --uart-base <ADDR>
  --entry <ADDR>
  --max-steps <N|unlimited>
  --debug <off|pc|full>
  -h, --help
```

数值支持十进制、`0x` 十六进制和下划线；DRAM 大小另外支持 `K/M/G` 与 `KiB/MiB/GiB` 后缀。

退出码：0 表示显示帮助或达到步数上限；1 表示镜像装载失败或 CPU 返回给宿主的执行错误；2 表示参数或平台配置错误。当前实现会把所有 RISC-V 架构异常送入目标程序的 `mtvec/stvec` 入口，不会直接结束模拟器。

## 依赖关系

- 依赖 `loader` 完成格式检测和镜像装载。
- 通过 `Platform` 应用 DRAM 布局、检查 MMIO 冲突并挂载 UART。
- 通过 `Machine` 统一复位设备与 CPU，并执行固定周期单步或连续运行。
- `bare` 和 `uart` 仍是轻量平台，不包含 CLINT、PLIC 或 virtio；完整的 xv6 平台仍只存在于测试辅助模块中。

## 已知限制

- 目标程序没有正常停机或退出协议；目前只能在达到步数上限时正常停止。若目标程序没有设置有效的异常入口，CPU 可能反复跳转到无效地址，而不是让宿主立即退出。
- 参数解析使用 UTF-8 `std::env::args()`，不支持非 UTF-8 镜像路径。
- 命令行平台预设仍只覆盖 DRAM/UART；更完整的设备组合可通过库接口 `Platform::attach_device` 扩展。
- DRAM 配置过大时，宿主内存分配失败可能直接终止进程；目前没有稀疏内存，也无法从内存耗尽中恢复。
- `--dram-base` 可以改变物理布局，但部分 xv6 加速仍使用 `cfg` 中的默认 DRAM 范围。普通指令执行不依赖这一范围；启用加速器并使用自定义布局时仍可能出现不一致。
