# `src/loader.rs`：镜像装载器

## 功能与实现

提供独立于命令行程序的裸二进制和 ELF64 镜像装载接口。`Auto` 根据 ELF 文件标识自动识别格式；裸二进制镜像从 DRAM 基址开始复制；ELF 根据 `PT_LOAD` 程序头写入物理地址，只有全部可装载段的 `p_paddr` 都为零时才统一改用 `p_vaddr`，并清零 `p_memsz-p_filesz` 对应的 BSS 区域。

## 实现状态

已经检查 ELF64、小端格式和 RISC-V 机器类型，也会检查文件头与程序段是否截断、整数是否溢出、段是否超出文件或 DRAM、是否存在可装载段，以及原始入口数值是否位于 DRAM。宿主文件读取失败归为 `LoadError::Io`，镜像或 BSS 超出 DRAM 则归为 `InvalidImage`。解析过程不依赖第三方库。入口检查目前只适用于入口虚拟地址与实际装载物理地址相同的镜像。

## 公共接口

- `ImageFormat::{Auto, Flat, Elf}`。
- `LoadedImage { format, entry, loaded_bytes }`。
- `LoadError::{Io, InvalidImage}`，实现 `Display` 和 `Error`。
- `load_image(&mut Dram, path, format)`。
- `load_image_bytes(&mut Dram, bytes, format)`。

## 依赖关系

装载器只依赖 `dram::Dram` 和标准库；命令行程序使用返回的入口地址初始化 CPU。ELF 装载位置必须落入调用方创建的 DRAM 区域。

## 已知限制

- 只处理小端 RISC-V ELF64 的 `PT_LOAD`，不解析节、符号、重定位、动态链接或设备树。
- 不检查 ELF 类型、段权限、对齐和段重叠。入口只需位于 DRAM，不要求落在已装载且可执行的段中。
- `e_entry` 是虚拟地址；当装载器选择非零 `p_paddr` 写入段时，当前不会按入口所在段的 `p_vaddr -> p_paddr` 偏移转换复位地址。非恒等布局可能被拒绝，或从未装载的位置启动。
- 装载过程不是原子的：如果后面的段无效，前面的段已经写入 DRAM。命令行程序会丢弃整个平台，但库调用方需要自行处理部分写入。
- 裸二进制镜像不携带入口地址，默认从 DRAM 基址启动；调用方可通过命令行参数 `--entry` 覆盖。
