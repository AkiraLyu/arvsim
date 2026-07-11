//! 单 hart RV64 CPU 状态和执行循环。
//!
//! [`Cpu::step`] 依次推进时间、处理中断、尝试可选 xv6 加速、取指、译码执行并提交 PC。
//! 地址翻译和监督模式 trap 也集中在本模块；具体物理内存和设备通过 [`MemDevice`] 注入。
//! 当前实现没有独立的特权级字段，部分监督/用户判断沿用现有 PC 地址范围约定。

use crate::bus::MemDevice;
use crate::csr;
use crate::instruction;
use crate::trap::Exception;

/// 一个 CPU hart 的可观察状态及其总线连接。
pub struct Cpu {
    /// 32 个整数寄存器；指令完成后必须保持 `registers[0] == 0`。
    pub registers: [u64; 32],
    /// 下一条待执行指令的 guest PC。
    pub pc: u64,
    /// 同时承载物理内存、MMIO 和外部中断查询的设备接口。
    pub bus: Box<dyn MemDevice>,
    /// 当前 hart 的控制与状态寄存器文件。
    pub csr: csr::Csr,
    /// 模拟周期计数；每次 `step` 按固定粒度增加。
    pub cycles: u64,
    reset_vector: u64,
    initial_sp: u64,
    xv6_accelerator: Option<Xv6Accelerator>,
}

/// 执行循环输出的调试信息级别。
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
pub enum DebugLevel {
    #[default]
    Off,
    Pc,
    Full,
}

/// [`Cpu::run`] 的停止条件和调试配置。
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
pub struct RunOptions {
    /// 最多成功执行的步数；`None` 表示不设上限。
    pub max_steps: Option<u64>,
    /// 每步执行前输出的状态详细程度。
    pub debug: DebugLevel,
}

/// [`Cpu::run`] 离开执行循环的原因。
#[derive(Debug, Copy, Clone)]
pub enum RunOutcome {
    /// 已成功执行指定步数，CPU 状态停在下一条指令之前。
    StepLimitReached { steps: u64 },
    /// 遇到未被 guest trap 入口接管的异常。
    Exception {
        /// 异常前已成功完成的步数。
        steps: u64,
        /// 产生异常的指令 PC。
        pc: u64,
        exception: Exception,
    },
}

/// xv6 专用快速路径所需的函数入口和全局对象地址。
///
/// 该配置默认关闭，必须与实际加载的 xv6 ELF 符号匹配；错误地址可能在普通指令中间误触发快速路径。
/// 结构体偏移和数组步长仍由本模块中的兼容性常量约束。
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
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
    pub proc_start: u64,
    pub proc_end: u64,
    pub user_exec: Option<u64>,
}

const XV6_CPU_STRIDE: u64 = 128;
const XV6_KMEM_FREELIST: u64 = 24;
const XV6_PROC_STRIDE: u64 = 360;
const XV6_PROC_STATE: u64 = 24;
const XV6_PROC_CHAN: u64 = 32;
const XV6_PROC_SLEEPING: u32 = 2;
const XV6_PROC_RUNNABLE: u32 = 3;
const XV6_PGSIZE: u64 = 4096;
const XV6_MAXVA: u64 = 1 << 38;
const XV6_PTE_V: u64 = 1 << 0;
const XV6_PTE_R: u64 = 1 << 1;
const XV6_PTE_W: u64 = 1 << 2;
const XV6_PTE_X: u64 = 1 << 3;
const TIMER_CYCLES_PER_STEP: u64 = 10;

/// 一次虚拟地址访问的用途，用于选择页权限和页错误类型。
#[derive(Copy, Clone)]
pub enum MemoryAccess {
    Fetch,
    Load,
    Store,
}

impl Cpu {
    /// 使用默认复位向量和 DRAM 末端栈指针创建 CPU。
    pub fn new(bus: Box<dyn MemDevice>) -> Self {
        Self::with_reset_vector(bus, crate::cfg::CPU_START_ADDR, crate::cfg::DRAM_END)
    }

    /// 使用调用方给定的复位向量和初始栈指针创建 CPU。
    pub fn with_reset_vector(bus: Box<dyn MemDevice>, reset_vector: u64, initial_sp: u64) -> Self {
        let mut cpu = Cpu {
            registers: [0; 32],
            pc: reset_vector,
            bus,
            csr: csr::Csr::new(),
            cycles: 0,
            reset_vector,
            initial_sp,
            xv6_accelerator: None,
        };
        cpu.registers[2] = initial_sp;
        cpu
    }

    /// 恢复构造时的寄存器、PC、CSR 和周期状态，同时保留总线及可选加速器配置。
    pub fn reset(&mut self) {
        self.registers = [0; 32];
        self.registers[2] = self.initial_sp;
        self.pc = self.reset_vector;
        self.csr = csr::Csr::new();
        self.cycles = 0;
    }

    /// 启用与当前 xv6 镜像匹配的快速路径。
    pub fn set_xv6_accelerator(&mut self, accelerator: Xv6Accelerator) {
        self.xv6_accelerator = Some(accelerator);
    }

    /// 关闭 xv6 快速路径，使所有地址都按普通指令执行。
    pub fn clear_xv6_accelerator(&mut self) {
        self.xv6_accelerator = None;
    }

    /// 推进一个 CPU 步骤。
    ///
    /// 中断或快速路径被接管时也算一个成功步骤；未被 guest trap 接管的异常才返回 `Err`。
    pub fn step(&mut self) -> Result<(), Exception> {
        // 先推进时间，使本步开始时即可观察到刚到期的定时器中断。
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
                // 取指失败与执行异常走同一 trap 入口；没有 STVEC 时再上报宿主。
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

    /// 重复调用 [`Cpu::step`]，直到步数耗尽或出现未处理异常。
    pub fn run(&mut self, options: RunOptions) -> RunOutcome {
        let mut steps = 0;
        loop {
            if options.max_steps.is_some_and(|limit| steps >= limit) {
                return RunOutcome::StepLimitReached { steps };
            }

            match options.debug {
                DebugLevel::Off => {}
                DebugLevel::Pc => self.dump_pc(),
                DebugLevel::Full => {
                    self.dump_pc();
                    self.dump_registers();
                    self.csr.dump_csr();
                }
            }

            let pc = self.pc;
            if let Err(exception) = self.step() {
                return RunOutcome::Exception {
                    steps,
                    pc,
                    exception,
                };
            }
            steps = steps.wrapping_add(1);
        }
    }

    /// 翻译当前 PC，并从物理总线读取一个 32 位取指窗口。
    ///
    /// 压缩指令只使用低 16 位；统一读取 4 字节可让译码入口保持单一格式。
    fn fetch(&mut self) -> Result<u64, Exception> {
        let addr = self.translate(self.pc, MemoryAccess::Fetch)?;
        self.bus.read(addr, 4)
    }
    /// 译码并执行一条指令，返回提交后的 PC。
    fn execute(&mut self, instruction: u64) -> Result<u64, Exception> {
        let old_pc = self.pc;
        let inst = instruction as u32;
        let decoded = instruction::decode(inst);
        match instruction::execute(self, decoded) {
            Ok(_) => {
                // 当前以“PC 是否变化”推断执行器有没有提交控制流或压缩指令。
                // 因此合法的零偏移 branch/jump 会被误判为普通 32 位指令并额外前进 4；
                // 后续应让执行器返回结构化 next-PC，而不是继续扩展这一启发式规则。
                if self.pc == old_pc {
                    Ok(self.pc.wrapping_add(4))
                } else {
                    Ok(self.pc)
                }
            }
            Err(e) => {
                // 异常必须以故障指令 PC 作为 SEPC，因此先撤销执行器可能留下的 PC 变化。
                self.pc = old_pc;
                if self.trap_exception(e) {
                    return Ok(self.pc);
                }
                Err(e)
            }
        }
    }

    fn trap_exception(&mut self, exception: Exception) -> bool {
        let Some((scause, stval)) = exception_trap_info(exception) else {
            return false;
        };
        // STVEC 为零表示 guest 尚未安装监督模式入口，此时把异常交还调用方。
        if self.csr.load(csr::STVEC) == 0 {
            return false;
        }
        self.enter_supervisor_trap(scause, stval);
        true
    }

    /// 输出当前 PC。
    pub fn dump_pc(&mut self) {
        println!("pc: {:#x}", self.pc);
    }

    /// 输出全部整数寄存器。
    pub fn dump_registers(&mut self) {
        for (i, &value) in self.registers.iter().enumerate() {
            println!("x{:02}: {:#018x}", i, value);
        }
    }

    /// 根据当前 `SATP` 把虚拟地址翻译为物理地址，并检查访问类型对应的页权限。
    ///
    /// `satp.mode=0` 直接返回原地址，mode 8 使用三级 Sv39 页表；其他模式在当前模型中返回页错误。
    pub fn translate(&mut self, addr: u64, access: MemoryAccess) -> Result<u64, Exception> {
        let satp = self.csr.load(csr::SATP);
        let mode = satp >> 60;
        if mode == 0 {
            return Ok(addr);
        }
        if mode != 8 {
            return Err(page_fault(access, addr));
        }

        // Sv39 每级索引 9 位，最低 12 位保留为页内偏移。
        let vpn = [
            (addr >> 12) & 0x1ff,
            (addr >> 21) & 0x1ff,
            (addr >> 30) & 0x1ff,
        ];
        // SATP 的低 44 位是根页表物理页号，恢复物理地址时补回 12 个零位。
        let mut table = (satp & ((1u64 << 44) - 1)) << 12;

        for level in (0..=2).rev() {
            let pte_addr = table + vpn[level] * 8;
            let pte = self.bus.read(pte_addr, 8)?;
            let valid = pte & 0x1 != 0;
            let readable = pte & 0x2 != 0;
            let writable = pte & 0x4 != 0;
            let executable = pte & 0x8 != 0;
            let user = pte & 0x10 != 0;
            // RISC-V 将 W=1、R=0 视为保留的非法叶子组合。
            if !valid || (writable && !readable) {
                return Err(page_fault(access, addr));
            }

            if readable || executable {
                // 当前模型用 PC 是否落在 DRAM 以下近似用户态；用户访问不能落到 U=0 的页。
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
                // 叶子可出现在任意层；低位来自虚拟地址，因而同时覆盖普通页和大页。
                return Ok(((ppn << 12) & !page_mask) | (addr & page_mask));
            }

            table = ((pte >> 10) & ((1u64 << 44) - 1)) << 12;
        }

        Err(page_fault(access, addr))
    }

    /// 保存监督模式 trap 状态并跳转到 `STVEC` 的直接入口。
    pub fn enter_supervisor_trap(&mut self, scause: u64, stval: u64) {
        let mut sstatus = self.csr.load(csr::SSTATUS);
        let was_sie = sstatus & csr::MASK_SIE != 0;
        if self.pc >= crate::cfg::DRAM_BASE {
            sstatus |= csr::MASK_SPP;
        } else {
            sstatus &= !csr::MASK_SPP;
        }
        // SIE 被压入 SPIE，随后关闭全局监督模式中断，供 sret 对称恢复。
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
        // 当前实现只进入直接基址，清除 STVEC 低两位的模式编码。
        self.pc = self.csr.load(csr::STVEC) & !0x3;
    }

    /// 按当前简化的 `sret` 规则恢复中断状态并返回 `SEPC`。
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
        // 全局 SIE 关闭时，单独的定时器/外部中断使能位不能触发 trap。
        if self.csr.load(csr::SSTATUS) & csr::MASK_SIE == 0 {
            return false;
        }

        // 先检查定时器，固定当前单 hart 模型中多个中断同时到达时的优先顺序。
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
        let Some(accelerator) = self.xv6_accelerator else {
            return Ok(false);
        };
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
            pc if accelerator.user_exec == Some(pc) => self.fast_xv6_user_exec(),
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
        // 只在最外层关中断时保存原 SIE；嵌套层退出不能覆盖最初状态。
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
        // 先完整读取再写回，保证源、目标区间重叠时仍符合 memmove 语义。
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

        // xv6 页表操作要求起始虚拟地址页对齐；不满足时退回真实 guest 实现处理。
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
            // freewalk 只释放中间页表；遇到叶子映射说明调用前置条件不成立。
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
        let accelerator = self.xv6_accelerator.expect("xv6 accelerator is configured");
        if page & 0xfff != 0 || !(accelerator.kernel_end..crate::cfg::DRAM_END).contains(&page) {
            return Ok(false);
        }

        for offset in (0..4096).step_by(8) {
            self.write_phys_u64(page + offset, 0x0101_0101_0101_0101)?;
        }

        let freelist = accelerator.kmem + XV6_KMEM_FREELIST;
        let old_head = self.read_phys_u64(freelist)?;
        self.write_phys_u64(page, old_head)?;
        self.write_phys_u64(freelist, page)?;
        Ok(true)
    }

    fn xv6_kalloc_page(&mut self) -> Result<Option<u64>, Exception> {
        let accelerator = self.xv6_accelerator.expect("xv6 accelerator is configured");
        let freelist = accelerator.kmem + XV6_KMEM_FREELIST;
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
        // 总线写接口只接收 u32，物理 64 位值按小端低字在前拆分。
        self.bus.write(addr, value as u32, 4)?;
        self.bus.write(addr + 4, (value >> 32) as u32, 4)
    }

    fn fast_xv6_wakeup(&mut self) -> Result<bool, Exception> {
        let accelerator = self.xv6_accelerator.expect("xv6 accelerator is configured");
        let chan = self.registers[10];
        let current = self.read_u64(self.xv6_cpu_addr())?;
        let mut proc = accelerator.proc_start;
        while proc < accelerator.proc_end {
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
        // 该兼容路径只针对用户地址空间；内核同地址值不能被当作用户函数入口。
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
        let accelerator = self.xv6_accelerator.expect("xv6 accelerator is configured");
        let hart = self.registers[4] as i32 as i64 as u64;
        accelerator.cpus + hart.wrapping_mul(XV6_CPU_STRIDE)
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
    // xv6 PTE 的物理页号从 bit10 开始，恢复地址时重新补上 12 位页内零偏移。
    ((pte >> 10) & ((1u64 << 44) - 1)) << 12
}

fn xv6_pa_to_pte(pa: u64) -> u64 {
    // 物理地址必须按页编码；先移除页内偏移，再放到 PTE 的 PPN 位段。
    (pa >> 12) << 10
}

fn exception_trap_info(exception: Exception) -> Option<(u64, u64)> {
    // 在一个位置维护 Exception 到监督模式 scause/stval 的映射，避免各执行路径自行编码。
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dram::Dram;

    #[test]
    fn run_reuses_step_and_stops_at_the_limit() {
        let base = 0x8000_0000;
        let mut dram = Dram::with_layout(base, 16);
        dram.load_bytes(base, &[0x93, 0x0f, 0xa0, 0x02]).unwrap();
        let mut cpu = Cpu::with_reset_vector(Box::new(dram), base, base + 16);

        let outcome = cpu.run(RunOptions {
            max_steps: Some(1),
            debug: DebugLevel::Off,
        });

        assert!(matches!(outcome, RunOutcome::StepLimitReached { steps: 1 }));
        assert_eq!(cpu.pc, base + 4);
        assert_eq!(cpu.registers[31], 42);
        assert_eq!(cpu.cycles, TIMER_CYCLES_PER_STEP);
    }

    #[test]
    fn reset_restores_configured_entry_stack_and_csrs() {
        let base = 0x9000_0000;
        let mut cpu =
            Cpu::with_reset_vector(Box::new(Dram::with_layout(base, 16)), base + 4, base + 16);
        cpu.pc = base + 8;
        cpu.registers[2] = 0;
        cpu.csr.store(csr::SATP, 123);
        cpu.cycles = 99;

        cpu.reset();

        assert_eq!(cpu.pc, base + 4);
        assert_eq!(cpu.registers[2], base + 16);
        assert_eq!(cpu.csr.load(csr::SATP), 0);
        assert_eq!(cpu.cycles, 0);
    }
}
