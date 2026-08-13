//! 连续、按小端解释的 DRAM 设备。
//!
//! [`Dram`] 用字节向量保存物理内存。镜像装载负责边界检查，普通总线访问则把越界映射成 RISC-V 访问错误。

use crate::{
    bus::{MemDevice, valid_access_size},
    trap::Exception,
};
use std::ops::Range;

/// 一段从 `base` 开始的连续物理内存。
pub struct Dram {
    pub dram: Vec<u8>,
    pub base: u64,
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
        }
    }

    /// 返回半开地址区间的末端；地址加法溢出时返回 `None`。
    pub fn end(&self) -> Option<u64> {
        self.base.checked_add(self.dram.len() as u64)
    }

    /// 将字节复制到指定物理地址，要求整个范围都位于 DRAM 内。
    pub fn load_bytes(&mut self, addr: u64, bytes: &[u8]) -> Result<(), std::io::Error> {
        let offset = addr
            .checked_sub(self.base)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| std::io::Error::other("image address is below DRAM base"))?;
        let end = offset
            .checked_add(bytes.len())
            .filter(|end| *end <= self.dram.len())
            .ok_or_else(|| std::io::Error::other("image segment exceeds DRAM size"))?;
        self.dram[offset..end].copy_from_slice(bytes);
        Ok(())
    }

    /// 清零指定物理范围，主要用于 ELF 中 `memsz` 大于 `filesz` 的部分。
    pub fn zero_range(&mut self, addr: u64, len: usize) -> Result<(), std::io::Error> {
        let offset = addr
            .checked_sub(self.base)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| std::io::Error::other("image address is below DRAM base"))?;
        let end = offset
            .checked_add(len)
            .filter(|end| *end <= self.dram.len())
            .ok_or_else(|| std::io::Error::other("image segment exceeds DRAM size"))?;
        self.dram[offset..end].fill(0);
        Ok(())
    }

    /// 为 DMA 设备复制一段任意长度的物理内存。
    pub fn read_bytes(&self, addr: u64, output: &mut [u8]) -> Result<(), Exception> {
        let offset = addr
            .checked_sub(self.base)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or(Exception::LoadAccessFault(addr))?;
        let end = offset
            .checked_add(output.len())
            .filter(|end| *end <= self.dram.len())
            .ok_or(Exception::LoadAccessFault(addr))?;
        output.copy_from_slice(&self.dram[offset..end]);
        Ok(())
    }

    /// 接收 DMA 设备写回的一段任意长度物理内存。
    pub fn write_bytes(&mut self, addr: u64, input: &[u8]) -> Result<(), Exception> {
        let offset = addr
            .checked_sub(self.base)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or(Exception::StoreAMOAccessFault(addr))?;
        let end = offset
            .checked_add(input.len())
            .filter(|end| *end <= self.dram.len())
            .ok_or(Exception::StoreAMOAccessFault(addr))?;
        self.dram[offset..end].copy_from_slice(input);
        Ok(())
    }

    /// 将 flat binary 装载到 DRAM 基址。
    pub fn load(&mut self, filename: &str) -> Result<(), std::io::Error> {
        use std::fs::File;
        use std::io::Read;
        let mut file = File::open(filename)?;
        let mut buffer = Vec::new();

        file.read_to_end(&mut buffer)?;

        self.load_bytes(self.base, &buffer)
    }

    fn access_range(&self, addr: u64, size: usize) -> Option<Range<usize>> {
        if !valid_access_size(size) {
            return None;
        }
        addr.checked_add(u64::try_from(size).ok()?)?;
        let offset = usize::try_from(addr.checked_sub(self.base)?).ok()?;
        let end = offset.checked_add(size)?;
        (end <= self.dram.len()).then_some(offset..end)
    }
}

impl MemDevice for Dram {
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
