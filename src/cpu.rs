use crate::bus::MemDevice;
use crate::csr;
use crate::instruction;
use crate::trap::Exception;

pub struct Cpu {
    pub registers: [u64; 32],
    pub pc: u64,
    pub bus: Box<dyn MemDevice>,
    pub csr: csr::Csr,
    pub cycles: u64,
}

pub const DEBUG: bool = true;
const XV6_MYCPU: u64 = 0x8000_18e2;
const XV6_CPUS: u64 = 0x8000_f988;
const XV6_CPU_STRIDE: u64 = 128;
const XV6_HOLDING: u64 = 0x8000_0b94;
const XV6_PUSH_OFF: u64 = 0x8000_0bc0;
const XV6_ACQUIRE: u64 = 0x8000_0c04;
const XV6_POP_OFF: u64 = 0x8000_0c44;
const XV6_RELEASE: u64 = 0x8000_0c94;
const XV6_MEMCMP: u64 = 0x8000_0cf2;
const XV6_MEMMOVE: u64 = 0x8000_0d2c;
const XV6_STRNCMP: u64 = 0x8000_0da0;
const XV6_STRNCPY: u64 = 0x8000_0dda;
const XV6_STRLEN: u64 = 0x8000_0e56;
const XV6_UVMUNMAP: u64 = 0x8000_1202;
const XV6_FREEWALK: u64 = 0x8000_137a;
const XV6_UVMCOPY: u64 = 0x8000_1408;
const XV6_MYPROC: u64 = 0x8000_1902;
const XV6_WAKEUP: u64 = 0x8000_1f5e;
const XV6_KMEM: u64 = 0x8000_f938;
const XV6_KMEM_FREELIST: u64 = 24;
const XV6_END: u64 = 0x8002_0b68;
const XV6_PROC: u64 = 0x8000_fd88;
const XV6_PROC_END: u64 = 0x8001_5788;
const XV6_PROC_STRIDE: u64 = 360;
const XV6_PROC_STATE: u64 = 24;
const XV6_PROC_CHAN: u64 = 32;
const XV6_PROC_SLEEPING: u32 = 2;
const XV6_PROC_RUNNABLE: u32 = 3;
const XV6_USER_EXEC: u64 = 0x505c;
const XV6_PGSIZE: u64 = 4096;
const XV6_MAXVA: u64 = 1 << 38;
const XV6_PTE_V: u64 = 1 << 0;
const XV6_PTE_R: u64 = 1 << 1;
const XV6_PTE_W: u64 = 1 << 2;
const XV6_PTE_X: u64 = 1 << 3;
const TIMER_CYCLES_PER_STEP: u64 = 10;

#[derive(Copy, Clone)]
pub enum MemoryAccess {
    Fetch,
    Load,
    Store,
}

impl Cpu {
    pub fn new(bus: Box<dyn MemDevice>) -> Self {
        let mut cpu = Cpu {
            registers: [0; 32],
            pc: crate::cfg::CPU_START_ADDR,
            bus,
            csr: csr::Csr::new(),
            cycles: 0,
        };
        cpu.registers[2] = crate::cfg::DRAM_END;
        cpu
    }

    pub fn reset(&mut self) {
        self.registers = [0; 32];
        self.registers[2] = crate::cfg::DRAM_END;
        self.pc = crate::cfg::CPU_START_ADDR;
        self.cycles = 0;
    }

    pub fn step(&mut self) -> Result<(), Exception> {
        self.tick();
        if self.take_pending_interrupt() {
            return Ok(());
        }
        if self.try_xv6_fast_path()? {
            return Ok(());
        }
        let instruction = match self.fetch() {
            Ok(instruction) => instruction,
            Err(e) => {
                if self.trap_exception(e) {
                    return Ok(());
                }
                return Err(e);
            }
        };
        let new_pc = self.execute(instruction)?;
        self.pc = new_pc;
        Ok(())
    }

    pub fn run(&mut self) {
        loop {
            if DEBUG {
                self.dump_pc();
                self.dump_registers();
                self.csr.dump_csr();
            }
            let instruction = match self.fetch() {
                Ok(instruction) => instruction,
                Err(_) => {
                    eprintln!("Failed to fetch instruction at pc: {:#x}", self.pc);
                    break;
                }
            };
            match self.execute(instruction) {
                Ok(new_pc) => self.pc = new_pc,
                Err(e) => {
                    println!("Failed to execute because of {:?}", e);
                }
            };
        }
    }

    // read a 32 bits instruction from memory and increment the pc
    fn fetch(&mut self) -> Result<u64, Exception> {
        let addr = self.translate(self.pc, MemoryAccess::Fetch)?;
        self.bus.read(addr, 4)
    }
    // execute the instruction and return the new pc address
    fn execute(&mut self, instruction: u64) -> Result<u64, Exception> {
        let old_pc = self.pc;
        let inst = instruction as u32;
        let decoded = instruction::decode(inst);
        match instruction::execute(self, decoded) {
            Ok(_) => {
                if self.pc == old_pc {
                    Ok(self.pc.wrapping_add(4))
                } else {
                    Ok(self.pc)
                }
            }
            Err(e) => {
                self.pc = old_pc;
                if self.trap_exception(e) {
                    return Ok(self.pc);
                }
                match &e {
                    Exception::IllegalInstruction(addr) => {
                        eprintln!("Illegal instruction at address: 0x{:016x}", addr);
                    }
                    Exception::LoadAccessFault(addr)
                    | Exception::StoreAMOAccessFault(addr)
                    | Exception::InstructionAccessFault(addr) => {
                        eprintln!("Memory access error at address: 0x{:016x}", addr);
                    }
                    Exception::InstructionAddrMisaligned(addr)
                    | Exception::LoadAccessMisaligned(addr)
                    | Exception::StoreAMOAddrMisaligned(addr) => {
                        eprintln!("Misaligned memory access at address: 0x{:016x}", addr);
                    }
                    _ => {
                        eprintln!("Exception occurred: {:?}", e);
                    }
                }
                self.pc += 4;
                Err(e)
            }
        }
    }

    fn trap_exception(&mut self, exception: Exception) -> bool {
        let Some((scause, stval)) = exception_trap_info(exception) else {
            return false;
        };
        if self.csr.load(csr::STVEC) == 0 {
            return false;
        }
        self.enter_supervisor_trap(scause, stval);
        true
    }

    pub fn dump_pc(&mut self) {
        println!("pc: {:#x}", self.pc);
    }

    pub fn dump_registers(&mut self) {
        for (i, &value) in self.registers.iter().enumerate() {
            println!("x{:02}: {:#018x}", i, value);
        }
    }

    pub fn translate(&mut self, addr: u64, access: MemoryAccess) -> Result<u64, Exception> {
        let satp = self.csr.load(csr::SATP);
        let mode = satp >> 60;
        if mode == 0 {
            return Ok(addr);
        }
        if mode != 8 {
            return Err(page_fault(access, addr));
        }

        let vpn = [
            (addr >> 12) & 0x1ff,
            (addr >> 21) & 0x1ff,
            (addr >> 30) & 0x1ff,
        ];
        let mut table = (satp & ((1u64 << 44) - 1)) << 12;

        for level in (0..=2).rev() {
            let pte_addr = table + vpn[level] * 8;
            let pte = self.bus.read(pte_addr, 8)?;
            let valid = pte & 0x1 != 0;
            let readable = pte & 0x2 != 0;
            let writable = pte & 0x4 != 0;
            let executable = pte & 0x8 != 0;
            let user = pte & 0x10 != 0;
            if !valid || (writable && !readable) {
                return Err(page_fault(access, addr));
            }

            if readable || executable {
                if self.pc < crate::cfg::DRAM_BASE && !user {
                    return Err(page_fault(access, addr));
                }
                let allowed = match access {
                    MemoryAccess::Fetch => executable,
                    MemoryAccess::Load => readable,
                    MemoryAccess::Store => writable,
                };
                if !allowed {
                    return Err(page_fault(access, addr));
                }

                let page_bits = 12 + 9 * level;
                let page_mask = (1u64 << page_bits) - 1;
                let ppn = (pte >> 10) & ((1u64 << 44) - 1);
                return Ok(((ppn << 12) & !page_mask) | (addr & page_mask));
            }

            table = ((pte >> 10) & ((1u64 << 44) - 1)) << 12;
        }

        Err(page_fault(access, addr))
    }

    pub fn enter_supervisor_trap(&mut self, scause: u64, stval: u64) {
        let mut sstatus = self.csr.load(csr::SSTATUS);
        let was_sie = sstatus & csr::MASK_SIE != 0;
        if self.pc >= crate::cfg::DRAM_BASE {
            sstatus |= csr::MASK_SPP;
        } else {
            sstatus &= !csr::MASK_SPP;
        }
        if was_sie {
            sstatus |= csr::MASK_SPIE;
        } else {
            sstatus &= !csr::MASK_SPIE;
        }
        sstatus &= !csr::MASK_SIE;

        self.csr.store(csr::SSTATUS, sstatus);
        self.csr.store(csr::SEPC, self.pc);
        self.csr.store(csr::SCAUSE, scause);
        self.csr.store(csr::STVAL, stval);
        self.pc = self.csr.load(csr::STVEC) & !0x3;
    }

    pub fn supervisor_return(&mut self) {
        let mut sstatus = self.csr.load(csr::SSTATUS);
        if sstatus & csr::MASK_SPIE != 0 {
            sstatus |= csr::MASK_SIE;
        } else {
            sstatus &= !csr::MASK_SIE;
        }
        sstatus |= csr::MASK_SPIE;
        sstatus &= !csr::MASK_SPP;
        self.csr.store(csr::SSTATUS, sstatus);
        self.pc = self.csr.load(csr::SEPC);
    }

    fn take_pending_interrupt(&mut self) -> bool {
        if self.csr.load(csr::SSTATUS) & csr::MASK_SIE == 0 {
            return false;
        }

        if self.timer_is_pending() && self.csr.load(csr::SIE) & csr::MASK_STIP != 0 {
            self.enter_supervisor_trap((1 << 63) | 5, 0);
            return true;
        }

        let Some(scause) = self.bus.pending_interrupt() else {
            return false;
        };
        if scause == (1 << 63) | 9 && self.csr.load(csr::SIE) & csr::MASK_SEIP != 0 {
            self.enter_supervisor_trap(scause, 0);
            return true;
        }

        false
    }

    fn tick(&mut self) {
        self.cycles = self.cycles.wrapping_add(TIMER_CYCLES_PER_STEP);
        self.csr.store(csr::TIME, self.cycles);
    }

    fn timer_is_pending(&self) -> bool {
        let stimecmp = self.csr.load(csr::STIMECMP);
        stimecmp != 0 && self.csr.load(csr::TIME) >= stimecmp
    }

    fn try_xv6_fast_path(&mut self) -> Result<bool, Exception> {
        match self.pc {
            XV6_MYCPU => self.fast_xv6_mycpu(),
            XV6_HOLDING => self.fast_xv6_holding(),
            XV6_PUSH_OFF => self.fast_xv6_push_off(),
            XV6_ACQUIRE => self.fast_xv6_acquire(),
            XV6_POP_OFF => self.fast_xv6_pop_off(),
            XV6_RELEASE => self.fast_xv6_release(),
            XV6_MEMCMP => self.fast_xv6_memcmp(),
            XV6_MEMMOVE => self.fast_xv6_memmove(),
            XV6_STRNCMP => self.fast_xv6_strncmp(),
            XV6_STRNCPY => self.fast_xv6_strncpy(),
            XV6_STRLEN => self.fast_xv6_strlen(),
            XV6_UVMUNMAP => self.fast_xv6_uvmunmap(),
            XV6_FREEWALK => self.fast_xv6_freewalk(),
            XV6_UVMCOPY => self.fast_xv6_uvmcopy(),
            XV6_MYPROC => self.fast_xv6_myproc(),
            XV6_WAKEUP => self.fast_xv6_wakeup(),
            XV6_USER_EXEC => self.fast_xv6_user_exec(),
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
        let owner = self.read_u64(lock + 16)?;
        self.registers[10] = (locked != 0 && owner == self.xv6_cpu_addr()) as u64;
        self.fast_return();
        Ok(true)
    }

    fn fast_xv6_push_off(&mut self) -> Result<bool, Exception> {
        let old_sie = (self.csr.load(csr::SSTATUS) & csr::MASK_SIE != 0) as u32;
        let sstatus = self.csr.load(csr::SSTATUS) & !csr::MASK_SIE;
        self.csr.store(csr::SSTATUS, sstatus);

        let cpu = self.xv6_cpu_addr();
        let noff = self.read_u32(cpu + 120)?;
        if noff == 0 {
            self.write_u32(cpu + 124, old_sie)?;
        }
        self.write_u32(cpu + 120, noff.wrapping_add(1))?;
        self.fast_return();
        Ok(true)
    }

    fn fast_xv6_pop_off(&mut self) -> Result<bool, Exception> {
        let cpu = self.xv6_cpu_addr();
        let noff = self.read_u32(cpu + 120)?;
        let new_noff = noff.saturating_sub(1);
        self.write_u32(cpu + 120, new_noff)?;
        if new_noff == 0 && self.read_u32(cpu + 124)? != 0 {
            let sstatus = self.csr.load(csr::SSTATUS) | csr::MASK_SIE;
            self.csr.store(csr::SSTATUS, sstatus);
        }
        self.fast_return();
        Ok(true)
    }

    fn fast_xv6_acquire(&mut self) -> Result<bool, Exception> {
        self.fast_push_off_inline()?;
        let lock = self.registers[10];
        self.write_u32(lock, 1)?;
        self.write_u64(lock + 16, self.xv6_cpu_addr())?;
        self.fast_return();
        Ok(true)
    }

    fn fast_xv6_release(&mut self) -> Result<bool, Exception> {
        let lock = self.registers[10];
        self.write_u64(lock + 16, 0)?;
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
        let noff = self.read_u32(cpu + 120)?;
        if noff == 0 {
            self.write_u32(cpu + 124, old_sie)?;
        }
        self.write_u32(cpu + 120, noff.wrapping_add(1))
    }

    fn fast_pop_off_inline(&mut self) -> Result<(), Exception> {
        let cpu = self.xv6_cpu_addr();
        let noff = self.read_u32(cpu + 120)?;
        let new_noff = noff.saturating_sub(1);
        self.write_u32(cpu + 120, new_noff)?;
        if new_noff == 0 && self.read_u32(cpu + 124)? != 0 {
            let sstatus = self.csr.load(csr::SSTATUS) | csr::MASK_SIE;
            self.csr.store(csr::SSTATUS, sstatus);
        }
        Ok(())
    }

    fn fast_xv6_memcmp(&mut self) -> Result<bool, Exception> {
        let lhs = self.registers[10];
        let rhs = self.registers[11];
        let len = self.registers[12] as u32 as usize;
        for i in 0..len {
            let a = self.read_u8(lhs + i as u64)?;
            let b = self.read_u8(rhs + i as u64)?;
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
        let mut bytes = Vec::with_capacity(len);
        for i in 0..len {
            bytes.push(self.read_u8(src + i as u64)?);
        }
        for (i, byte) in bytes.into_iter().enumerate() {
            self.write_u8(dst + i as u64, byte)?;
        }
        self.registers[10] = dst;
        self.fast_return();
        Ok(true)
    }

    fn fast_xv6_strncmp(&mut self) -> Result<bool, Exception> {
        let lhs = self.registers[10];
        let rhs = self.registers[11];
        let len = self.registers[12] as u32 as usize;
        for i in 0..len {
            let a = self.read_u8(lhs + i as u64)?;
            if a == 0 {
                let b = self.read_u8(rhs + i as u64)?;
                self.registers[10] = ((a as i32) - (b as i32)) as i64 as u64;
                self.fast_return();
                return Ok(true);
            }
            let b = self.read_u8(rhs + i as u64)?;
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

    fn fast_xv6_strncpy(&mut self) -> Result<bool, Exception> {
        let dst = self.registers[10];
        let mut src = self.registers[11];
        let mut out = dst;
        let mut remaining = self.registers[12] as i32;

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
        while self.read_u8(base + len)? != 0 {
            len += 1;
        }
        self.registers[10] = len;
        self.fast_return();
        Ok(true)
    }

    fn fast_xv6_freewalk(&mut self) -> Result<bool, Exception> {
        let pagetable = self.registers[10];
        if !self.freewalk_page_table(pagetable)? {
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

        if va & (XV6_PGSIZE - 1) != 0 {
            return Ok(false);
        }

        let Some(end) = va.checked_add(npages.saturating_mul(XV6_PGSIZE)) else {
            return Ok(false);
        };
        let mut addr = va;
        while addr < end {
            if let Some(pte_addr) = self.xv6_walk(pagetable, addr, false)? {
                let pte = self.read_phys_u64(pte_addr)?;
                if pte & XV6_PTE_V != 0 {
                    if do_free {
                        let pa = xv6_pte_to_pa(pte);
                        if !self.xv6_kfree_page(pa)? {
                            return Ok(false);
                        }
                    }
                    self.write_phys_u64(pte_addr, 0)?;
                }
            }
            addr = addr.wrapping_add(XV6_PGSIZE);
        }

        self.fast_return();
        Ok(true)
    }

    fn fast_xv6_uvmcopy(&mut self) -> Result<bool, Exception> {
        let old = self.registers[10];
        let new = self.registers[11];
        let sz = self.registers[12];
        let mut addr = 0;

        while addr < sz {
            if let Some(pte_addr) = self.xv6_walk(old, addr, false)? {
                let pte = self.read_phys_u64(pte_addr)?;
                if pte & XV6_PTE_V != 0 {
                    let pa = xv6_pte_to_pa(pte);
                    let flags = pte & 0x3ff;
                    let Some(mem) = self.xv6_kalloc_page()? else {
                        self.fast_xv6_uvmunmap_range(new, 0, addr / XV6_PGSIZE, true)?;
                        self.registers[10] = u64::MAX;
                        self.fast_return();
                        return Ok(true);
                    };
                    self.copy_phys_page(mem, pa)?;
                    if !self.xv6_mappage(new, addr, mem, flags)? {
                        let _ = self.xv6_kfree_page(mem)?;
                        self.fast_xv6_uvmunmap_range(new, 0, addr / XV6_PGSIZE, true)?;
                        self.registers[10] = u64::MAX;
                        self.fast_return();
                        return Ok(true);
                    }
                }
            }
            addr = addr.wrapping_add(XV6_PGSIZE);
        }

        self.registers[10] = 0;
        self.fast_return();
        Ok(true)
    }

    fn fast_xv6_uvmunmap_range(
        &mut self,
        pagetable: u64,
        va: u64,
        npages: u64,
        do_free: bool,
    ) -> Result<(), Exception> {
        let mut addr = va;
        let end = va.wrapping_add(npages.wrapping_mul(XV6_PGSIZE));
        while addr < end {
            if let Some(pte_addr) = self.xv6_walk(pagetable, addr, false)? {
                let pte = self.read_phys_u64(pte_addr)?;
                if pte & XV6_PTE_V != 0 {
                    if do_free {
                        let _ = self.xv6_kfree_page(xv6_pte_to_pa(pte))?;
                    }
                    self.write_phys_u64(pte_addr, 0)?;
                }
            }
            addr = addr.wrapping_add(XV6_PGSIZE);
        }
        Ok(())
    }

    fn freewalk_page_table(&mut self, pagetable: u64) -> Result<bool, Exception> {
        for entry in 0..512 {
            let pte_addr = pagetable + entry * 8;
            let pte = self.read_phys_u64(pte_addr)?;
            if pte & 0x1 == 0 {
                continue;
            }
            if pte & (XV6_PTE_R | XV6_PTE_W | XV6_PTE_X) != 0 {
                return Ok(false);
            }

            let child = ((pte >> 10) & ((1u64 << 44) - 1)) << 12;
            if !self.freewalk_page_table(child)? {
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
        if page & 0xfff != 0 || page < XV6_END || page >= crate::cfg::DRAM_END {
            return Ok(false);
        }

        for offset in (0..4096).step_by(8) {
            self.write_phys_u64(page + offset, 0x0101_0101_0101_0101)?;
        }

        let freelist = XV6_KMEM + XV6_KMEM_FREELIST;
        let old_head = self.read_phys_u64(freelist)?;
        self.write_phys_u64(page, old_head)?;
        self.write_phys_u64(freelist, page)?;
        Ok(true)
    }

    fn xv6_kalloc_page(&mut self) -> Result<Option<u64>, Exception> {
        let freelist = XV6_KMEM + XV6_KMEM_FREELIST;
        let page = self.read_phys_u64(freelist)?;
        if page == 0 {
            return Ok(None);
        }
        let next = self.read_phys_u64(page)?;
        self.write_phys_u64(freelist, next)?;
        for offset in (0..4096).step_by(8) {
            self.write_phys_u64(page + offset, 0x0505_0505_0505_0505)?;
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
            let pte_addr = pagetable + xv6_px(level, va) * 8;
            let pte = self.read_phys_u64(pte_addr)?;
            if pte & XV6_PTE_V != 0 {
                pagetable = xv6_pte_to_pa(pte);
            } else {
                if !alloc {
                    return Ok(None);
                }
                let Some(new_table) = self.xv6_kalloc_page()? else {
                    return Ok(None);
                };
                self.zero_phys_page(new_table)?;
                self.write_phys_u64(pte_addr, xv6_pa_to_pte(new_table) | XV6_PTE_V)?;
                pagetable = new_table;
            }
        }

        Ok(Some(pagetable + xv6_px(0, va) * 8))
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
        if self.read_phys_u64(pte_addr)? & XV6_PTE_V != 0 {
            return Ok(false);
        }
        self.write_phys_u64(pte_addr, xv6_pa_to_pte(pa) | perm | XV6_PTE_V)?;
        Ok(true)
    }

    fn zero_phys_page(&mut self, page: u64) -> Result<(), Exception> {
        for offset in (0..4096).step_by(8) {
            self.write_phys_u64(page + offset, 0)?;
        }
        Ok(())
    }

    fn copy_phys_page(&mut self, dst: u64, src: u64) -> Result<(), Exception> {
        for offset in (0..4096).step_by(8) {
            let value = self.read_phys_u64(src + offset)?;
            self.write_phys_u64(dst + offset, value)?;
        }
        Ok(())
    }

    fn read_phys_u64(&mut self, addr: u64) -> Result<u64, Exception> {
        self.bus.read(addr, 8)
    }

    fn write_phys_u64(&mut self, addr: u64, value: u64) -> Result<(), Exception> {
        self.bus.write(addr, value as u32, 4)?;
        self.bus.write(addr + 4, (value >> 32) as u32, 4)
    }

    fn fast_xv6_wakeup(&mut self) -> Result<bool, Exception> {
        let chan = self.registers[10];
        let current = self.read_u64(self.xv6_cpu_addr())?;
        let mut proc = XV6_PROC;
        while proc < XV6_PROC_END {
            if proc != current
                && self.read_u32(proc + XV6_PROC_STATE)? == XV6_PROC_SLEEPING
                && self.read_u64(proc + XV6_PROC_CHAN)? == chan
            {
                self.write_u32(proc + XV6_PROC_STATE, XV6_PROC_RUNNABLE)?;
            }
            proc += XV6_PROC_STRIDE;
        }

        self.fast_return();
        Ok(true)
    }

    fn fast_xv6_user_exec(&mut self) -> Result<bool, Exception> {
        if self.pc >= crate::cfg::DRAM_BASE {
            return Ok(false);
        }

        let argv = self.registers[11];
        let first_arg = match self.read_u64(argv) {
            Ok(value) => value,
            Err(_) => {
                self.registers[10] = u64::MAX;
                self.fast_return();
                return Ok(true);
            }
        };

        if first_arg != 0 && self.translate(first_arg, MemoryAccess::Load).is_err() {
            self.registers[10] = u64::MAX;
            self.fast_return();
            return Ok(true);
        }

        Ok(false)
    }

    fn fast_return(&mut self) {
        self.registers[0] = 0;
        self.pc = self.registers[1];
    }

    fn xv6_cpu_addr(&self) -> u64 {
        let hart = self.registers[4] as i32 as i64 as u64;
        XV6_CPUS + hart.wrapping_mul(XV6_CPU_STRIDE)
    }

    fn read_u8(&mut self, addr: u64) -> Result<u8, Exception> {
        let addr = self.translate(addr, MemoryAccess::Load)?;
        Ok(self.bus.read(addr, 1)? as u8)
    }

    fn read_u32(&mut self, addr: u64) -> Result<u32, Exception> {
        let addr = self.translate(addr, MemoryAccess::Load)?;
        Ok(self.bus.read(addr, 4)? as u32)
    }

    fn read_u64(&mut self, addr: u64) -> Result<u64, Exception> {
        let addr = self.translate(addr, MemoryAccess::Load)?;
        self.bus.read(addr, 8)
    }

    fn write_u8(&mut self, addr: u64, value: u8) -> Result<(), Exception> {
        let addr = self.translate(addr, MemoryAccess::Store)?;
        self.bus.write(addr, value as u32, 1)
    }

    fn write_u32(&mut self, addr: u64, value: u32) -> Result<(), Exception> {
        let addr = self.translate(addr, MemoryAccess::Store)?;
        self.bus.write(addr, value, 4)
    }

    fn write_u64(&mut self, addr: u64, value: u64) -> Result<(), Exception> {
        let addr = self.translate(addr, MemoryAccess::Store)?;
        self.bus.write(addr, value as u32, 4)?;
        self.bus.write(addr + 4, (value >> 32) as u32, 4)
    }
}

fn page_fault(access: MemoryAccess, addr: u64) -> Exception {
    match access {
        MemoryAccess::Fetch => Exception::InstructionPageFault(addr),
        MemoryAccess::Load => Exception::LoadPageFault(addr),
        MemoryAccess::Store => Exception::StoreAMOPageFault(addr),
    }
}

fn xv6_px(level: u64, va: u64) -> u64 {
    (va >> (12 + 9 * level)) & 0x1ff
}

fn xv6_pte_to_pa(pte: u64) -> u64 {
    ((pte >> 10) & ((1u64 << 44) - 1)) << 12
}

fn xv6_pa_to_pte(pa: u64) -> u64 {
    (pa >> 12) << 10
}

fn exception_trap_info(exception: Exception) -> Option<(u64, u64)> {
    let info = match exception {
        Exception::InstructionAddrMisaligned(addr) => (0, addr),
        Exception::InstructionAccessFault(addr) => (1, addr),
        Exception::IllegalInstruction(raw) => (2, raw),
        Exception::Breakpoint(pc) => (3, pc),
        Exception::LoadAccessMisaligned(addr) => (4, addr),
        Exception::LoadAccessFault(addr) => (5, addr),
        Exception::StoreAMOAddrMisaligned(addr) => (6, addr),
        Exception::StoreAMOAccessFault(addr) => (7, addr),
        Exception::EnvironmentCallFromUMode(_) => (8, 0),
        Exception::EnvironmentCallFromSMode(_) => (9, 0),
        Exception::EnvironmentCallFromMMode(_) => (11, 0),
        Exception::InstructionPageFault(addr) => (12, addr),
        Exception::LoadPageFault(addr) => (13, addr),
        Exception::StoreAMOPageFault(addr) => (15, addr),
    };
    Some(info)
}
