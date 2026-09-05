# `src/main.rs`：主命令行程序

命令行程序读取参数、创建 `Platform`、加载原始二进制或 ELF 镜像，并按配置添加 UART，然后创建和运行 `Machine`。参数解析只使用 Rust 标准库，通过 `ExitCode` 返回退出状态。

## 参数与运行规则

- 接收一个镜像路径；未知选项、缺少选项值或提供多个镜像路径都会报错。
- `-h/--help` 显示帮助。
- `--format auto|flat|elf` 自动识别格式，或指定原始二进制、ELF 格式。
- `--platform bare|uart` 选择仅有 DRAM 的平台，或 DRAM 加 UART 的平台。
- `--dram-base`、`--dram-size`、`--uart-base` 和 `--entry` 分别设置内存基址、容量、UART 基址和程序入口。
- `--uart-base` 不能与 `--platform bare` 同时使用。数值格式错误会指出对应选项。
- `--max-steps` 默认上限为 1,000,000 步，`unlimited` 表示不限制步数。
- `--debug off|pc|full` 分别表示不输出调试信息、输出 PC，或输出 PC、寄存器和 CSR。

启动前会检查 UART 与 DRAM 是否重叠、地址是否溢出、入口是否位于 DRAM 内且按 2 字节对齐，以及初始栈对齐后是否低于 DRAM 起点。

## 命令格式

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

数值支持十进制、以 `0x` 开头的十六进制和下划线分隔。DRAM 容量还支持 `K/M/G` 与 `KiB/MiB/GiB` 后缀；两组后缀都按 1024 进位，例如 `1K` 和 `1KiB` 都是 1024 字节。

## 退出状态

| 退出码 | 含义 |
| --- | --- |
| 0 | 显示帮助，或达到设定的步数上限 |
| 1 | 镜像加载失败，或运行接口返回执行错误 |
| 2 | 参数或平台配置错误 |

当前 RISC-V 异常都会进入被模拟程序的 `mtvec/stvec` 处理入口，不会直接结束模拟器。退出码 1 中的执行错误情况主要为后续功能预留。

## 依赖关系与限制

`loader` 负责识别和加载镜像，`Platform` 检查地址布局并添加设备，`Machine` 负责复位和运行。

`bare` 与 `uart` 只包含基本内存或串口，不含 CLINT、PLIC 和 Virtio。完整设备组合可通过库中的 `VirtPlatform` 或 `Platform::attach_device` 使用，主命令行尚无对应的平台和磁盘选项。

被模拟程序没有专门通知模拟器正常退出的机制，当前只能通过步数上限停止。程序未设置有效异常入口时，CPU 可能反复跳到无效地址。参数使用 `std::env::args()` 读取，只支持 UTF-8 路径。DRAM 一次性分配；容量过大导致主机内存不足时，进程可能直接结束，目前没有稀疏内存或内存分配失败后的恢复机制。
