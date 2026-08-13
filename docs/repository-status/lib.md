# `src/lib.rs`：库入口

## 功能

声明并公开库的十五个顶层模块，不做二次封装或统一导出。

## 实现状态

每个源文件都直接导出为公共模块。`plic`、`uart`、`virtio` 和 `virt_platform` 均已有正式实现；`clint` 仍是占位模块。

## 公共接口

公开模块为 `bus`、`cfg`、`clint`、`cpu`、`csr`、`dram`、`instruction`、`interrupt`、`loader`、`machine`、`plic`、`trap`、`uart`、`virt_platform`、`virtio`。

## 影响范围

本文件没有运行时逻辑，但它决定了调用方可以直接使用哪些实现细节，包括 `Cpu` 公共字段、CSR 常量和 `DeviceRegion`。

## 已知问题

- 没有按“稳定公共接口”和“内部实现”区分可见性。
- `clint` 空模块仍被公开，容易使调用方误判本地中断支持程度。
- 已有简短的库说明，但缺少可编译示例，也没有说明接口稳定性、输入的可信要求和兼容范围。
