//! 连续、按小端解释的 DRAM 设备。
//!
//! [`Dram`] 用字节向量保存物理内存。镜像装载负责边界检查，普通总线访问则把越界映射成 RISC-V 访问错误。

use crate::paging::PAGE_SIZE;
use crate::{
    bus::{MemDevice, valid_access_size},
    trap::Exception,
};
use std::ops::Range;

/// 一段从 `base` 开始的连续物理内存。
pub struct Dram {
    pub(crate) dram: Vec<u8>,
    pub(crate) base: u64,
    page_epochs: Vec<u64>,
}

impl Dram {
    /// 使用 [`crate::cfg`] 中的默认基址和容量创建清零内存。
    pub fn new() -> Self {
        Self::with_layout(crate::cfg::DRAM_BASE, crate::cfg::DRAM_SIZE)
    }

    /// 使用指定布局创建清零内存。
    pub fn with_layout(base: u64, size: usize) -> Self {
        Dram {
            dram: vec![0; size],
            base,
            // 基址可以不按页对齐，此时首尾最多多占一页。
            page_epochs: vec![0; size.div_ceil(PAGE_SIZE as usize) + 1],
        }
    }

    /// 返回半开地址区间的末端；地址加法溢出时返回 `None`。
    pub fn end(&self) -> Option<u64> {
        self.base.checked_add(self.dram.len() as u64)
    }

    pub const fn base(&self) -> u64 {
        self.base
    }

    /// 只读访问内存；写入应使用受检方法，以便同步使 LR/SC 保留失效。
    pub fn bytes(&self) -> &[u8] {
        &self.dram
    }

    /// 将字节复制到指定物理地址，要求整个范围都位于 DRAM 内。
    pub fn load_bytes(&mut self, addr: u64, bytes: &[u8]) -> Result<(), std::io::Error> {
        self.write_bytes(addr, bytes)
            .map_err(|_| std::io::Error::other("image segment is outside DRAM"))
    }

    /// 清零指定物理范围，主要用于 ELF 中 `memsz` 大于 `filesz` 的部分。
    pub fn zero_range(&mut self, addr: u64, len: usize) -> Result<(), std::io::Error> {
        let range = self
            .byte_range(addr, len)
            .ok_or_else(|| std::io::Error::other("image segment is outside DRAM"))?;
        self.dram[range].fill(0);
        self.record_write(addr, len);
        Ok(())
    }

    /// 为 DMA 设备复制一段任意长度的物理内存。
    pub fn read_bytes(&self, addr: u64, output: &mut [u8]) -> Result<(), Exception> {
        let range = self
            .byte_range(addr, output.len())
            .ok_or(Exception::LoadAccessFault(addr))?;
        output.copy_from_slice(&self.dram[range]);
        Ok(())
    }

    /// 接收 DMA 设备写回的一段任意长度物理内存。
    pub fn write_bytes(&mut self, addr: u64, input: &[u8]) -> Result<(), Exception> {
        let range = self
            .byte_range(addr, input.len())
            .ok_or(Exception::StoreAMOAccessFault(addr))?;
        self.dram[range].copy_from_slice(input);
        self.record_write(addr, input.len());
        Ok(())
    }

    /// 将 flat binary 装载到 DRAM 基址。
    pub fn load(&mut self, filename: &str) -> Result<(), std::io::Error> {
        self.load_bytes(self.base, &std::fs::read(filename)?)
    }

    fn access_range(&self, addr: u64, size: usize) -> Option<Range<usize>> {
        if !valid_access_size(size) {
            return None;
        }
        self.byte_range(addr, size)
    }

    /// 无副作用地检查任意字节范围，供镜像装载和 DMA 预检查共用。
    pub fn contains_range(&self, addr: u64, len: usize) -> bool {
        self.byte_range(addr, len).is_some()
    }

    fn byte_range(&self, addr: u64, size: usize) -> Option<Range<usize>> {
        addr.checked_add(u64::try_from(size).ok()?)?;
        let offset = usize::try_from(addr.checked_sub(self.base)?).ok()?;
        let end = offset.checked_add(size)?;
        (end <= self.dram.len()).then_some(offset..end)
    }

    fn page_index(&self, addr: u64) -> usize {
        (addr / PAGE_SIZE - self.base / PAGE_SIZE) as usize
    }

    fn record_write(&mut self, addr: u64, len: usize) {
        if len == 0 {
            return;
        }
        let first = self.page_index(addr);
        let last = self.page_index(addr + len as u64 - 1);
        for epoch in &mut self.page_epochs[first..=last] {
            *epoch = epoch.wrapping_add(1);
        }
    }
}

impl MemDevice for Dram {
    fn reservation_epoch(&mut self, addr: u64, size: usize) -> Option<u64> {
        self.access_range(addr, size)?;
        if !matches!(size, 4 | 8) || addr & (size as u64 - 1) != 0 {
            return None;
        }
        Some(self.page_epochs[self.page_index(addr)])
    }
    fn read(&mut self, addr: u64, size: usize) -> Result<u64, Exception> {
        let range = self
            .access_range(addr, size)
            .ok_or(Exception::LoadAccessFault(addr))?;

        let mut val = 0u64;
        for (i, byte) in self.dram[range].iter().enumerate() {
            // 低地址字节放到整数低位，保持 guest 可见的小端顺序。
            val |= u64::from(*byte) << (i * 8);
        }
        Ok(val)
    }

    fn write(&mut self, addr: u64, value: u64, size: usize) -> Result<(), Exception> {
        let range = self
            .access_range(addr, size)
            .ok_or(Exception::StoreAMOAccessFault(addr))?;

        for (i, byte) in self.dram[range].iter_mut().enumerate() {
            // 每次只取对应字节，避免宿主端字节序影响模拟结果。
            *byte = ((value >> (i * 8)) & 0xff) as u8;
        }
        self.record_write(addr, size);
        Ok(())
    }
}

impl Default for Dram {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dram_read_write() {
        let mut dram = Dram::new();

        assert!(dram.write(dram.base, 0x12345678, 4).is_ok());
        let val = dram.read(dram.base, 4).unwrap();
        assert_eq!(val, 0x12345678);

        assert!(dram.write(dram.base + 4, 0x9abc, 2).is_ok());
        let val = dram.read(dram.base + 4, 2).unwrap();
        assert_eq!(val, 0x9abc);

        assert!(dram.write(dram.base + 6, 0xde, 1).is_ok());
        let val = dram.read(dram.base + 6, 1).unwrap();
        assert_eq!(val, 0xde);

        assert!(dram.write(dram.base + 8, 0x0123_4567_89ab_cdef, 8).is_ok());
        assert_eq!(dram.read(dram.base + 8, 8).unwrap(), 0x0123_4567_89ab_cdef);

        assert!(
            dram.read(dram.base + crate::cfg::DRAM_SIZE as u64, 4)
                .is_err()
        );
        assert!(
            dram.write(dram.base + crate::cfg::DRAM_SIZE as u64, 0x1234, 2)
                .is_err()
        );
    }

    #[test]
    fn test_dram_rejects_invalid_or_partial_accesses_without_modification() {
        let base = 0x8000_0000;
        let mut dram = Dram::with_layout(base, 8);
        dram.dram.copy_from_slice(&[0, 1, 2, 3, 4, 5, 6, 7]);
        let original = dram.dram.clone();

        for size in [0, 3, 9, usize::MAX] {
            assert_eq!(dram.read(base, size), Err(Exception::LoadAccessFault(base)));
            assert_eq!(
                dram.write(base, u64::MAX, size),
                Err(Exception::StoreAMOAccessFault(base))
            );
        }
        assert_eq!(
            dram.write(base + 4, u64::MAX, 8),
            Err(Exception::StoreAMOAccessFault(base + 4))
        );
        assert_eq!(
            dram.read(u64::MAX, 1),
            Err(Exception::LoadAccessFault(u64::MAX))
        );
        assert_eq!(dram.dram, original);
    }

    #[test]
    fn byte_accesses_reject_physical_address_overflow() {
        let mut dram = Dram::with_layout(u64::MAX - 3, 8);
        let mut output = [0x5a; 8];
        assert!(!dram.contains_range(u64::MAX - 3, 8));
        assert!(dram.read_bytes(u64::MAX - 3, &mut output).is_err());
        assert!(dram.write_bytes(u64::MAX - 3, &[1; 8]).is_err());
        assert!(dram.zero_range(u64::MAX - 3, 8).is_err());
        assert_eq!(output, [0x5a; 8]);
        assert_eq!(dram.dram, vec![0; 8]);
    }

    #[test]
    fn test_dram_load() {
        let path =
            std::env::temp_dir().join(format!("arvsim-dram-load-{}.bin", std::process::id()));
        std::fs::write(&path, [0x93, 0x0f, 0xa0, 0x02]).unwrap();

        let mut dram = Dram::new();
        let result = dram.load(path.to_str().unwrap());
        assert!(result.is_ok());
        let val = dram.read(dram.base, 4).unwrap();
        assert_eq!(val, 0x02a00f93);

        std::fs::remove_file(path).unwrap();
    }
}
