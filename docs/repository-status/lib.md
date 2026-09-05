# `src/lib.rs`：库入口

公开模块为 `bus`、`cfg`、`clint`、`cpu`、`csr`、`dram`、`instruction`、`interrupt`、`loader`、`machine`、`plic`、`trap`、`uart`、`virt_platform`、`virtio` 和 `xv6`。

`paging` 是私有的页表定义模块；xv6 内核加速位于 CPU 的私有子模块，配置类型仍通过 `cpu` 导出。`clint` 仍是占位，不能据此认为已支持本地中断控制器。

库不依赖第三方 Rust 包。普通镜像通过 `Platform` 与 `Machine` 装载运行，完整设备平台使用 `VirtPlatform`；xv6 的可运行示例见 `examples/xv6.rs`。
