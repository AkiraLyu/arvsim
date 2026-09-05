//! Sv39 的页大小、页表字段与页号编码，供 CPU 和内存保留检查共用。

pub(crate) const PAGE_SIZE: u64 = 4096;
pub(crate) const SV39_ROOT_LEVEL: u8 = 2;
pub(crate) const PTE_VALID: u64 = 1 << 0;
pub(crate) const PTE_READ: u64 = 1 << 1;
pub(crate) const PTE_WRITE: u64 = 1 << 2;
pub(crate) const PTE_EXECUTE: u64 = 1 << 3;
pub(crate) const PTE_USER: u64 = 1 << 4;
pub(crate) const PTE_ACCESSED: u64 = 1 << 6;
pub(crate) const PTE_DIRTY: u64 = 1 << 7;
pub(crate) const PTE_PPN_MASK: u64 = (1 << 44) - 1;
pub(crate) const PTE_RESERVED_SHIFT: u32 = 54;

pub(crate) fn vpn_index(level: u64, va: u64) -> u64 {
    (va >> (12 + 9 * level)) & 0x1ff
}

pub(crate) fn pte_to_pa(pte: u64) -> u64 {
    // PTE 的物理页号从 bit10 开始，恢复地址时重新补上 12 位页内零偏移。
    ((pte >> 10) & PTE_PPN_MASK) << 12
}

pub(crate) fn pa_to_pte(pa: u64) -> u64 {
    // 物理地址必须按页编码；先移除页内偏移，再放到 PTE 的 PPN 位段。
    (pa >> 12) << 10
}
