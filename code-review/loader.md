# ELF 装载审查与修复

2026-09-05 已修复非恒等虚拟/物理布局下的入口错误，并补充装载失败的原子性。

入口按所在可执行 `PT_LOAD` 段转换为实际装载地址。例如 `p_vaddr=0x400000`、`p_paddr=0x80000000`、`e_entry=0x400100`，最终入口为 `0x80000100`。入口不属于可执行装载段、映射有歧义或不满足两字节对齐时拒绝镜像。

装载器先检查 ELF64、小端、RISC-V、`ET_EXEC`、版本、全部段范围和入口，再写入文件内容并清零 BSS。后续段或入口无效不会留下已修改的 RAM。各段的物理地址选择仍采用镜像级回退规则：全部 `p_paddr` 为零时使用虚拟地址。

回归测试覆盖高虚拟地址到低物理地址的入口转换、混合物理地址、不可执行入口、段外入口以及失败时 RAM 不变。动态链接和重定位等限制见 [装载器文档](../docs/repository-status/loader.md)。

依据：[ELF Header](https://gabi.xinuos.com/elf/02-eheader.html)、[Program Header](https://gabi.xinuos.com/elf/07-pheader.html)。
