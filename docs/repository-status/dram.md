# `src/dram.rs`：主存

## 设计

- DRAM 是一个从 `cfg::DRAM_BASE` 开始的连续小端字节数组。
- 它既用于正式命令行运行，也用于基础单元测试。

## 实现

- `Dram::new()` 分配 `cfg::DRAM_SIZE` 字节并清零。
- `Dram::load(filename)` 把整个文件读入内存起始位置。
- `read()` 和 `write()` 按小端序处理 1、2、4、8 等字节宽度；实际写入接口接收 `u32`，因此 8 字节写需要上层拆分。
- 地址小于基址或超过 DRAM 范围时返回访问错误。

## 接口

- `struct Dram { dram: Vec<u8>, base: u64 }`
- `Dram::new()`
- `Dram::load(&mut self, filename: &str) -> Result<(), std::io::Error>`
- `impl MemDevice for Dram`

## 限制

- 只支持把二进制文件加载到 DRAM 起始位置，不解析 ELF。
- 没有内存权限、缓存、MMU 或设备重叠检查；这些由 CPU 地址翻译或总线布局承担。
