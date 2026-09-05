# `src/loader.rs`：镜像装载

装载器支持裸二进制和小端 RISC-V ELF64 可执行文件。裸二进制从 DRAM 基址开始装入；ELF 装入 `PT_LOAD` 段并清零 BSS。只有所有可装载段的 `p_paddr` 都为零时，才统一改用 `p_vaddr`。

ELF 入口必须属于可执行段，并按该段的虚拟地址与物理装载地址转换为初始 PC。入口还须满足两字节对齐；有冲突的入口映射会被拒绝。段范围、文件范围、整数溢出和入口均在写入前检查，因此非法镜像不会留下部分装载内容。字段含义依据 [ELF 程序装载规范](https://gabi.xinuos.com/elf/07-pheader.html)。

公共接口为 `load_image`、`load_image_bytes`、`ImageFormat`、`LoadedImage` 和 `LoadError`。`LoadedImage.entry` 是物理入口；文件读取错误属于 `Io`，镜像布局错误属于 `InvalidImage`。

测试覆盖非恒等地址映射、物理地址零值、BSS、不可执行或未对齐入口，以及后续坏段导致的整体拒绝。当前不支持重定位、动态链接、符号解析或设备树；重叠装载段仍按程序头顺序写入，不提供通用进程装载语义。
