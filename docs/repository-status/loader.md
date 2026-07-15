# `src/loader.rs`：镜像装载器

## 功能与实现

提供独立于命令行程序的裸二进制和 ELF64 镜像装载接口。`Auto` 根据 ELF 文件标识自动识别格式；裸二进制镜像从 DRAM 基址开始复制；ELF 根据 `PT_LOAD` 程序头写入物理地址，当 `p_paddr=0` 时改用 `p_vaddr`，并清零 `p_memsz-p_filesz` 对应的 BSS 区域。

## 实现状态

已经检查 ELF64、小端格式和 RISC-V 机器类型，也会检查文件头与程序段是否截断、整数是否溢出、段是否超出文件或 DRAM、是否存在可装载段，以及入口是否位于 DRAM。解析过程不依赖第三方库。

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
- 装载过程不是原子的：如果后面的段无效，前面的段已经写入 DRAM。命令行程序会丢弃整个平台，但库调用方需要自行处理部分写入。
- 段或裸二进制镜像越界时，`Dram` 返回 `std::io::Error`，随后被归入 `LoadError::Io`。这会把镜像布局错误和宿主文件读写错误混在一起。
- 裸二进制镜像不携带入口地址，默认从 DRAM 基址启动；调用方可通过命令行参数 `--entry` 覆盖。
