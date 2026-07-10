# `src/loader.rs`：镜像装载器

## 功能与实现思路

提供独立于 CLI 的 flat binary 和 ELF64 装载 API。`Auto` 模式按 ELF magic 自动选择格式；flat 镜像复制到 DRAM 基址；ELF 按 `PT_LOAD` program header 把文件内容写入物理地址（`p_paddr=0` 时使用 `p_vaddr`），并清零 `p_memsz-p_filesz` 的 BSS 区域。

## 当前状态

已实现 RV64 所需的 ELF64、小端、RISC-V machine 检查，以及 header/segment 截断、整数溢出、文件尺寸、DRAM 边界、loadable segment 和入口范围检查。没有第三方解析依赖。

## 对外接口

- `ImageFormat::{Auto, Flat, Elf}`。
- `LoadedImage { format, entry, loaded_bytes }`。
- `LoadError::{Io, InvalidImage}`，实现 `Display` 和 `Error`。
- `load_image(&mut Dram, path, format)`。
- `load_image_bytes(&mut Dram, bytes, format)`。

## 耦合方式

装载器只依赖 `dram::Dram` 和标准库；CLI 使用返回的 entry 初始化 CPU。ELF 装载位置必须落入调用方创建的 DRAM 布局。

## 剩余边界

- 只处理 ELF64 little-endian RISC-V 的 `PT_LOAD`，不解析 section、symbol、relocation、动态链接或设备树。
- 不检查 ELF 类型、segment flags、对齐约束和相互覆盖。
- flat binary 不携带入口，默认使用 DRAM 基址，调用方可通过 CLI `--entry` 覆盖。
