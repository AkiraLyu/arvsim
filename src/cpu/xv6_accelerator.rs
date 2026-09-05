//! 可选的 xv6 内核兼容加速；标准指令、特权级和页表翻译由父模块实现。
//! 所有长循环受单步预算限制，用户态始终执行原始指令。

use super::{Cpu, MemoryAccess, PrivilegeMode};
use crate::{csr, paging::*, trap::Exception};
use std::collections::BTreeSet;
use std::ops::Range;

/// xv6 专用快速路径所需的函数入口和全局对象地址。
///
/// 该配置默认关闭，必须与实际加载的 xv6 ELF 符号匹配；错误地址可能在普通指令中间误触发快速路径。
/// 结构体偏移和数组步长仍由本模块中的兼容性常量约束。
#[derive(Debug, Copy, Clone)]
pub struct Xv6Accelerator {
    pub mycpu: u64,
    pub holding: u64,
    pub push_off: u64,
    pub acquire: u64,
    pub pop_off: u64,
    pub release: u64,
    pub memcmp: u64,
    pub memmove: u64,
    pub strncmp: u64,
    pub strncpy: u64,
    pub strlen: u64,
    pub uvmunmap: u64,
    pub freewalk: u64,
    pub uvmcopy: u64,
    pub myproc: u64,
    pub wakeup: u64,
    pub cpus: u64,
    pub kmem: u64,
    pub kernel_end: u64,
    /// guest 内核使用的物理内存排他上界（xv6 的 `PHYSTOP`）。
    pub phys_top: u64,
    /// 加速访存可使用的实际 DRAM 半开区间起点。
    pub dram_base: u64,
    /// 加速访存可使用的实际 DRAM 半开区间终点。
    pub dram_end: u64,
    pub proc_start: u64,
    pub proc_end: u64,
}

impl Xv6Accelerator {
    /// 检查固定布局所需的地址范围；具体代码版本仍须由镜像装载方确认。
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.dram_base >= self.kernel_end
            || self.kernel_end >= self.phys_top
            || self.phys_top > self.dram_end
        {
            return Err("invalid xv6 DRAM or kernel range");
        }
        let within_kernel = |addr: u64, len: u64| {
            addr >= self.dram_base
                && addr
                    .checked_add(len)
                    .is_some_and(|end| end <= self.kernel_end)
        };
        if !within_kernel(self.cpus, XV6_CPU_STRIDE)
            || !within_kernel(self.kmem, XV6_KMEM_FREELIST + 8)
            || !within_kernel(self.proc_start, XV6_PROC_TABLE_SIZE)
            || self.proc_end.checked_sub(self.proc_start) != Some(XV6_PROC_TABLE_SIZE)
        {
            return Err("invalid xv6 global object layout");
        }
        for entry in [
            self.mycpu,
            self.holding,
            self.push_off,
            self.acquire,
            self.pop_off,
            self.release,
            self.memcmp,
            self.memmove,
            self.strncmp,
            self.strncpy,
            self.strlen,
            self.uvmunmap,
            self.freewalk,
            self.uvmcopy,
            self.myproc,
            self.wakeup,
        ] {
            if entry & 1 != 0 || !within_kernel(entry, 2) {
                return Err("xv6 function entry is outside the kernel or misaligned");
            }
        }
        Ok(())
    }
}

const XV6_CPU_STRIDE: u64 = 128;
const XV6_SPINLOCK_CPU: u64 = 16;
const XV6_CPU_NOFF: u64 = 120;
const XV6_CPU_INTENA: u64 = 124;
const XV6_KMEM_FREELIST: u64 = 24;
const XV6_PROC_STRIDE: u64 = 360;
const XV6_PROC_COUNT: u64 = 64;
/// 当前 xv6 兼容加速器所支持的进程表总字节数。
pub const XV6_PROC_TABLE_SIZE: u64 = XV6_PROC_COUNT * XV6_PROC_STRIDE;
const XV6_PROC_STATE: u64 = 24;
const XV6_PROC_CHAN: u64 = 32;
const XV6_PROC_SLEEPING: u32 = 2;
const XV6_PROC_RUNNABLE: u32 = 3;
const XV6_MAXVA: u64 = 1 << 38;
/// 可选 xv6 路径单次处理的最大字节数；超出后执行原始指令。
pub(crate) const XV6_FAST_PATH_MAX_BYTES: u64 = 1024 * 1024;
const XV6_FAST_PATH_MAX_PAGES: u64 = XV6_FAST_PATH_MAX_BYTES / PAGE_SIZE;

struct Xv6LeafMapping {
    pte_addr: u64,
    virtual_addr: u64,
    physical_addr: u64,
    flags: u64,
}

struct Xv6MappingWalk {
    physical_range: Option<(u64, u64)>,
    visited: BTreeSet<u64>,
    mappings: Vec<Xv6LeafMapping>,
}

impl Cpu {
    pub(super) fn try_xv6_fast_path(&mut self) -> Result<bool, Exception> {
        let Some(accelerator) = self.xv6_accelerator else {
            return Ok(false);
        };
        if self.privilege != PrivilegeMode::Supervisor {
            return Ok(false);
        }
        // 当前平台只有 hart 0，不能让客体 tp 把 cpus 索引到声明范围之外。
        if self.registers[4] != 0 {
            return Ok(false);
        }

        // 仅匹配已从当前 ELF 解析出的函数入口，避免在普通指令中间误触发。
        match self.pc {
            pc if pc == accelerator.mycpu => self.fast_xv6_mycpu(),
            pc if pc == accelerator.holding => self.fast_xv6_holding(),
            pc if pc == accelerator.push_off => self.fast_xv6_push_off(),
            pc if pc == accelerator.acquire => self.fast_xv6_acquire(),
            pc if pc == accelerator.pop_off => self.fast_xv6_pop_off(),
            pc if pc == accelerator.release => self.fast_xv6_release(),
            pc if pc == accelerator.memcmp => self.fast_xv6_memcmp(),
            pc if pc == accelerator.memmove => self.fast_xv6_memmove(),
            pc if pc == accelerator.strncmp => self.fast_xv6_strncmp(),
            pc if pc == accelerator.strncpy => self.fast_xv6_strncpy(),
            pc if pc == accelerator.strlen => self.fast_xv6_strlen(),
            pc if pc == accelerator.uvmunmap => self.fast_xv6_uvmunmap(),
            pc if pc == accelerator.freewalk => self.fast_xv6_freewalk(),
            pc if pc == accelerator.uvmcopy => self.fast_xv6_uvmcopy(),
            pc if pc == accelerator.myproc => self.fast_xv6_myproc(),
            pc if pc == accelerator.wakeup => self.fast_xv6_wakeup(),
            _ => Ok(false),
        }
    }

    fn fast_xv6_mycpu(&mut self) -> Result<bool, Exception> {
        self.registers[10] = self.xv6_cpu_addr();
        self.fast_return();
        Ok(true)
    }

    fn fast_xv6_myproc(&mut self) -> Result<bool, Exception> {
        self.registers[10] = self.read_u64(self.xv6_cpu_addr())?;
        self.fast_return();
        Ok(true)
    }

    fn fast_xv6_holding(&mut self) -> Result<bool, Exception> {
        let lock = self.registers[10];
        let locked = self.read_u32(lock)?;
        let owner = self.read_u64(lock.wrapping_add(XV6_SPINLOCK_CPU))?;
        self.registers[10] = (locked != 0 && owner == self.xv6_cpu_addr()) as u64;
        self.fast_return();
        Ok(true)
    }

    fn fast_xv6_push_off(&mut self) -> Result<bool, Exception> {
        self.fast_push_off_inline()?;
        self.fast_return();
        Ok(true)
    }

    fn fast_xv6_pop_off(&mut self) -> Result<bool, Exception> {
        self.fast_pop_off_inline()?;
        self.fast_return();
        Ok(true)
    }

    fn fast_xv6_acquire(&mut self) -> Result<bool, Exception> {
        self.fast_push_off_inline()?;
        let lock = self.registers[10];
        self.write_u32(lock, 1)?;
        self.write_u64(lock.wrapping_add(XV6_SPINLOCK_CPU), self.xv6_cpu_addr())?;
        self.fast_return();
        Ok(true)
    }

    fn fast_xv6_release(&mut self) -> Result<bool, Exception> {
        let lock = self.registers[10];
        self.write_u64(lock.wrapping_add(XV6_SPINLOCK_CPU), 0)?;
        self.write_u32(lock, 0)?;
        self.fast_pop_off_inline()?;
        self.fast_return();
        Ok(true)
    }

    fn fast_push_off_inline(&mut self) -> Result<(), Exception> {
        let old_sie = (self.csr.load(csr::SSTATUS) & csr::MASK_SIE != 0) as u32;
        let sstatus = self.csr.load(csr::SSTATUS) & !csr::MASK_SIE;
        self.csr.store(csr::SSTATUS, sstatus);

        let cpu = self.xv6_cpu_addr();
        let noff = self.read_u32(cpu.wrapping_add(XV6_CPU_NOFF))?;
        // 只在最外层关中断时保存原 SIE；嵌套层退出不能覆盖最初状态。
        if noff == 0 {
            self.write_u32(cpu.wrapping_add(XV6_CPU_INTENA), old_sie)?;
        }
        self.write_u32(cpu.wrapping_add(XV6_CPU_NOFF), noff.wrapping_add(1))
    }

    fn fast_pop_off_inline(&mut self) -> Result<(), Exception> {
        let cpu = self.xv6_cpu_addr();
        let noff = self.read_u32(cpu.wrapping_add(XV6_CPU_NOFF))?;
        let new_noff = noff.saturating_sub(1);
        self.write_u32(cpu.wrapping_add(XV6_CPU_NOFF), new_noff)?;
        if new_noff == 0 && self.read_u32(cpu.wrapping_add(XV6_CPU_INTENA))? != 0 {
            let sstatus = self.csr.load(csr::SSTATUS) | csr::MASK_SIE;
            self.csr.store(csr::SSTATUS, sstatus);
        }
        Ok(())
    }

    fn fast_xv6_memcmp(&mut self) -> Result<bool, Exception> {
        let lhs = self.registers[10];
        let rhs = self.registers[11];
        let len = self.registers[12] as u32 as usize;
        if len as u64 > XV6_FAST_PATH_MAX_BYTES {
            return Ok(false);
        }
        for i in 0..len {
            let a = self.read_u8(lhs.wrapping_add(i as u64))?;
            let b = self.read_u8(rhs.wrapping_add(i as u64))?;
            if a != b {
                self.registers[10] = ((a as i32) - (b as i32)) as i64 as u64;
                self.fast_return();
                return Ok(true);
            }
        }
        self.registers[10] = 0;
        self.fast_return();
        Ok(true)
    }

    fn fast_xv6_memmove(&mut self) -> Result<bool, Exception> {
        let dst = self.registers[10];
        let src = self.registers[11];
        let len = self.registers[12] as u32 as usize;
        if len as u64 > XV6_FAST_PATH_MAX_BYTES {
            return Ok(false);
        }
        // 先完整读取再写回，保证源、目标区间重叠时仍符合 memmove 语义。
        // 长度已受单步预算限制，临时缓冲最多占用 1 MiB。
        let mut bytes = Vec::with_capacity(len);
        for i in 0..len {
            bytes.push(self.read_u8(src.wrapping_add(i as u64))?);
        }
        for (i, byte) in bytes.into_iter().enumerate() {
            self.write_u8(dst.wrapping_add(i as u64), byte)?;
        }
        self.registers[10] = dst;
        self.fast_return();
        Ok(true)
    }

    fn fast_xv6_strncmp(&mut self) -> Result<bool, Exception> {
        let lhs = self.registers[10];
        let rhs = self.registers[11];
        let len = self.registers[12] as u32 as usize;
        if len as u64 > XV6_FAST_PATH_MAX_BYTES {
            return Ok(false);
        }
        for i in 0..len {
            let a = self.read_u8(lhs.wrapping_add(i as u64))?;
            let b = self.read_u8(rhs.wrapping_add(i as u64))?;
            if a == 0 || a != b {
                self.registers[10] = ((a as i32) - (b as i32)) as i64 as u64;
                self.fast_return();
                return Ok(true);
            }
        }
        self.registers[10] = 0;
        self.fast_return();
        Ok(true)
    }

    fn fast_xv6_strncpy(&mut self) -> Result<bool, Exception> {
        let dst = self.registers[10];
        let mut src = self.registers[11];
        let mut out = dst;
        let mut remaining = self.registers[12] as i32;
        if remaining > 0 && remaining as u64 > XV6_FAST_PATH_MAX_BYTES {
            return Ok(false);
        }

        while remaining > 0 {
            remaining -= 1;
            let byte = self.read_u8(src)?;
            self.write_u8(out, byte)?;
            out = out.wrapping_add(1);
            src = src.wrapping_add(1);
            if byte == 0 {
                break;
            }
        }
        while remaining > 0 {
            remaining -= 1;
            self.write_u8(out, 0)?;
            out = out.wrapping_add(1);
        }

        self.registers[10] = dst;
        self.fast_return();
        Ok(true)
    }

    fn fast_xv6_strlen(&mut self) -> Result<bool, Exception> {
        let base = self.registers[10];
        let mut len = 0u64;
        while self.read_u8(base.wrapping_add(len))? != 0 {
            len += 1;
            if len == XV6_FAST_PATH_MAX_BYTES {
                return Ok(false);
            }
        }
        self.registers[10] = len;
        self.fast_return();
        Ok(true)
    }

    fn fast_xv6_freewalk(&mut self) -> Result<bool, Exception> {
        let pagetable = self.registers[10];
        let mut visited = BTreeSet::new();
        if !self.freewalk_is_safe(pagetable, SV39_ROOT_LEVEL, &mut visited)? {
            return Ok(false);
        }
        if !self.freewalk_page_table(pagetable, SV39_ROOT_LEVEL)? {
            return Ok(false);
        }
        if !self.xv6_kfree_page(pagetable)? {
            return Ok(false);
        }
        self.fast_return();
        Ok(true)
    }

    fn fast_xv6_uvmunmap(&mut self) -> Result<bool, Exception> {
        let pagetable = self.registers[10];
        let va = self.registers[11];
        let npages = self.registers[12];
        let do_free = self.registers[13] != 0;
        // xv6 页表操作要求起始虚拟地址页对齐；不满足时退回真实 guest 实现处理。
        if va & (PAGE_SIZE - 1) != 0 {
            return Ok(false);
        }

        let Some(end) = npages
            .checked_mul(PAGE_SIZE)
            .and_then(|span| va.checked_add(span))
        else {
            return Ok(false);
        };
        let physical_range = if do_free {
            let Some(accelerator) = self.xv6_accelerator else {
                return Ok(false);
            };
            Some((accelerator.kernel_end, accelerator.phys_top))
        } else {
            None
        };
        let Some(mappings) = self.xv6_leaf_mappings(pagetable, va..end, physical_range)? else {
            return Ok(false);
        };
        for mapping in mappings {
            if do_free && !self.xv6_kfree_page(mapping.physical_addr)? {
                return Ok(false);
            }
            self.write_phys_u64(mapping.pte_addr, 0)?;
        }

        self.fast_return();
        Ok(true)
    }

    fn fast_xv6_uvmcopy(&mut self) -> Result<bool, Exception> {
        let old = self.registers[10];
        let new = self.registers[11];
        let sz = self.registers[12];
        let Some(accelerator) = self.xv6_accelerator else {
            return Ok(false);
        };
        let Some(mappings) = self.xv6_leaf_mappings(
            old,
            0..sz,
            Some((accelerator.dram_base, accelerator.dram_end)),
        )?
        else {
            return Ok(false);
        };

        for (index, mapping) in mappings.iter().enumerate() {
            let Some(mem) = self.xv6_kalloc_page()? else {
                self.xv6_unmap_copied_pages(new, &mappings[..index])?;
                self.registers[10] = u64::MAX;
                self.fast_return();
                return Ok(true);
            };
            self.copy_phys_page(mem, mapping.physical_addr)?;
            if !self.xv6_mappage(new, mapping.virtual_addr, mem, mapping.flags)? {
                let _ = self.xv6_kfree_page(mem)?;
                self.xv6_unmap_copied_pages(new, &mappings[..index])?;
                self.registers[10] = u64::MAX;
                self.fast_return();
                return Ok(true);
            }
        }

        self.registers[10] = 0;
        self.fast_return();
        Ok(true)
    }

    /// 只遍历实际存在的页表，先收集映射，再允许调用方修改内存。
    ///
    /// 惰性分配可以产生很大的虚拟空洞；预算按页表页和叶子映射计数，
    /// 不按虚拟跨度计数，既限制宿主工作量，也避免逐页扫描空区间。
    fn xv6_leaf_mappings(
        &mut self,
        pagetable: u64,
        range: Range<u64>,
        physical_range: Option<(u64, u64)>,
    ) -> Result<Option<Vec<Xv6LeafMapping>>, Exception> {
        if range.end < range.start || range.end > XV6_MAXVA {
            return Ok(None);
        }
        let mut walk = Xv6MappingWalk {
            physical_range,
            visited: BTreeSet::new(),
            mappings: Vec::new(),
        };
        if !range.is_empty()
            && !self.xv6_collect_mappings(pagetable, SV39_ROOT_LEVEL, range, &mut walk)?
        {
            return Ok(None);
        }
        Ok(Some(walk.mappings))
    }

    fn xv6_collect_mappings(
        &mut self,
        pagetable: u64,
        level: u8,
        range: Range<u64>,
        walk: &mut Xv6MappingWalk,
    ) -> Result<bool, Exception> {
        if pagetable & (PAGE_SIZE - 1) != 0
            || walk.visited.len() as u64 >= XV6_FAST_PATH_MAX_PAGES
            || !walk.visited.insert(pagetable)
        {
            return Ok(false);
        }
        let span = PAGE_SIZE << (9 * level);
        let table_base = range.start & !(span * (PAGE_SIZE / 8) - 1);
        let first = vpn_index(u64::from(level), range.start);
        let last = vpn_index(u64::from(level), range.end - 1);
        for index in first..=last {
            let Some(pte_addr) = pagetable.checked_add(index * 8) else {
                return Ok(false);
            };
            let pte = self.read_phys_u64(pte_addr)?;
            if pte & PTE_VALID == 0 {
                continue;
            }
            let virtual_addr = table_base + index * span;
            let physical_addr = pte_to_pa(pte);
            let leaf = pte & (PTE_READ | PTE_WRITE | PTE_EXECUTE) != 0;
            if level > 0 {
                // xv6 使用 4 KiB 叶子；超级页或重复下级表交给原始实现处理。
                if leaf
                    || !self.xv6_collect_mappings(
                        physical_addr,
                        level - 1,
                        range.start.max(virtual_addr)..range.end.min(virtual_addr + span),
                        walk,
                    )?
                {
                    return Ok(false);
                }
            } else {
                if !leaf || walk.mappings.len() as u64 >= XV6_FAST_PATH_MAX_PAGES {
                    return Ok(false);
                }
                if let Some((start, end)) = walk.physical_range
                    && (physical_addr < start
                        || physical_addr
                            .checked_add(PAGE_SIZE)
                            .is_none_or(|limit| limit > end))
                {
                    return Ok(false);
                }
                walk.mappings.push(Xv6LeafMapping {
                    pte_addr,
                    virtual_addr,
                    physical_addr,
                    flags: pte & 0x3ff,
                });
            }
        }
        Ok(true)
    }

    fn xv6_unmap_copied_pages(
        &mut self,
        pagetable: u64,
        mappings: &[Xv6LeafMapping],
    ) -> Result<(), Exception> {
        for mapping in mappings {
            if let Some(pte_addr) = self.xv6_walk(pagetable, mapping.virtual_addr, false)? {
                let pte = self.read_phys_u64(pte_addr)?;
                if pte & PTE_VALID != 0 {
                    let _ = self.xv6_kfree_page(pte_to_pa(pte))?;
                    self.write_phys_u64(pte_addr, 0)?;
                }
            }
        }
        Ok(())
    }

    fn freewalk_is_safe(
        &mut self,
        pagetable: u64,
        level: u8,
        visited: &mut BTreeSet<u64>,
    ) -> Result<bool, Exception> {
        let Some(accelerator) = self.xv6_accelerator else {
            return Ok(false);
        };
        if visited.len() as u64 >= XV6_FAST_PATH_MAX_PAGES
            || !visited.insert(pagetable)
            || pagetable & (PAGE_SIZE - 1) != 0
            || pagetable < accelerator.kernel_end
            || pagetable
                .checked_add(PAGE_SIZE)
                .is_none_or(|end| end > accelerator.phys_top)
        {
            return Ok(false);
        }
        for entry in 0..PAGE_SIZE / 8 {
            let pte = self.read_phys_u64(pagetable + entry * 8)?;
            if pte & PTE_VALID == 0 {
                continue;
            }
            if level == 0
                || pte & (PTE_READ | PTE_WRITE | PTE_EXECUTE) != 0
                || !self.freewalk_is_safe(pte_to_pa(pte), level - 1, visited)?
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn freewalk_page_table(&mut self, pagetable: u64, level: u8) -> Result<bool, Exception> {
        for entry in 0..PAGE_SIZE / 8 {
            let pte_addr = pagetable.wrapping_add(entry * 8);
            let pte = self.read_phys_u64(pte_addr)?;
            if pte & PTE_VALID == 0 {
                continue;
            }
            // freewalk 只释放中间页表；遇到叶子映射说明调用前置条件不成立。
            if pte & (PTE_READ | PTE_WRITE | PTE_EXECUTE) != 0 {
                return Ok(false);
            }
            // Sv39 的最低层不能再指向下级页表；更深的指针通常表示损坏或成环。
            if level == 0 {
                return Ok(false);
            }

            let child = pte_to_pa(pte);
            if !self.freewalk_page_table(child, level - 1)? {
                return Ok(false);
            }
            self.write_phys_u64(pte_addr, 0)?;
            if !self.xv6_kfree_page(child)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn xv6_kfree_page(&mut self, page: u64) -> Result<bool, Exception> {
        let accelerator = self.xv6_accelerator.expect("xv6 accelerator is configured");
        if page & (PAGE_SIZE - 1) != 0
            || !(accelerator.kernel_end..accelerator.phys_top).contains(&page)
        {
            return Ok(false);
        }

        for offset in (0..PAGE_SIZE).step_by(8) {
            self.write_phys_u64(page.wrapping_add(offset), 0x0101_0101_0101_0101)?;
        }

        let freelist = accelerator.kmem.wrapping_add(XV6_KMEM_FREELIST);
        let old_head = self.read_phys_u64(freelist)?;
        self.write_phys_u64(page, old_head)?;
        self.write_phys_u64(freelist, page)?;
        Ok(true)
    }

    fn xv6_kalloc_page(&mut self) -> Result<Option<u64>, Exception> {
        let accelerator = self.xv6_accelerator.expect("xv6 accelerator is configured");
        let freelist = accelerator.kmem.wrapping_add(XV6_KMEM_FREELIST);
        let page = self.read_phys_u64(freelist)?;
        if page == 0 {
            return Ok(None);
        }
        let next = self.read_phys_u64(page)?;
        self.write_phys_u64(freelist, next)?;
        for offset in (0..PAGE_SIZE).step_by(8) {
            self.write_phys_u64(page.wrapping_add(offset), 0x0505_0505_0505_0505)?;
        }
        Ok(Some(page))
    }

    fn xv6_walk(
        &mut self,
        mut pagetable: u64,
        va: u64,
        alloc: bool,
    ) -> Result<Option<u64>, Exception> {
        if va >= XV6_MAXVA {
            return Ok(None);
        }

        for level in (1..=2).rev() {
            let pte_addr = pagetable.wrapping_add(vpn_index(level, va).wrapping_mul(8));
            let pte = self.read_phys_u64(pte_addr)?;
            if pte & PTE_VALID != 0 {
                pagetable = pte_to_pa(pte);
            } else {
                if !alloc {
                    return Ok(None);
                }
                let Some(new_table) = self.xv6_kalloc_page()? else {
                    return Ok(None);
                };
                self.zero_phys_page(new_table)?;
                self.write_phys_u64(pte_addr, pa_to_pte(new_table) | PTE_VALID)?;
                pagetable = new_table;
            }
        }

        Ok(Some(
            pagetable.wrapping_add(vpn_index(0, va).wrapping_mul(8)),
        ))
    }

    fn xv6_mappage(
        &mut self,
        pagetable: u64,
        va: u64,
        pa: u64,
        perm: u64,
    ) -> Result<bool, Exception> {
        let Some(pte_addr) = self.xv6_walk(pagetable, va, true)? else {
            return Ok(false);
        };
        if self.read_phys_u64(pte_addr)? & PTE_VALID != 0 {
            return Ok(false);
        }
        self.write_phys_u64(pte_addr, pa_to_pte(pa) | perm | PTE_VALID)?;
        Ok(true)
    }

    fn zero_phys_page(&mut self, page: u64) -> Result<(), Exception> {
        for offset in (0..PAGE_SIZE).step_by(8) {
            self.write_phys_u64(page.wrapping_add(offset), 0)?;
        }
        Ok(())
    }

    fn copy_phys_page(&mut self, dst: u64, src: u64) -> Result<(), Exception> {
        for offset in (0..PAGE_SIZE).step_by(8) {
            let value = self.read_phys_u64(src.wrapping_add(offset))?;
            self.write_phys_u64(dst.wrapping_add(offset), value)?;
        }
        Ok(())
    }

    fn read_phys_u64(&mut self, addr: u64) -> Result<u64, Exception> {
        // xv6 将可用物理内存恒等映射到内核地址空间；仍走普通访存路径以保留页权限和 PMP 检查。
        self.read_u64(addr)
    }

    fn write_phys_u64(&mut self, addr: u64, value: u64) -> Result<(), Exception> {
        self.write_u64(addr, value)
    }

    fn fast_xv6_wakeup(&mut self) -> Result<bool, Exception> {
        let accelerator = self.xv6_accelerator.expect("xv6 accelerator is configured");
        let chan = self.registers[10];
        let current = self.read_u64(self.xv6_cpu_addr())?;
        for index in 0..XV6_PROC_COUNT {
            let proc = accelerator.proc_start + index * XV6_PROC_STRIDE;
            if proc != current
                && self.read_u32(proc.wrapping_add(XV6_PROC_STATE))? == XV6_PROC_SLEEPING
                && self.read_u64(proc.wrapping_add(XV6_PROC_CHAN))? == chan
            {
                self.write_u32(proc.wrapping_add(XV6_PROC_STATE), XV6_PROC_RUNNABLE)?;
            }
        }

        self.fast_return();
        Ok(true)
    }

    fn fast_return(&mut self) {
        self.registers[0] = 0;
        self.write_pc(self.registers[1]);
    }

    fn xv6_cpu_addr(&self) -> u64 {
        let accelerator = self.xv6_accelerator.expect("xv6 accelerator is configured");
        let hart = self.registers[4] as i32 as i64 as u64;
        accelerator
            .cpus
            .wrapping_add(hart.wrapping_mul(XV6_CPU_STRIDE))
    }

    fn read_u8(&mut self, addr: u64) -> Result<u8, Exception> {
        let physical = self.translate_sized(addr, MemoryAccess::Load, 1)?;
        self.bus
            .read(physical, 1)
            .map(|value| value as u8)
            .map_err(|_| Exception::LoadAccessFault(addr))
    }

    fn read_u32(&mut self, addr: u64) -> Result<u32, Exception> {
        if addr & 0x3 != 0 {
            let mut bytes = [0u8; 4];
            for (offset, byte) in bytes.iter_mut().enumerate() {
                *byte = self.read_u8(addr.wrapping_add(offset as u64))?;
            }
            return Ok(u32::from_le_bytes(bytes));
        }
        let physical = self.translate_sized(addr, MemoryAccess::Load, 4)?;
        self.bus
            .read(physical, 4)
            .map(|value| value as u32)
            .map_err(|_| Exception::LoadAccessFault(addr))
    }

    fn read_u64(&mut self, addr: u64) -> Result<u64, Exception> {
        if addr & 0x7 != 0 {
            let mut bytes = [0u8; 8];
            for (offset, byte) in bytes.iter_mut().enumerate() {
                *byte = self.read_u8(addr.wrapping_add(offset as u64))?;
            }
            return Ok(u64::from_le_bytes(bytes));
        }
        let physical = self.translate_sized(addr, MemoryAccess::Load, 8)?;
        self.bus
            .read(physical, 8)
            .map_err(|_| Exception::LoadAccessFault(addr))
    }

    fn write_u8(&mut self, addr: u64, value: u8) -> Result<(), Exception> {
        let physical = self.translate_sized(addr, MemoryAccess::Store, 1)?;
        self.clear_reservation();
        self.bus
            .write(physical, u64::from(value), 1)
            .map_err(|_| Exception::StoreAMOAccessFault(addr))
    }

    fn write_u32(&mut self, addr: u64, value: u32) -> Result<(), Exception> {
        if addr & 0x3 != 0 {
            for (offset, byte) in value.to_le_bytes().into_iter().enumerate() {
                self.write_u8(addr.wrapping_add(offset as u64), byte)?;
            }
            return Ok(());
        }
        let physical = self.translate_sized(addr, MemoryAccess::Store, 4)?;
        self.clear_reservation();
        self.bus
            .write(physical, u64::from(value), 4)
            .map_err(|_| Exception::StoreAMOAccessFault(addr))
    }

    fn write_u64(&mut self, addr: u64, value: u64) -> Result<(), Exception> {
        if addr & 0x7 != 0 {
            for (offset, byte) in value.to_le_bytes().into_iter().enumerate() {
                self.write_u8(addr.wrapping_add(offset as u64), byte)?;
            }
            return Ok(());
        }
        let physical = self.translate_sized(addr, MemoryAccess::Store, 8)?;
        self.clear_reservation();
        self.bus
            .write(physical, value, 8)
            .map_err(|_| Exception::StoreAMOAccessFault(addr))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dram::Dram;

    const BASE: u64 = crate::cfg::DRAM_BASE;

    fn accelerator() -> Xv6Accelerator {
        Xv6Accelerator {
            mycpu: BASE,
            holding: BASE + 4,
            push_off: BASE + 8,
            acquire: BASE + 12,
            pop_off: BASE + 16,
            release: BASE + 20,
            memcmp: BASE + 24,
            memmove: BASE + 28,
            strncmp: BASE + 32,
            strncpy: BASE + 36,
            strlen: BASE + 40,
            uvmunmap: BASE + 44,
            freewalk: BASE + 48,
            uvmcopy: BASE + 52,
            myproc: BASE + 56,
            wakeup: BASE + 60,
            cpus: BASE + 0x1000,
            kmem: BASE + 0x1100,
            kernel_end: BASE + 0x8000,
            dram_base: BASE,
            dram_end: BASE + 0x10000,
            phys_top: BASE + 0x10000,
            proc_start: BASE + 0x2000,
            proc_end: BASE + 0x2000 + XV6_PROC_TABLE_SIZE,
        }
    }

    fn cpu() -> (Cpu, Xv6Accelerator) {
        let config = accelerator();
        let mut cpu = Cpu::with_reset_vector(
            Box::new(Dram::with_layout(BASE, 0x10000)),
            BASE,
            BASE + 0x10000,
        );
        cpu.csr.store(csr::PMPADDR0, (1u64 << 54) - 1);
        cpu.csr.store(csr::PMPCFG0, 0x0f);
        cpu.privilege = PrivilegeMode::Supervisor;
        cpu.registers[1] = BASE + 0x100;
        cpu.set_xv6_accelerator(config).unwrap();
        (cpu, config)
    }

    #[test]
    fn oversized_memmove_executes_the_guest_instruction_without_copying() {
        let (mut cpu, config) = cpu();
        cpu.pc = config.memmove;
        cpu.bus.write(cpu.pc, 0x02a0_0293, 4).unwrap(); // addi t0, zero, 42
        let data = BASE + 0x9000;
        cpu.bus.write(data, 0x1234, 8).unwrap();
        cpu.registers[10] = data;
        cpu.registers[11] = data + 8;
        cpu.registers[12] = u64::from(u32::MAX);

        cpu.step().unwrap();

        assert_eq!(cpu.registers[5], 42);
        assert_eq!(cpu.bus.read(data, 8), Ok(0x1234));
    }

    #[test]
    fn memmove_preserves_overlapping_data_and_returns_to_the_caller() {
        let data = BASE + 0x9000;
        for (src, dst, expected) in [
            (data, data + 2, [1, 2, 1, 2, 3, 4, 5, 6]),
            (data + 2, data, [3, 4, 5, 6, 7, 8, 7, 8]),
        ] {
            let (mut cpu, config) = cpu();
            cpu.pc = config.memmove;
            cpu.bus
                .write(data, u64::from_le_bytes([1, 2, 3, 4, 5, 6, 7, 8]), 8)
                .unwrap();
            cpu.registers[10] = dst;
            cpu.registers[11] = src;
            cpu.registers[12] = 6;

            cpu.step().unwrap();

            assert_eq!(cpu.bus.read(data, 8), Ok(u64::from_le_bytes(expected)));
            assert_eq!(cpu.registers[10], dst);
            assert_eq!(cpu.pc, cpu.registers[1]);
        }
    }

    #[test]
    fn byte_store_loops_preserve_values_that_change_with_the_pointer() {
        let start = BASE + 0x200;
        let data = BASE + 0x9000;
        for (store, expected) in [
            (0x00a5_0023, [0, 1, 2, 3, 4, 5, 6, 7]), // sb a0, 0(a0)
            (0x00c5_0023, [0x5a; 8]),                // sb a2, 0(a0)
        ] {
            let (mut cpu, _) = cpu();
            cpu.bus.write(start, store, 4).unwrap();
            cpu.bus.write(start + 4, 0x0505, 2).unwrap(); // c.addi a0, 1
            cpu.bus.write(start + 6, 0xfeb5_1de3, 4).unwrap(); // bne a0, a1, -6
            cpu.pc = start;
            cpu.registers[10] = data;
            cpu.registers[11] = data + 8;
            cpu.registers[12] = 0x5a;
            for _ in 0..32 {
                cpu.step().unwrap();
                if cpu.pc == start + 10 {
                    break;
                }
            }
            assert_eq!(cpu.pc, start + 10);
            assert_eq!(cpu.bus.read(data, 8), Ok(u64::from_le_bytes(expected)));
        }
    }

    #[test]
    fn invalid_configuration_does_not_disable_the_previous_accelerator() {
        let (mut cpu, config) = cpu();
        for invalid in [
            Xv6Accelerator {
                dram_end: BASE,
                ..config
            },
            Xv6Accelerator {
                proc_end: u64::MAX,
                ..config
            },
            Xv6Accelerator {
                cpus: u64::MAX,
                ..config
            },
            Xv6Accelerator {
                memmove: BASE + 1,
                ..config
            },
        ] {
            assert!(cpu.set_xv6_accelerator(invalid).is_err());
            cpu.pc = config.mycpu;
            cpu.registers[10] = 0;
            cpu.step().unwrap();
            assert_eq!(cpu.registers[10], config.cpus);
            assert_eq!(cpu.pc, cpu.registers[1]);
        }
    }

    #[test]
    fn other_privilege_modes_and_harts_execute_guest_code() {
        for (mode, hart) in [
            (PrivilegeMode::User, 0),
            (PrivilegeMode::Machine, 0),
            (PrivilegeMode::Supervisor, 1),
        ] {
            let (mut cpu, config) = cpu();
            cpu.pc = config.mycpu;
            cpu.bus.write(cpu.pc, 0x02a0_0293, 4).unwrap(); // addi t0, zero, 42
            cpu.privilege = mode;
            cpu.registers[4] = hart;

            cpu.step().unwrap();

            assert_eq!(cpu.registers[5], 42);
        }
    }
}
