# `src/clint.rs` 和 `src/plic.rs`：平台设备占位模块

## 设计

- 这两个文件当前只是库模块占位。
- 它们表达了后续把 CLINT/PLIC 从测试支撑代码迁移到正式模型的方向。

## 实现

- `src/clint.rs` 为空文件。
- `src/plic.rs` 为空文件。
- 当前正式库没有 CLINT 或 PLIC 设备类型。

## 接口

- 只有 `src/lib.rs` 中的模块导出：`pub mod clint;`、`pub mod plic;`。

## 限制

- xv6 当前能跑，是因为测试总线提供了 PLIC、定时器行为由 CPU 的 `TIME/STIMECMP` 简化逻辑提供。
- 正式命令行运行入口无法模拟 xv6 所需的 PLIC 和 virtio 设备。
