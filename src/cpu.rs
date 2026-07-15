//! 单 hart RV64 CPU 状态和执行循环。
//!
//! [`Cpu::step`] 依次推进时间、处理中断、取指、尝试可选 xv6 加速、译码执行并提交 PC。
//! U/S/M 特权级、地址翻译和 trap 路由集中在本模块；具体物理内存和设备通过
//! [`MemDevice`] 注入。

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
    /// 当前执行特权级；复位后为机器模式。
    pub privilege: PrivilegeMode,
    /// 模拟周期计数；每次 `step` 按固定粒度增加。
    pub cycles: u64,
    reset_vector: u64,
    initial_sp: u64,
    pc_written: bool,
    xv6_accelerator: Option<Xv6Accelerator>,
}

/// RISC-V 基础特权级编码。
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum PrivilegeMode {
    User = 0,
    Supervisor = 1,
    Machine = 3,
}

impl PrivilegeMode {
    fn from_encoding(value: u64) -> Option<Self> {
        match value {
            0 => Some(Self::User),
            1 => Some(Self::Supervisor),
            3 => Some(Self::Machine),
            _ => None,
        }
    }
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
const INTERRUPT_FLAG: u64 = 1 << 63;
const INTERRUPT_PRIORITY: [u64; 6] = [11, 3, 7, 9, 1, 5];

const PTE_VALID: u64 = 1 << 0;
const PTE_READ: u64 = 1 << 1;
const PTE_WRITE: u64 = 1 << 2;
const PTE_EXECUTE: u64 = 1 << 3;
const PTE_USER: u64 = 1 << 4;
const PTE_ACCESSED: u64 = 1 << 6;
const PTE_DIRTY: u64 = 1 << 7;
const PTE_PPN_MASK: u64 = (1 << 44) - 1;
const PTE_RESERVED_SHIFT: u32 = 54;

const PMP_CFG_READ: u8 = 1 << 0;
const PMP_CFG_WRITE: u8 = 1 << 1;
const PMP_CFG_EXECUTE: u8 = 1 << 2;
const PMP_CFG_ADDRESS_SHIFT: u32 = 3;
const PMP_CFG_ADDRESS_MASK: u8 = 0b11;
const PMP_CFG_LOCKED: u8 = 1 << 7;
const PMP_ADDRESS_OFF: u8 = 0;
const PMP_ADDRESS_TOR: u8 = 1;
const PMP_ADDRESS_NA4: u8 = 2;
const PMP_ADDRESS_NAPOT: u8 = 3;

/// 一次虚拟地址访问的用途，用于选择页权限和页错误类型。
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
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
            privilege: PrivilegeMode::Machine,
            cycles: 0,
            reset_vector,
            initial_sp,
            pc_written: false,
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
        self.privilege = PrivilegeMode::Machine;
        self.cycles = 0;
        self.pc_written = false;
    }

    /// 启用与当前 xv6 镜像匹配的快速路径。
    pub fn set_xv6_accelerator(&mut self, accelerator: Xv6Accelerator) {
        self.xv6_accelerator = Some(accelerator);
    }

    /// 关闭 xv6 快速路径，使所有地址都按普通指令执行。
    pub fn clear_xv6_accelerator(&mut self) {
        self.xv6_accelerator = None;
    }

    pub(crate) fn xv6_acceleration_enabled(&self) -> bool {
        self.xv6_accelerator.is_some()
    }

    /// 推进一个 CPU 步骤。
    ///
    /// 中断或异常被架构 trap 入口接管时也算一个成功步骤。
    pub fn step(&mut self) -> Result<(), Exception> {
        // 先推进时间，使本步开始时即可观察到刚到期的定时器中断。
        self.tick();
        if self.take_pending_interrupt() {
            return Ok(());
        }
        let instruction = match self.fetch() {
            Ok(instruction) => instruction,
            Err(exception) => {
                self.trap_exception(exception);
                return Ok(());
            }
        };
        match self.try_xv6_fast_path() {
            Ok(true) => return Ok(()),
            Ok(false) => {}
            Err(exception) => {
                self.trap_exception(exception);
                return Ok(());
            }
        }
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
        if self.pc & 1 != 0 {
            return Err(Exception::InstructionAddrMisaligned(self.pc));
        }
        let addr = self.translate_sized(self.pc, MemoryAccess::Fetch, 4)?;
        self.bus
            .read(addr, 4)
            .map_err(|_| Exception::InstructionAccessFault(self.pc))
    }
    /// 译码并执行一条指令，返回提交后的 PC。
    fn execute(&mut self, instruction: u64) -> Result<u64, Exception> {
        let old_pc = self.pc;
        self.pc_written = false;
        let inst = instruction as u32;
        let decoded = instruction::decode(inst);
        match instruction::execute(self, decoded) {
            Ok(_) => {
                if self.pc_written {
                    Ok(self.pc)
                } else {
                    Ok(old_pc.wrapping_add(4))
                }
            }
            Err(exception) => {
                // 异常必须以故障指令 PC 作为 xEPC，因此先撤销执行器可能留下的 PC 变化。
                self.pc = old_pc;
                self.pc_written = false;
                self.trap_exception(exception);
                Ok(self.pc)
            }
        }
    }

    fn trap_exception(&mut self, exception: Exception) {
        let cause = exception.cause();
        let delegated = self.privilege != PrivilegeMode::Machine
            && self.csr.load(csr::MEDELEG) & (1 << cause) != 0;
        let target = if delegated {
            PrivilegeMode::Supervisor
        } else {
            PrivilegeMode::Machine
        };
        self.enter_trap(target, cause, exception.value());
    }

    /// 记录由当前指令明确写入的下一条 PC。
    pub(crate) fn write_pc(&mut self, pc: u64) {
        self.pc = pc;
        self.pc_written = true;
    }

    /// 检查当前特权级能否按指定方式访问 CSR。
    pub(crate) fn csr_access_allowed(&self, addr: usize, write: bool) -> bool {
        if !csr::Csr::is_implemented(addr) || (write && csr::Csr::is_read_only(addr)) {
            return false;
        }

        let required = (addr >> 8) & 0b11;
        if (self.privilege as usize) < required {
            return false;
        }

        if self.privilege == PrivilegeMode::Supervisor
            && addr == csr::SATP
            && self.csr.load(csr::MSTATUS) & csr::MASK_TVM != 0
        {
            return false;
        }

        if addr == csr::TIME {
            let machine_enabled = self.csr.load(csr::MCOUNTEREN) & csr::MASK_COUNTEREN_TM != 0;
            return match self.privilege {
                PrivilegeMode::Machine => true,
                PrivilegeMode::Supervisor => machine_enabled,
                PrivilegeMode::User => {
                    machine_enabled && self.csr.load(csr::SCOUNTEREN) & csr::MASK_COUNTEREN_TM != 0
                }
            };
        }

        if addr == csr::STIMECMP && self.privilege != PrivilegeMode::Machine {
            let enabled = self.csr.load(csr::MENVCFG) & csr::MASK_STCE != 0
                && self.csr.load(csr::MCOUNTEREN) & csr::MASK_COUNTEREN_TM != 0;
            if !enabled {
                return false;
            }
        }

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

    /// 根据当前有效特权级和 `SATP` 翻译地址，并检查 Sv39 页权限。
    pub fn translate(&mut self, addr: u64, access: MemoryAccess) -> Result<u64, Exception> {
        self.translate_sized(addr, access, 1)
    }

    pub(crate) fn translate_sized(
        &mut self,
        addr: u64,
        access: MemoryAccess,
        size: usize,
    ) -> Result<u64, Exception> {
        let privilege = self.effective_privilege(access);
        if privilege == PrivilegeMode::Machine {
            return self.check_physical_access(addr, size, privilege, access);
        }

        let satp = self.csr.load(csr::SATP);
        let mode = satp >> 60;
        if mode == 0 {
            return self.check_physical_access(addr, size, privilege, access);
        }
        if mode != 8 {
            return Err(page_fault(access, addr));
        }

        let sign = (addr >> 38) & 1;
        let upper = addr >> 39;
        let canonical_upper = if sign == 0 { 0 } else { (1 << 25) - 1 };
        if upper != canonical_upper {
            return Err(page_fault(access, addr));
        }

        // Sv39 每级索引 9 位，最低 12 位保留为页内偏移。
        let vpn = [
            (addr >> 12) & 0x1ff,
            (addr >> 21) & 0x1ff,
            (addr >> 30) & 0x1ff,
        ];
        // SATP 的低 44 位是根页表物理页号，恢复物理地址时补回 12 个零位。
        let mut table = (satp & PTE_PPN_MASK) << 12;

        for level in (0..=2).rev() {
            let pte_addr = table + vpn[level] * 8;
            self.check_pmp(pte_addr, 8, PrivilegeMode::Supervisor, MemoryAccess::Load)
                .map_err(|_| access_fault(access, addr))?;
            let pte = self
                .bus
                .read(pte_addr, 8)
                .map_err(|_| access_fault(access, addr))?;
            let valid = pte & PTE_VALID != 0;
            let readable = pte & PTE_READ != 0;
            let writable = pte & PTE_WRITE != 0;
            let executable = pte & PTE_EXECUTE != 0;
            let user = pte & PTE_USER != 0;
            let accessed = pte & PTE_ACCESSED != 0;
            let dirty = pte & PTE_DIRTY != 0;
            // RISC-V 将 W=1、R=0 和未实现的高位编码视为非法 PTE。
            if !valid || (writable && !readable) || pte >> PTE_RESERVED_SHIFT != 0 {
                return Err(page_fault(access, addr));
            }

            if readable || executable {
                let mstatus = self.csr.load(csr::MSTATUS);
                let privilege_allowed = match privilege {
                    PrivilegeMode::User => user,
                    PrivilegeMode::Supervisor if access == MemoryAccess::Fetch => !user,
                    PrivilegeMode::Supervisor => !user || mstatus & csr::MASK_SUM != 0,
                    PrivilegeMode::Machine => true,
                };
                let allowed = match access {
                    MemoryAccess::Fetch => executable,
                    MemoryAccess::Load => readable || (executable && mstatus & csr::MASK_MXR != 0),
                    MemoryAccess::Store => writable,
                };
                if !privilege_allowed || !allowed {
                    return Err(page_fault(access, addr));
                }

                let ppn = (pte >> 10) & PTE_PPN_MASK;
                let superpage_bits = 9 * level;
                if superpage_bits != 0 && ppn & ((1 << superpage_bits) - 1) != 0 {
                    return Err(page_fault(access, addr));
                }

                let page_bits = 12 + superpage_bits;
                let page_mask = (1u64 << page_bits) - 1;
                let physical = ((ppn << 12) & !page_mask) | (addr & page_mask);
                self.check_pmp(physical, size, privilege, access)
                    .map_err(|()| access_fault(access, addr))?;

                if !accessed || (access == MemoryAccess::Store && !dirty) {
                    let updated = pte
                        | PTE_ACCESSED
                        | if access == MemoryAccess::Store {
                            PTE_DIRTY
                        } else {
                            0
                        };
                    self.check_pmp(pte_addr, 8, PrivilegeMode::Supervisor, MemoryAccess::Store)
                        .map_err(|_| access_fault(access, addr))?;
                    self.bus
                        .write(pte_addr, updated as u32, 4)
                        .map_err(|_| access_fault(access, addr))?;
                }

                return Ok(physical);
            }

            if pte & (PTE_USER | PTE_ACCESSED | PTE_DIRTY) != 0 {
                return Err(page_fault(access, addr));
            }

            table = ((pte >> 10) & PTE_PPN_MASK) << 12;
        }

        Err(page_fault(access, addr))
    }

    fn effective_privilege(&self, access: MemoryAccess) -> PrivilegeMode {
        let mstatus = self.csr.load(csr::MSTATUS);
        if self.privilege == PrivilegeMode::Machine
            && access != MemoryAccess::Fetch
            && mstatus & csr::MASK_MPRV != 0
        {
            PrivilegeMode::from_encoding((mstatus & csr::MASK_MPP) >> 11)
                .expect("mstatus.MPP contains only implemented privilege modes")
        } else {
            self.privilege
        }
    }

    fn check_physical_access(
        &self,
        addr: u64,
        size: usize,
        privilege: PrivilegeMode,
        access: MemoryAccess,
    ) -> Result<u64, Exception> {
        self.check_pmp(addr, size, privilege, access)
            .map(|()| addr)
            .map_err(|()| access_fault(access, addr))
    }

    fn check_pmp(
        &self,
        addr: u64,
        size: usize,
        privilege: PrivilegeMode,
        access: MemoryAccess,
    ) -> Result<(), ()> {
        let end = addr
            .checked_add(size as u64)
            .filter(|end| *end > addr)
            .ok_or(())?;
        let required = match access {
            MemoryAccess::Fetch => PMP_CFG_EXECUTE,
            MemoryAccess::Load => PMP_CFG_READ,
            MemoryAccess::Store => PMP_CFG_WRITE,
        };
        let mut previous = 0;
        for index in 0..csr::PMP_ENTRIES {
            let config = self.csr.pmp_config(index);
            let address = self.csr.pmp_address(index);
            if let Some((start, limit)) = pmp_range(previous, address, config)
                && addr < limit
                && end > start
            {
                if addr < start || end > limit {
                    return Err(());
                }
                if privilege == PrivilegeMode::Machine && config & PMP_CFG_LOCKED == 0 {
                    return Ok(());
                }
                return if config & required != 0 {
                    Ok(())
                } else {
                    Err(())
                };
            }
            previous = address;
        }

        (privilege == PrivilegeMode::Machine)
            .then_some(())
            .ok_or(())
    }

    /// 保存监督模式 trap 状态并跳转到 `STVEC`。
    pub fn enter_supervisor_trap(&mut self, scause: u64, stval: u64) {
        self.enter_trap(PrivilegeMode::Supervisor, scause, stval);
    }

    /// 保存机器模式 trap 状态并跳转到 `MTVEC`。
    pub fn enter_machine_trap(&mut self, mcause: u64, mtval: u64) {
        self.enter_trap(PrivilegeMode::Machine, mcause, mtval);
    }

    fn enter_trap(&mut self, target: PrivilegeMode, cause: u64, value: u64) {
        let previous = self.privilege;
        let vector = match target {
            PrivilegeMode::Machine => {
                let mut mstatus = self.csr.load(csr::MSTATUS);
                if mstatus & csr::MASK_MIE != 0 {
                    mstatus |= csr::MASK_MPIE;
                } else {
                    mstatus &= !csr::MASK_MPIE;
                }
                mstatus = (mstatus & !csr::MASK_MPP) | ((previous as u64) << 11);
                mstatus &= !csr::MASK_MIE;
                self.csr.store(csr::MSTATUS, mstatus);
                self.csr.store(csr::MEPC, self.pc);
                self.csr.store(csr::MCAUSE, cause);
                self.csr.store(csr::MTVAL, value);
                self.csr.load(csr::MTVEC)
            }
            PrivilegeMode::Supervisor => {
                let mut sstatus = self.csr.load(csr::SSTATUS);
                if sstatus & csr::MASK_SIE != 0 {
                    sstatus |= csr::MASK_SPIE;
                } else {
                    sstatus &= !csr::MASK_SPIE;
                }
                if previous == PrivilegeMode::Supervisor {
                    sstatus |= csr::MASK_SPP;
                } else {
                    sstatus &= !csr::MASK_SPP;
                }
                sstatus &= !csr::MASK_SIE;
                self.csr.store(csr::SSTATUS, sstatus);
                self.csr.store(csr::SEPC, self.pc);
                self.csr.store(csr::SCAUSE, cause);
                self.csr.store(csr::STVAL, value);
                self.csr.load(csr::STVEC)
            }
            PrivilegeMode::User => unreachable!("traps are not delegated to user mode"),
        };

        self.privilege = target;
        self.pc = trap_vector(vector, cause);
        self.pc_written = true;
    }

    /// 恢复监督模式 trap 状态并返回 `SEPC`。
    pub fn supervisor_return(&mut self) {
        let mut sstatus = self.csr.load(csr::SSTATUS);
        let target = if sstatus & csr::MASK_SPP != 0 {
            PrivilegeMode::Supervisor
        } else {
            PrivilegeMode::User
        };
        if sstatus & csr::MASK_SPIE != 0 {
            sstatus |= csr::MASK_SIE;
        } else {
            sstatus &= !csr::MASK_SIE;
        }
        sstatus |= csr::MASK_SPIE;
        sstatus &= !csr::MASK_SPP;
        self.csr.store(csr::SSTATUS, sstatus);
        let mstatus = self.csr.load(csr::MSTATUS) & !csr::MASK_MPRV;
        self.csr.store(csr::MSTATUS, mstatus);
        self.privilege = target;
        self.write_pc(self.csr.load(csr::SEPC));
    }

    /// 恢复机器模式 trap 状态并返回 `MEPC`。
    pub fn machine_return(&mut self) {
        let mut mstatus = self.csr.load(csr::MSTATUS);
        let target = PrivilegeMode::from_encoding((mstatus & csr::MASK_MPP) >> 11)
            .expect("mstatus.MPP contains only implemented privilege modes");
        if mstatus & csr::MASK_MPIE != 0 {
            mstatus |= csr::MASK_MIE;
        } else {
            mstatus &= !csr::MASK_MIE;
        }
        mstatus |= csr::MASK_MPIE;
        mstatus &= !csr::MASK_MPP;
        if target != PrivilegeMode::Machine {
            mstatus &= !csr::MASK_MPRV;
        }
        self.csr.store(csr::MSTATUS, mstatus);
        self.privilege = target;
        self.write_pc(self.csr.load(csr::MEPC));
    }

    fn take_pending_interrupt(&mut self) -> bool {
        self.refresh_pending_interrupts();
        let pending = self.csr.load(csr::MIP) & self.csr.load(csr::MIE);
        let mideleg = self.csr.load(csr::MIDELEG);
        let mstatus = self.csr.load(csr::MSTATUS);

        // 标准中断的默认优先级：MEI、MSI、MTI、SEI、SSI、STI。
        for cause in INTERRUPT_PRIORITY {
            let bit = 1 << cause;
            if pending & bit == 0 {
                continue;
            }

            let target = if mideleg & bit != 0 {
                if self.privilege == PrivilegeMode::Machine
                    || (self.privilege == PrivilegeMode::Supervisor && mstatus & csr::MASK_SIE == 0)
                {
                    continue;
                }
                PrivilegeMode::Supervisor
            } else {
                if self.privilege == PrivilegeMode::Machine && mstatus & csr::MASK_MIE == 0 {
                    continue;
                }
                PrivilegeMode::Machine
            };

            self.enter_trap(target, INTERRUPT_FLAG | cause, 0);
            return true;
        }

        false
    }

    fn refresh_pending_interrupts(&mut self) {
        let timer_driven = self.csr.load(csr::MENVCFG) & csr::MASK_STCE != 0;
        let timer_pending = if timer_driven && self.timer_is_pending() {
            csr::MASK_STIP
        } else {
            0
        };

        let device_pending = self
            .bus
            .pending_interrupt()
            .filter(|cause| cause & INTERRUPT_FLAG != 0)
            .map(|cause| cause & !INTERRUPT_FLAG)
            .filter(|cause| *cause < 64)
            .map_or(0, |cause| 1 << cause);

        let hardware_pending = timer_pending | device_pending;
        self.csr.update_pending(hardware_pending);
    }

    fn tick(&mut self) {
        self.cycles = self.cycles.wrapping_add(TIMER_CYCLES_PER_STEP);
        self.csr.store(csr::TIME, self.cycles);
    }

    fn timer_is_pending(&self) -> bool {
        let stimecmp = self.csr.load(csr::STIMECMP);
        self.csr.load(csr::TIME) >= stimecmp
    }

    fn try_xv6_fast_path(&mut self) -> Result<bool, Exception> {
        let Some(accelerator) = self.xv6_accelerator else {
            return Ok(false);
        };
        if self.privilege == PrivilegeMode::User {
            return if accelerator.user_exec == Some(self.pc) {
                self.fast_xv6_user_exec()
            } else {
                Ok(false)
            };
        }
        if self.privilege != PrivilegeMode::Supervisor {
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
        // 该兼容路径只针对用户模式；同一地址不能在更高特权级误触发。
        if self.privilege != PrivilegeMode::User {
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

        if first_arg != 0
            && self
                .translate_sized(first_arg, MemoryAccess::Load, 1)
                .is_err()
        {
            self.registers[10] = u64::MAX;
            self.fast_return();
            return Ok(true);
        }

        Ok(false)
    }

    fn fast_return(&mut self) {
        self.registers[0] = 0;
        self.write_pc(self.registers[1]);
    }

    fn xv6_cpu_addr(&self) -> u64 {
        let accelerator = self.xv6_accelerator.expect("xv6 accelerator is configured");
        let hart = self.registers[4] as i32 as i64 as u64;
        accelerator.cpus + hart.wrapping_mul(XV6_CPU_STRIDE)
    }

    fn read_u8(&mut self, addr: u64) -> Result<u8, Exception> {
        let physical = self.translate_sized(addr, MemoryAccess::Load, 1)?;
        self.bus
            .read(physical, 1)
            .map(|value| value as u8)
            .map_err(|_| Exception::LoadAccessFault(addr))
    }

    fn read_u32(&mut self, addr: u64) -> Result<u32, Exception> {
        let physical = self.translate_sized(addr, MemoryAccess::Load, 4)?;
        self.bus
            .read(physical, 4)
            .map(|value| value as u32)
            .map_err(|_| Exception::LoadAccessFault(addr))
    }

    fn read_u64(&mut self, addr: u64) -> Result<u64, Exception> {
        let physical = self.translate_sized(addr, MemoryAccess::Load, 8)?;
        self.bus
            .read(physical, 8)
            .map_err(|_| Exception::LoadAccessFault(addr))
    }

    fn write_u8(&mut self, addr: u64, value: u8) -> Result<(), Exception> {
        let physical = self.translate_sized(addr, MemoryAccess::Store, 1)?;
        self.bus
            .write(physical, value as u32, 1)
            .map_err(|_| Exception::StoreAMOAccessFault(addr))
    }

    fn write_u32(&mut self, addr: u64, value: u32) -> Result<(), Exception> {
        let physical = self.translate_sized(addr, MemoryAccess::Store, 4)?;
        self.bus
            .write(physical, value, 4)
            .map_err(|_| Exception::StoreAMOAccessFault(addr))
    }

    fn write_u64(&mut self, addr: u64, value: u64) -> Result<(), Exception> {
        let physical = self.translate_sized(addr, MemoryAccess::Store, 8)?;
        self.bus
            .write(physical, value as u32, 4)
            .and_then(|()| self.bus.write(physical + 4, (value >> 32) as u32, 4))
            .map_err(|_| Exception::StoreAMOAccessFault(addr))
    }
}

fn page_fault(access: MemoryAccess, addr: u64) -> Exception {
    match access {
        MemoryAccess::Fetch => Exception::InstructionPageFault(addr),
        MemoryAccess::Load => Exception::LoadPageFault(addr),
        MemoryAccess::Store => Exception::StoreAMOPageFault(addr),
    }
}

fn access_fault(access: MemoryAccess, addr: u64) -> Exception {
    match access {
        MemoryAccess::Fetch => Exception::InstructionAccessFault(addr),
        MemoryAccess::Load => Exception::LoadAccessFault(addr),
        MemoryAccess::Store => Exception::StoreAMOAccessFault(addr),
    }
}

fn trap_vector(vector: u64, cause: u64) -> u64 {
    let base = vector & !0b11;
    if vector & 0b11 == 1 && cause & INTERRUPT_FLAG != 0 {
        base.wrapping_add((cause & !INTERRUPT_FLAG).wrapping_mul(4))
    } else {
        base
    }
}

fn pmp_range(previous: u64, address: u64, config: u8) -> Option<(u64, u64)> {
    match (config >> PMP_CFG_ADDRESS_SHIFT) & PMP_CFG_ADDRESS_MASK {
        PMP_ADDRESS_OFF => None,
        PMP_ADDRESS_TOR => Some((previous << 2, address << 2)),
        PMP_ADDRESS_NA4 => {
            let start = address << 2;
            Some((start, start.checked_add(4)?))
        }
        PMP_ADDRESS_NAPOT => {
            let ones = address.trailing_ones();
            let encoded_mask = if ones == 0 { 0 } else { (1u64 << ones) - 1 };
            let start = (address & !encoded_mask) << 2;
            let size = 1u64 << (ones + 3);
            Some((start, start.checked_add(size)?))
        }
        _ => unreachable!(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dram::Dram;

    const BASE: u64 = 0x8000_0000;
    const MEMORY_SIZE: usize = 0x10_000;

    fn test_cpu() -> Cpu {
        Cpu::with_reset_vector(
            Box::new(Dram::with_layout(BASE, MEMORY_SIZE)),
            BASE,
            BASE + MEMORY_SIZE as u64,
        )
    }

    fn execute_raw(cpu: &mut Cpu, raw: u32) {
        cpu.pc = cpu.execute(u64::from(raw)).unwrap();
    }

    fn allow_all_memory(cpu: &mut Cpu) {
        cpu.csr.store(csr::PMPADDR0, (1u64 << 54) - 1);
        cpu.csr.store(csr::PMPCFG0, 0x0f);
    }

    fn write_phys_u64(cpu: &mut Cpu, addr: u64, value: u64) {
        cpu.bus.write(addr, value as u32, 4).unwrap();
        cpu.bus.write(addr + 4, (value >> 32) as u32, 4).unwrap();
    }

    fn install_sv39_mapping(cpu: &mut Cpu, virtual_addr: u64, physical: u64, flags: u64) -> u64 {
        let root = BASE;
        let level_1 = BASE + 0x1000;
        let level_0 = BASE + 0x2000;
        let vpn = [
            (virtual_addr >> 12) & 0x1ff,
            (virtual_addr >> 21) & 0x1ff,
            (virtual_addr >> 30) & 0x1ff,
        ];
        write_phys_u64(cpu, root + vpn[2] * 8, xv6_pa_to_pte(level_1) | 1);
        write_phys_u64(cpu, level_1 + vpn[1] * 8, xv6_pa_to_pte(level_0) | 1);
        let leaf = level_0 + vpn[0] * 8;
        write_phys_u64(cpu, leaf, xv6_pa_to_pte(physical) | flags | 1);
        cpu.csr.store(csr::SATP, (8 << 60) | (root >> 12));
        leaf
    }

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
        assert_eq!(cpu.privilege, PrivilegeMode::Machine);
    }

    #[test]
    fn delegated_user_trap_and_sret_restore_supervisor_state() {
        let mut cpu = test_cpu();
        cpu.privilege = PrivilegeMode::User;
        cpu.pc = 0x1000;
        cpu.csr.store(csr::STVEC, 0x2001);
        cpu.csr.store(csr::MEDELEG, 1 << 8);
        cpu.csr.store(csr::SSTATUS, csr::MASK_SIE);

        execute_raw(&mut cpu, 0x0000_0073);

        assert_eq!(cpu.privilege, PrivilegeMode::Supervisor);
        assert_eq!(cpu.pc, 0x2000);
        assert_eq!(cpu.csr.load(csr::SEPC), 0x1000);
        assert_eq!(cpu.csr.load(csr::SCAUSE), 8);
        assert_eq!(cpu.csr.load(csr::STVAL), 0);
        assert_eq!(cpu.csr.load(csr::SSTATUS) & csr::MASK_SIE, 0);
        assert_ne!(cpu.csr.load(csr::SSTATUS) & csr::MASK_SPIE, 0);
        assert_eq!(cpu.csr.load(csr::SSTATUS) & csr::MASK_SPP, 0);

        cpu.csr.store(csr::SEPC, 0x1004);
        cpu.csr
            .store(csr::MSTATUS, cpu.csr.load(csr::MSTATUS) | csr::MASK_MPRV);
        execute_raw(&mut cpu, 0x1020_0073);

        assert_eq!(cpu.privilege, PrivilegeMode::User);
        assert_eq!(cpu.pc, 0x1004);
        assert_ne!(cpu.csr.load(csr::SSTATUS) & csr::MASK_SIE, 0);
        assert_ne!(cpu.csr.load(csr::SSTATUS) & csr::MASK_SPIE, 0);
        assert_eq!(cpu.csr.load(csr::MSTATUS) & csr::MASK_MPRV, 0);
    }

    #[test]
    fn step_executes_mret_then_routes_a_user_ecall() {
        let mut dram = Dram::with_layout(BASE, 16);
        dram.load_bytes(
            BASE,
            &[
                0x73, 0x00, 0x20, 0x30, // mret
                0x73, 0x00, 0x00, 0x00, // ecall
            ],
        )
        .unwrap();
        let mut cpu = Cpu::with_reset_vector(Box::new(dram), BASE, BASE + 16);
        allow_all_memory(&mut cpu);
        cpu.csr.store(csr::MEPC, BASE + 4);
        cpu.csr.store(csr::MEDELEG, 1 << 8);
        cpu.csr.store(csr::STVEC, BASE + 8);

        cpu.step().unwrap();
        assert_eq!(cpu.privilege, PrivilegeMode::User);
        assert_eq!(cpu.pc, BASE + 4);

        cpu.step().unwrap();
        assert_eq!(cpu.privilege, PrivilegeMode::Supervisor);
        assert_eq!(cpu.pc, BASE + 8);
        assert_eq!(cpu.csr.load(csr::SEPC), BASE + 4);
        assert_eq!(cpu.csr.load(csr::SCAUSE), 8);
    }

    #[test]
    fn delegated_supervisor_traps_return_to_supervisor_mode() {
        let mut cpu = test_cpu();
        cpu.privilege = PrivilegeMode::Supervisor;
        cpu.pc = 0x1000;
        cpu.csr.store(csr::STVEC, 0x2000);
        cpu.csr.store(csr::MEDELEG, 1 << 9);

        execute_raw(&mut cpu, 0x0000_0073);
        assert_eq!(cpu.privilege, PrivilegeMode::Supervisor);
        assert_eq!(cpu.csr.load(csr::SCAUSE), 9);
        assert_ne!(cpu.csr.load(csr::SSTATUS) & csr::MASK_SPP, 0);

        cpu.csr.store(csr::SEPC, 0x1004);
        execute_raw(&mut cpu, 0x1020_0073);
        assert_eq!(cpu.privilege, PrivilegeMode::Supervisor);
        assert_eq!(cpu.pc, 0x1004);
        assert_eq!(cpu.csr.load(csr::SSTATUS) & csr::MASK_SPP, 0);
    }

    #[test]
    fn machine_trap_and_mret_restore_the_previous_mode() {
        let mut cpu = test_cpu();
        cpu.privilege = PrivilegeMode::Supervisor;
        cpu.pc = 0x1800;
        cpu.csr.store(csr::MTVEC, 0x3001);
        cpu.csr.store(csr::MSTATUS, csr::MASK_MIE | csr::MASK_MPRV);

        execute_raw(&mut cpu, 0x0000_0073);

        assert_eq!(cpu.privilege, PrivilegeMode::Machine);
        assert_eq!(cpu.pc, 0x3000);
        assert_eq!(cpu.csr.load(csr::MEPC), 0x1800);
        assert_eq!(cpu.csr.load(csr::MCAUSE), 9);
        assert_eq!(
            cpu.csr.load(csr::MSTATUS) & csr::MASK_MPP,
            (PrivilegeMode::Supervisor as u64) << 11
        );
        assert_ne!(cpu.csr.load(csr::MSTATUS) & csr::MASK_MPIE, 0);
        assert_eq!(cpu.csr.load(csr::MSTATUS) & csr::MASK_MIE, 0);

        cpu.csr.store(csr::MEPC, 0x1804);
        execute_raw(&mut cpu, 0x3020_0073);

        assert_eq!(cpu.privilege, PrivilegeMode::Supervisor);
        assert_eq!(cpu.pc, 0x1804);
        assert_ne!(cpu.csr.load(csr::MSTATUS) & csr::MASK_MIE, 0);
        assert_ne!(cpu.csr.load(csr::MSTATUS) & csr::MASK_MPIE, 0);
        assert_eq!(cpu.csr.load(csr::MSTATUS) & csr::MASK_MPP, 0);
        assert_eq!(cpu.csr.load(csr::MSTATUS) & csr::MASK_MPRV, 0);
    }

    #[test]
    fn machine_mode_exceptions_are_never_delegated() {
        let mut cpu = test_cpu();
        cpu.pc = 0x1800;
        cpu.csr.store(csr::MTVEC, 0x3000);
        cpu.csr.store(csr::STVEC, 0x4000);
        cpu.csr.store(csr::MEDELEG, 1 << 2);

        execute_raw(&mut cpu, u32::MAX);

        assert_eq!(cpu.privilege, PrivilegeMode::Machine);
        assert_eq!(cpu.pc, 0x3000);
        assert_eq!(cpu.csr.load(csr::MCAUSE), 2);
        assert_eq!(cpu.csr.load(csr::MEPC), 0x1800);
    }

    #[test]
    fn interrupts_obey_delegation_global_enable_and_vector_mode() {
        let mut cpu = test_cpu();
        cpu.privilege = PrivilegeMode::User;
        cpu.pc = 0x1000;
        cpu.csr.store(csr::STVEC, 0x2001);
        cpu.csr.store(csr::MIDELEG, csr::MASK_SEIP);
        cpu.csr.store(csr::MIE, csr::MASK_SEIP);
        cpu.csr.store(csr::MIP, csr::MASK_SEIP);

        assert!(cpu.take_pending_interrupt());
        assert_eq!(cpu.privilege, PrivilegeMode::Supervisor);
        assert_eq!(cpu.pc, 0x2000 + 4 * 9);
        assert_eq!(cpu.csr.load(csr::SCAUSE), (1 << 63) | 9);

        let mut cpu = test_cpu();
        cpu.privilege = PrivilegeMode::Machine;
        cpu.csr.store(csr::MIDELEG, csr::MASK_SEIP);
        cpu.csr.store(csr::MIE, csr::MASK_SEIP);
        cpu.csr.store(csr::MIP, csr::MASK_SEIP);
        cpu.csr.store(csr::MSTATUS, csr::MASK_MIE);
        assert!(!cpu.take_pending_interrupt());

        let mut cpu = test_cpu();
        cpu.privilege = PrivilegeMode::Supervisor;
        cpu.csr.store(csr::MIDELEG, csr::MASK_SEIP);
        cpu.csr.store(csr::MIE, csr::MASK_SEIP);
        cpu.csr.store(csr::MIP, csr::MASK_SEIP);
        assert!(!cpu.take_pending_interrupt());
        cpu.csr.store(csr::SSTATUS, csr::MASK_SIE);
        assert!(cpu.take_pending_interrupt());

        let mut cpu = test_cpu();
        cpu.csr.store(csr::MIE, csr::MASK_SEIP);
        cpu.csr.store(csr::MIP, csr::MASK_SEIP);
        assert!(!cpu.take_pending_interrupt());
        cpu.csr.store(csr::MSTATUS, csr::MASK_MIE);
        assert!(cpu.take_pending_interrupt());

        let mut cpu = test_cpu();
        cpu.privilege = PrivilegeMode::Supervisor;
        cpu.pc = 0x4000;
        cpu.csr.store(csr::MTVEC, 0x5001);
        cpu.csr.store(csr::MIDELEG, 0);
        cpu.csr.store(csr::MIE, csr::MASK_SEIP);
        cpu.csr.store(csr::MIP, csr::MASK_SEIP);
        assert!(cpu.take_pending_interrupt());
        assert_eq!(cpu.privilege, PrivilegeMode::Machine);
        assert_eq!(cpu.pc, 0x5000 + 4 * 9);
        assert_eq!(cpu.csr.load(csr::MCAUSE), (1 << 63) | 9);
    }

    #[test]
    fn csr_and_privileged_instruction_accesses_are_checked() {
        let mut cpu = test_cpu();
        cpu.privilege = PrivilegeMode::Supervisor;
        assert!(!cpu.csr_access_allowed(csr::MSTATUS, false));
        assert!(!cpu.csr_access_allowed(csr::TIME, false));
        cpu.csr.store(csr::MCOUNTEREN, csr::MASK_COUNTEREN_TM);
        assert!(cpu.csr_access_allowed(csr::TIME, false));
        assert!(!cpu.csr_access_allowed(csr::TIME, true));
        assert!(!cpu.csr_access_allowed(csr::STIMECMP, true));
        cpu.csr.store(csr::MENVCFG, csr::MASK_STCE);
        assert!(cpu.csr_access_allowed(csr::STIMECMP, true));
        cpu.csr.store(csr::MSTATUS, csr::MASK_TVM | csr::MASK_TSR);
        assert!(!cpu.csr_access_allowed(csr::SATP, false));

        cpu.privilege = PrivilegeMode::User;
        assert!(!cpu.csr_access_allowed(csr::TIME, false));
        cpu.csr.store(csr::SCOUNTEREN, csr::MASK_COUNTEREN_TM);
        assert!(cpu.csr_access_allowed(csr::TIME, false));
        cpu.privilege = PrivilegeMode::Supervisor;

        cpu.pc = 0x6000;
        cpu.csr.store(csr::STVEC, 0x7000);
        cpu.csr.store(csr::MEDELEG, 1 << 2);
        execute_raw(&mut cpu, 0x1020_0073);
        assert_eq!(cpu.privilege, PrivilegeMode::Supervisor);
        assert_eq!(cpu.pc, 0x7000);
        assert_eq!(cpu.csr.load(csr::SCAUSE), 2);
        assert_eq!(cpu.csr.load(csr::STVAL), 0x1020_0073);

        let mut cpu = test_cpu();
        cpu.privilege = PrivilegeMode::User;
        cpu.pc = 0x8000;
        cpu.csr.store(csr::STVEC, 0x9000);
        cpu.csr.store(csr::MEDELEG, 1 << 2);
        let csrrs_mstatus = ((csr::MSTATUS as u32) << 20) | (2 << 12) | (1 << 7) | 0x73;
        execute_raw(&mut cpu, csrrs_mstatus);
        assert_eq!(cpu.privilege, PrivilegeMode::Supervisor);
        assert_eq!(cpu.csr.load(csr::SCAUSE), 2);
        assert_eq!(cpu.csr.load(csr::STVAL), u64::from(csrrs_mstatus));
    }

    #[test]
    fn csr_instructions_reject_read_only_and_unimplemented_registers() {
        let mut cpu = test_cpu();
        cpu.csr.store(csr::TIME, 123);
        let read_time = ((csr::TIME as u32) << 20) | (2 << 12) | (1 << 7) | 0x73;
        execute_raw(&mut cpu, read_time);
        assert_eq!(cpu.registers[1], 123);

        cpu.csr.update_pending(csr::MASK_SEIP);
        cpu.registers[2] = csr::MASK_SSIP;
        let set_mip = ((csr::MIP as u32) << 20) | (2 << 15) | (2 << 12) | (1 << 7) | 0x73;
        execute_raw(&mut cpu, set_mip);
        assert_eq!(cpu.registers[1] & csr::MASK_SEIP, csr::MASK_SEIP);
        cpu.csr.update_pending(0);
        assert_eq!(cpu.csr.load(csr::MIP), csr::MASK_SSIP);

        cpu.csr.store(csr::MTVEC, 0x7000);
        let write_time = ((csr::TIME as u32) << 20) | (1 << 15) | (1 << 12) | 0x73;
        execute_raw(&mut cpu, write_time);
        assert_eq!(cpu.csr.load(csr::MCAUSE), 2);
        assert_eq!(cpu.csr.load(csr::MTVAL), u64::from(write_time));

        cpu.pc = 0x8000;
        let unimplemented = (0x7ff << 20) | (2 << 12) | (1 << 7) | 0x73;
        execute_raw(&mut cpu, unimplemented);
        assert_eq!(cpu.csr.load(csr::MCAUSE), 2);
        assert_eq!(cpu.csr.load(csr::MTVAL), u64::from(unimplemented));
    }

    #[test]
    fn lower_modes_cannot_execute_higher_privilege_instructions() {
        for raw in [0x1020_0073, 0x1050_0073, 0x1200_0073, 0x3020_0073] {
            let mut cpu = test_cpu();
            cpu.privilege = PrivilegeMode::User;
            cpu.pc = 0x6000;
            cpu.csr.store(csr::STVEC, 0x7000);
            cpu.csr.store(csr::MEDELEG, 1 << 2);

            execute_raw(&mut cpu, raw);

            assert_eq!(cpu.privilege, PrivilegeMode::Supervisor);
            assert_eq!(cpu.csr.load(csr::SCAUSE), 2);
            assert_eq!(cpu.csr.load(csr::STVAL), u64::from(raw));
        }
    }

    #[test]
    fn pmp_enforces_permissions_for_lower_modes_and_locked_machine_access() {
        let mut cpu = test_cpu();
        cpu.privilege = PrivilegeMode::Supervisor;
        assert_eq!(
            cpu.translate_sized(BASE, MemoryAccess::Load, 8),
            Err(Exception::LoadAccessFault(BASE))
        );

        cpu.csr.store(csr::PMPADDR0, (BASE + 0x1000) >> 2);
        cpu.csr.store(csr::PMPCFG0, 0x0d);
        assert_eq!(cpu.translate_sized(BASE, MemoryAccess::Load, 8), Ok(BASE));
        assert_eq!(
            cpu.translate_sized(BASE, MemoryAccess::Store, 8),
            Err(Exception::StoreAMOAccessFault(BASE))
        );
        assert_eq!(
            cpu.translate_sized(BASE + 0x1000, MemoryAccess::Load, 1),
            Err(Exception::LoadAccessFault(BASE + 0x1000))
        );

        cpu.privilege = PrivilegeMode::Machine;
        assert_eq!(cpu.translate_sized(BASE, MemoryAccess::Store, 8), Ok(BASE));
        cpu.csr.store(csr::PMPCFG0, 0x8d);
        assert_eq!(
            cpu.translate_sized(BASE, MemoryAccess::Store, 8),
            Err(Exception::StoreAMOAccessFault(BASE))
        );
    }

    #[test]
    fn pmp_uses_the_first_overlapping_entry_and_rejects_partial_matches() {
        let mut cpu = test_cpu();
        let eight_byte_napot = BASE >> 2;
        let page_napot = (BASE >> 2) | 0x1ff;
        cpu.csr.store(csr::PMPADDR0, eight_byte_napot);
        cpu.csr.store(csr::PMPADDR0 + 1, page_napot);
        cpu.csr.store(csr::PMPCFG0, 0x18 | (0x1f << 8));
        cpu.privilege = PrivilegeMode::Supervisor;

        assert_eq!(
            cpu.translate_sized(BASE, MemoryAccess::Load, 1),
            Err(Exception::LoadAccessFault(BASE))
        );
        assert_eq!(
            cpu.translate_sized(BASE + 4, MemoryAccess::Load, 8),
            Err(Exception::LoadAccessFault(BASE + 4))
        );
        assert_eq!(
            cpu.translate_sized(BASE + 8, MemoryAccess::Load, 8),
            Ok(BASE + 8)
        );

        let mut cpu = test_cpu();
        cpu.csr.store(csr::PMPADDR0, BASE >> 2);
        cpu.csr.store(csr::PMPADDR0 + 1, (BASE + 0x1000) >> 2);
        cpu.csr.store(csr::PMPCFG0, 0x0d << 8);
        cpu.privilege = PrivilegeMode::Supervisor;
        assert_eq!(cpu.translate(BASE, MemoryAccess::Load), Ok(BASE));
        assert_eq!(
            cpu.translate(BASE - 1, MemoryAccess::Load),
            Err(Exception::LoadAccessFault(BASE - 1))
        );
    }

    #[test]
    fn mprv_uses_mpp_for_data_but_not_instruction_fetches() {
        const VIRTUAL: u64 = 0x4000;
        const PHYSICAL: u64 = BASE + 0x4000;

        let mut cpu = test_cpu();
        allow_all_memory(&mut cpu);
        install_sv39_mapping(
            &mut cpu,
            VIRTUAL,
            PHYSICAL,
            PTE_READ | PTE_USER | PTE_ACCESSED,
        );
        cpu.csr.store(
            csr::MSTATUS,
            csr::MASK_MPRV | ((PrivilegeMode::User as u64) << 11),
        );

        assert_eq!(cpu.translate(VIRTUAL, MemoryAccess::Load), Ok(PHYSICAL));
        assert_eq!(cpu.translate(VIRTUAL, MemoryAccess::Fetch), Ok(VIRTUAL));
    }

    #[test]
    fn timer_pending_state_is_visible_and_clears_when_sstc_is_disabled() {
        let mut cpu = test_cpu();
        cpu.csr.store(csr::MIDELEG, csr::MASK_STIP);
        cpu.csr.store(csr::MENVCFG, csr::MASK_STCE);
        cpu.csr.store(csr::STIMECMP, 10);
        cpu.csr.store(csr::TIME, 10);

        cpu.refresh_pending_interrupts();
        assert_ne!(cpu.csr.load(csr::MIP) & csr::MASK_STIP, 0);
        assert_ne!(cpu.csr.load(csr::SIP) & csr::MASK_STIP, 0);

        cpu.csr.store(csr::MENVCFG, 0);
        cpu.refresh_pending_interrupts();
        assert_eq!(cpu.csr.load(csr::MIP) & csr::MASK_STIP, 0);
    }

    #[test]
    fn fetch_failures_enter_machine_traps_with_instruction_causes() {
        let mut cpu = test_cpu();
        cpu.csr.store(csr::MTVEC, BASE);
        cpu.pc = BASE + MEMORY_SIZE as u64;

        cpu.step().unwrap();
        assert_eq!(cpu.csr.load(csr::MCAUSE), 1);
        assert_eq!(cpu.csr.load(csr::MTVAL), BASE + MEMORY_SIZE as u64);

        cpu.pc = BASE + 1;
        cpu.step().unwrap();
        assert_eq!(cpu.csr.load(csr::MCAUSE), 0);
        assert_eq!(cpu.csr.load(csr::MTVAL), BASE + 1);
    }

    #[test]
    fn sv39_enforces_privilege_permissions_and_updates_ad_bits() {
        const VIRTUAL: u64 = 0x4000;
        const PHYSICAL: u64 = BASE + 0x4000;
        const PTE_R: u64 = 1 << 1;
        const PTE_W: u64 = 1 << 2;
        const PTE_X: u64 = 1 << 3;
        const PTE_U: u64 = 1 << 4;
        const PTE_A: u64 = 1 << 6;
        const PTE_D: u64 = 1 << 7;

        let mut cpu = test_cpu();
        allow_all_memory(&mut cpu);
        let leaf = install_sv39_mapping(&mut cpu, VIRTUAL, PHYSICAL, PTE_R | PTE_W | PTE_U);
        cpu.privilege = PrivilegeMode::User;

        assert_eq!(
            cpu.translate_sized(VIRTUAL, MemoryAccess::Load, 8),
            Ok(PHYSICAL)
        );
        assert_ne!(cpu.bus.read(leaf, 8).unwrap() & PTE_A, 0);
        assert_eq!(
            cpu.translate_sized(VIRTUAL, MemoryAccess::Store, 8),
            Ok(PHYSICAL)
        );
        assert_ne!(cpu.bus.read(leaf, 8).unwrap() & PTE_D, 0);

        cpu.privilege = PrivilegeMode::Supervisor;
        assert_eq!(
            cpu.translate(VIRTUAL, MemoryAccess::Load),
            Err(Exception::LoadPageFault(VIRTUAL))
        );
        cpu.csr.store(csr::SSTATUS, csr::MASK_SUM);
        assert_eq!(cpu.translate(VIRTUAL, MemoryAccess::Load), Ok(PHYSICAL));
        assert_eq!(
            cpu.translate(VIRTUAL, MemoryAccess::Fetch),
            Err(Exception::InstructionPageFault(VIRTUAL))
        );

        write_phys_u64(&mut cpu, leaf, xv6_pa_to_pte(PHYSICAL) | PTE_X | PTE_A | 1);
        cpu.csr.store(csr::SSTATUS, 0);
        assert_eq!(
            cpu.translate(VIRTUAL, MemoryAccess::Load),
            Err(Exception::LoadPageFault(VIRTUAL))
        );
        cpu.csr.store(csr::SSTATUS, csr::MASK_MXR);
        assert_eq!(cpu.translate(VIRTUAL, MemoryAccess::Load), Ok(PHYSICAL));

        cpu.privilege = PrivilegeMode::User;
        let noncanonical = 1 << 39;
        assert_eq!(
            cpu.translate(noncanonical, MemoryAccess::Load),
            Err(Exception::LoadPageFault(noncanonical))
        );

        cpu.privilege = PrivilegeMode::Machine;
        assert_eq!(
            cpu.translate(noncanonical, MemoryAccess::Load),
            Ok(noncanonical)
        );
    }

    #[test]
    fn translated_pmp_faults_report_the_virtual_address() {
        const VIRTUAL: u64 = 0x4000;
        const PHYSICAL: u64 = BASE + 0x4000;

        let mut cpu = test_cpu();
        install_sv39_mapping(
            &mut cpu,
            VIRTUAL,
            PHYSICAL,
            PTE_READ | PTE_USER | PTE_ACCESSED,
        );
        cpu.csr.store(csr::PMPADDR0, (BASE + 0x3000) >> 2);
        cpu.csr.store(csr::PMPCFG0, 0x0b);
        cpu.privilege = PrivilegeMode::User;

        assert_eq!(
            cpu.translate(VIRTUAL, MemoryAccess::Load),
            Err(Exception::LoadAccessFault(VIRTUAL))
        );
    }

    #[test]
    fn translated_bus_faults_report_the_virtual_address() {
        const VIRTUAL: u64 = 0x4000;
        const UNMAPPED_PHYSICAL: u64 = BASE + MEMORY_SIZE as u64 + 0x1000;

        let mut cpu = test_cpu();
        allow_all_memory(&mut cpu);
        install_sv39_mapping(
            &mut cpu,
            VIRTUAL,
            UNMAPPED_PHYSICAL,
            PTE_READ | PTE_USER | PTE_ACCESSED,
        );
        cpu.privilege = PrivilegeMode::User;
        cpu.pc = 0x6000;
        cpu.registers[1] = VIRTUAL;
        cpu.csr.store(csr::STVEC, 0x7000);
        cpu.csr.store(csr::MEDELEG, 1 << 5);
        let load = (1 << 15) | (3 << 12) | (2 << 7) | 0x03;

        execute_raw(&mut cpu, load);

        assert_eq!(cpu.privilege, PrivilegeMode::Supervisor);
        assert_eq!(cpu.csr.load(csr::SCAUSE), 5);
        assert_eq!(cpu.csr.load(csr::STVAL), VIRTUAL);
    }
}
