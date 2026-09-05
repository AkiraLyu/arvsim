# `src/lib.rs`：库入口

公开模块包括 `bus`、`cfg`、`clint`、`cpu`、`csr`、`dram`、`instruction`、`interrupt`、`loader`、`machine`、`plic`、`trap`、`uart`、`virt_platform`、`virtio` 和 `xv6`。

`paging` 统一定义页表相关常量和函数，仅供库内部使用。xv6 内核加速也放在 CPU 的内部子模块中，其配置类型仍通过 `cpu` 公开。`clint` 目前只有模块名，尚无本地中断控制器实现。

库不依赖第三方 Rust 库。普通镜像通过 `Platform` 与 `Machine` 加载和运行，需要完整设备的平台使用 `VirtPlatform`。xv6 的运行示例见 `examples/xv6.rs`。
