# `tests/cli.rs`：命令行进程测试

## 功能与实现思路

通过 Cargo 提供的 `CARGO_BIN_EXE_arvsim` 启动真实可执行文件，验证帮助输出、flat 镜像运行到步数上限，以及镜像缺失时的非零退出状态。测试使用进程专属临时文件，执行后立即清理。

## 当前状态

已实现 3 个默认执行的集成测试，覆盖 CLI 最重要的进程边界和 stdout/stderr 约定。

## 对外接口与耦合

测试无产品接口；依赖编译后的 `arvsim` binary、标准库 `Command` 和宿主临时目录。flat 镜像只包含一条 `addi x31, x0, 42`，以 `--max-steps 1` 正常停止。

## 剩余边界

尚未以进程方式覆盖 ELF、所有参数错误、调试输出、UART 平台、地址重叠和 guest exception 退出码。
