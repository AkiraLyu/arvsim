# `src/main.rs`：命令行入口

## 功能与实现思路

命令行入口负责解析运行参数、创建 `Platform`、装载 flat/ELF 镜像、按预设选择挂载 UART，再构建 `Machine` 并执行。参数解析不依赖第三方 crate，错误通过 `ExitCode` 显式返回。

## 当前状态

当前提供：

- 正确跳过 `argv[0]`，将唯一位置参数作为 guest 镜像路径。
- 支持 `-h/--help`、未知参数检查、缺值检查和重复镜像检查。
- `--format auto|flat|elf` 支持自动识别 ELF 或强制格式。
- `--platform bare|uart` 选择纯 DRAM 或 DRAM + UART 平台。
- `--dram-base`、`--dram-size`、`--uart-base` 和 `--entry` 提供平台/入口覆盖。
- `--max-steps` 默认限制为 1,000,000 步，也可设置 `unlimited`。
- `--debug off|pc|full` 控制无跟踪、PC 跟踪或完整寄存器/CSR 跟踪。
- CPU 异常、装载失败和参数错误均返回非零退出码；达到显式步数上限正常返回成功。
- UART 与 DRAM 区域重叠、地址溢出和入口超出 DRAM 会在组装前被拒绝。

## 对外接口

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

退出码约定：0 表示帮助或达到步数上限；1 表示镜像装载或 guest 执行异常；2 表示参数或平台配置错误。

## 耦合方式

- 依赖 `loader` 完成格式检测和镜像装载。
- 通过 `Platform` 应用 DRAM 布局、检查 MMIO 冲突并挂载 UART。
- 通过 `Machine` facade 启动 CPU；当前时钟和总线中断查询仍在 CPU 内部完成。
- `bare` 和 `uart` 仍是轻量平台，不包含 CLINT、PLIC 或 virtio；xv6 完整平台仍只存在测试支撑中。

## 剩余边界

- 没有 guest 主动 halt/exit 协议；目前正常停止点是步数上限，未被 trap 处理的 guest 异常返回失败。
- 参数解析使用 UTF-8 `std::env::args()`，不支持非 UTF-8 镜像路径。
- CLI 平台预设仍只覆盖 DRAM/UART；更完整的设备组合可通过库级 `Platform::attach_device` 扩展。
- 超大 DRAM 配置可能因宿主分配失败而由分配器终止，未提供稀疏内存或可恢复 OOM。
- `--dram-base` 虽能改变物理布局，但 CPU privilege 启发式和部分指令/xv6 加速仍引用默认 `cfg` 常量，复杂 guest 的自定义布局并非完全一致。
