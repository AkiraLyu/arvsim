# `tests/support/mod.rs`：测试机器和 xv6 设备模型

## 设计

- 测试支撑代码提供比正式 `Bus` 更完整的机器模型，用于集成测试和 xv6 验收测试。
- 它把 RAM、UART、PLIC、virtio-mmio 块设备、MMIO 日志和 xv6 镜像加载集中在一个 `TestBus` 中。

## 实现

- `TestBusState` 保存 RAM、UART 输出、UART 输入队列、PLIC 寄存器、待处理中断位、磁盘镜像、virtio 状态和 MMIO 日志。
- `TestBus` 用 `Rc<RefCell<TestBusState>>` 共享状态，使测试可以在 CPU 运行后检查 UART 输出或注入输入。
- RAM 地址范围使用 `cfg::DRAM_BASE`，冒烟测试默认 1 MiB，xv6 测试使用 128 MiB。
- UART：
  - 读 RBR 返回输入队列当前字节。
  - 读 LSR 返回发送空闲位，并在有输入时返回接收就绪位。
  - 写 THR 追加到 `uart_output`。
- PLIC：
  - 基址 `0x0c000000`，大小 `0x04000000`。
  - 覆盖 UART IRQ 10 的 pending、enable、priority、threshold、claim/complete。
  - `pending_interrupt()` 在 PLIC 可 claim 时返回监督模式外部中断 `scause`。
- virtio-mmio 块设备：
  - 基址 `0x10001000`，大小 `0x1000`。
  - 队列大小固定为 8。
  - 支持读取 magic、version、device id、vendor id、队列配置、状态和中断状态。
  - 写 `QueueNotify` 时解析描述符链，处理块读和块写，更新 used ring，并置位 virtio 中断状态。
- xv6 辅助：
  - 从 `target/testbench/xv6-riscv` 加载 `kernel.bin` 和 `fs.img`。
  - 使用 `riscv64-elf-nm` 查找 `tx_busy`，找不到时回退到固定地址，避免测试 UART 缺少真实发送中断导致 xv6 卡住。

## 接口

- `TestBus`
  - `new(ram_size)`
  - `rv64_smoke()`
  - `xv6_sized()`
  - `state()`
  - `load_flat_binary(path)`
  - `load_disk_image(path)`
- `TestBusState`
  - `queue_uart_input(bytes)`
  - `uart_output_string()`
  - `mmio_log()`
- `TestMachine`
  - `from_bus(bus)`
  - `with_flat_binary(path, ram_size)`
  - `run_steps(max_steps)`
  - `queue_uart_input(input)`
  - `run_until_uart_contains(needle, max_steps)`
  - `require_uart_contains(label, needle, max_steps)`
  - `require_uart_lacks(forbidden)`
- xv6 工具函数：
  - `project_root()`
  - `testbench_target_dir()`
  - `build_flat_asm(name, asm)`
  - `xv6_kernel_bin()`
  - `xv6_kernel_elf()`
  - `xv6_fs_img()`
  - `require_xv6_fixture()`
  - `xv6_machine()`
  - `require_tool(tool)`
  - `run(command)`

## 限制

- 这是测试支撑模型，不是正式 QEMU `virt` 机器实现。
- PLIC 只覆盖 xv6 当前使用的 UART IRQ 10 路径。
- virtio 队列大小固定，特性协商和设备行为只覆盖 xv6 的块设备访问路径。
- 使用 `Rc<RefCell<_>>`，适合单线程测试，不是并发设备模型。
