# `src/virt_platform.rs`：正式 `virt` 平台组装器

## 功能与实现

`VirtPlatform` 在通用 `Platform` 上组装单 hart QEMU `virt` 风格设备：共享 DRAM、两个 PLIC 上下文（M 外部中断与 S 外部中断）、16550 UART 和 Virtio MMIO 块设备。UART 与块设备从 PLIC 取得独立 `InterruptLine`，virtio 从 `Platform::dram_handle` 取得同一 DRAM 的 DMA 视图。所有地址、窗口、IRQ、源数、优先级、发送延迟、queue 上限和 vendor id 都由 `VirtPlatformConfig` 提供。

## 公共接口

- `VirtPlatformConfig` 与默认 QEMU `virt` 风格配置。
- `VirtPlatform::{new, dram_mut, devices, build}`。
- `VirtPlatformDevices` 的 DRAM、PLIC、UART、块设备共享句柄访问器。
- `VirtMachine { machine, devices }`，并可解引用为 `Machine`。
- `VirtPlatformError`。

## 依赖关系

组装器依赖 `Platform` 完成区域、复位向量和栈检查；依赖正式 `Dram`、`Plic`、`Uart` 与 `VirtioBlock`。宿主 UART 与块介质后端由调用方注入，平台不依赖测试模块。

## 已知限制

- 当前 CPU 只有一个 hart，因此默认只创建 M/S 两个 PLIC 上下文。
- 尚未组装 CLINT/ACLINT、RTC、设备树或固件；直接启动的 guest 必须自行满足现有 CPU 初始化约定。
- 命令行主程序尚未暴露完整 `virt` preset；xv6 测试与交互脚本已经使用该正式库入口。
