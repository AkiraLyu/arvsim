# `src/virt_platform.rs`：创建 `virt` 平台

`VirtPlatform` 在通用 `Platform` 上创建单硬件线程的 QEMU `virt` 风格平台，包括共享 DRAM、PLIC、16550 UART 和 Virtio MMIO 块设备。

PLIC 有两个中断目标，分别对应机器模式和监督模式。UART 和块设备各有独立的 `InterruptLine`，Virtio 通过 `Platform::dram_handle` 访问同一块 DRAM。设备地址范围、中断号、中断源数量、优先级、发送延迟、队列上限和厂商编号均由 `VirtPlatformConfig` 配置。

## 公开接口

- `VirtPlatformConfig`：默认采用 QEMU `virt` 风格配置。
- `VirtPlatform::{new, dram_mut, devices, build}`。
- `VirtPlatformDevices`：提供 DRAM、PLIC、UART 和块设备的共享访问方法。
- `VirtMachine { machine, devices }`：保存机器和设备，也可通过解引用调用 `Machine` 方法。
- `VirtPlatformError`：平台创建错误。

## 依赖关系与限制

平台使用库中的 `Dram`、`Plic`、`Uart` 和 `VirtioBlock`，由 `Platform` 检查地址区域、复位地址和初始栈。UART 输入输出和磁盘存储后端由调用方提供，不依赖测试模块。

当前只有一个硬件线程，默认只创建机器模式和监督模式两个 PLIC 中断目标。CLINT/ACLINT、实时时钟（RTC）、设备树和固件尚未接入；被模拟程序需要能够在当前 CPU 初始状态下直接启动。

主命令行尚未提供完整的 `virt` 配置。xv6 测试和 Cargo 示例通过库中的镜像加载与平台创建接口使用它。
