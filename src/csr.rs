//! 控制与状态寄存器（CSR）存储。
//!
//! [`Csr`] 用 4096 项数组覆盖 12 位 CSR 地址空间，并为 `SSTATUS`、`SIE`、`SIP`
//! 提供机器级寄存器的监督模式视图。CSR 指令的权限检查由 CPU 完成，本模块负责别名和
//! 当前实现支持字段的 WARL 约束。

/// 机器供应商标识；非商业实现返回 0。
pub const MVENDORID: usize = 0xf11;
/// 机器架构标识；未分配时返回 0。
pub const MARCHID: usize = 0xf12;
/// 机器实现标识；未分配时返回 0。
pub const MIMPID: usize = 0xf13;
/// hart 标识寄存器。
pub const MHARTID: usize = 0xf14;
/// 机器配置结构指针；当前平台没有该结构，返回 0。
pub const MCONFIGPTR: usize = 0xf15;
/// 机器模式状态寄存器。
pub const MSTATUS: usize = 0x300;
/// 机器 ISA 能力寄存器；当前实现按规范允许的方式固定返回 0。
pub const MISA: usize = 0x301;
/// 机器模式异常委托寄存器。
pub const MEDELEG: usize = 0x302;
/// 机器模式中断委托寄存器。
pub const MIDELEG: usize = 0x303;
/// 机器模式中断使能寄存器。
pub const MIE: usize = 0x304;
/// 机器模式 trap 入口寄存器。
pub const MTVEC: usize = 0x305;
/// 机器模式计数器使能寄存器。
pub const MCOUNTEREN: usize = 0x306;
/// 机器模式环境配置寄存器。
pub const MENVCFG: usize = 0x30a;
/// 机器模式 trap 临时寄存器。
pub const MSCRATCH: usize = 0x340;
/// 机器模式异常返回 PC。
pub const MEPC: usize = 0x341;
/// 机器模式 trap 原因。
pub const MCAUSE: usize = 0x342;
/// 机器模式 trap 附加地址或指令值。
pub const MTVAL: usize = 0x343;
/// 机器模式中断待处理位。
pub const MIP: usize = 0x344;
/// 物理内存保护配置项 0 到 7。
pub const PMPCFG0: usize = 0x3a0;
/// 物理内存保护配置项 8 到 15。
pub const PMPCFG2: usize = 0x3a2;
/// 第一项物理内存保护地址 CSR。
pub const PMPADDR0: usize = 0x3b0;
/// 物理内存保护地址 CSR 的数量。
pub const PMP_ENTRIES: usize = 16;

// 监督模式 CSR。
/// 监督模式状态寄存器，是 `MSTATUS` 中监督模式可见位的视图。
pub const SSTATUS: usize = 0x100;
/// 监督模式中断使能寄存器。
pub const SIE: usize = 0x104;
/// 监督模式 trap 入口寄存器。
pub const STVEC: usize = 0x105;
/// 监督模式计数器使能寄存器。
pub const SCOUNTEREN: usize = 0x106;
/// 监督模式 trap 临时寄存器。
pub const SSCRATCH: usize = 0x140;
/// 监督模式异常返回 PC。
pub const SEPC: usize = 0x141;
/// 监督模式 trap 原因。
pub const SCAUSE: usize = 0x142;
/// 监督模式 trap 附加地址或指令值。
pub const STVAL: usize = 0x143;
/// 监督模式中断待处理寄存器。
pub const SIP: usize = 0x144;
/// 监督模式地址翻译与保护寄存器。
pub const SATP: usize = 0x180;
/// 监督模式定时器比较值。
pub const STIMECMP: usize = 0x14d;

// 非特权计数器与计时器。
/// 当前模拟时间。
pub const TIME: usize = 0xc01;

// mstatus/sstatus 字段掩码。
pub const MASK_SIE: u64 = 1 << 1;
pub const MASK_MIE: u64 = 1 << 3;
pub const MASK_SPIE: u64 = 1 << 5;
pub const MASK_UBE: u64 = 1 << 6;
pub const MASK_MPIE: u64 = 1 << 7;
pub const MASK_SPP: u64 = 1 << 8;
pub const MASK_VS: u64 = 0b11 << 9;
pub const MASK_MPP: u64 = 0b11 << 11;
pub const MASK_FS: u64 = 0b11 << 13;
pub const MASK_XS: u64 = 0b11 << 15;
pub const MASK_MPRV: u64 = 1 << 17;
pub const MASK_SUM: u64 = 1 << 18;
pub const MASK_MXR: u64 = 1 << 19;
pub const MASK_TVM: u64 = 1 << 20;
pub const MASK_TW: u64 = 1 << 21;
pub const MASK_TSR: u64 = 1 << 22;
pub const MASK_UXL: u64 = 0b11 << 32;
pub const MASK_SXL: u64 = 0b11 << 34;
pub const MASK_SBE: u64 = 1 << 36;
pub const MASK_MBE: u64 = 1 << 37;
pub const MASK_SD: u64 = 1 << 63;
pub const MASK_SSTATUS: u64 = MASK_SIE
    | MASK_SPIE
    | MASK_UBE
    | MASK_SPP
    | MASK_FS
    | MASK_XS
    | MASK_SUM
    | MASK_MXR
    | MASK_UXL
    | MASK_SD;

// MIP/SIP 中断位掩码。
pub const MASK_SSIP: u64 = 1 << 1;
pub const MASK_MSIP: u64 = 1 << 3;
pub const MASK_STIP: u64 = 1 << 5;
pub const MASK_MTIP: u64 = 1 << 7;
pub const MASK_SEIP: u64 = 1 << 9;
pub const MASK_MEIP: u64 = 1 << 11;

/// 本实现支持的机器中断位。
pub const MASK_INTERRUPTS: u64 =
    MASK_SSIP | MASK_MSIP | MASK_STIP | MASK_MTIP | MASK_SEIP | MASK_MEIP;
/// `mip` 中由机器模式软件直接写入的中断位。
pub const MASK_MIP_WRITABLE: u64 = MASK_SSIP | MASK_SEIP;
/// 可委托到监督模式的中断位。
pub const MASK_MIDELEG: u64 = MASK_SSIP | MASK_STIP | MASK_SEIP;
/// 可委托到监督模式的同步异常位。
pub const MASK_MEDELEG: u64 = (1 << 0)
    | (1 << 1)
    | (1 << 2)
    | (1 << 3)
    | (1 << 4)
    | (1 << 5)
    | (1 << 6)
    | (1 << 7)
    | (1 << 8)
    | (1 << 9)
    | (1 << 12)
    | (1 << 13)
    | (1 << 15);
/// `menvcfg.STCE`：允许监督模式直接使用 `stimecmp`。
pub const MASK_STCE: u64 = 1 << 63;
/// `mcounteren/scounteren.TM`：允许较低特权级读取时间计数器。
pub const MASK_COUNTEREN_TM: u64 = 1 << 1;

const MASK_MSTATUS_WRITABLE: u64 = MASK_SIE
    | MASK_MIE
    | MASK_SPIE
    | MASK_MPIE
    | MASK_SPP
    | MASK_MPP
    | MASK_MPRV
    | MASK_SUM
    | MASK_MXR
    | MASK_TVM
    | MASK_TW
    | MASK_TSR;
const MASK_SSTATUS_WRITABLE: u64 = MASK_SIE | MASK_SPIE | MASK_SPP | MASK_SUM | MASK_MXR;
const MSTATUS_XLEN: u64 = (0b10 << 32) | (0b10 << 34);
const PMPADDR_MASK: u64 = (1 << 54) - 1;
const PMP_CONFIGS_PER_CSR: usize = 8;
const PMP_CFG_READ: u8 = 1 << 0;
const PMP_CFG_WRITE: u8 = 1 << 1;
const PMP_CFG_EXECUTE: u8 = 1 << 2;
const PMP_CFG_LOCKED: u8 = 1 << 7;
const PMP_CFG_ADDRESS_MASK: u8 = 0b11 << 3;
const PMP_CFG_TOR: u8 = 0b01 << 3;
const PMP_CFG_WRITABLE_MASK: u8 =
    PMP_CFG_READ | PMP_CFG_WRITE | PMP_CFG_EXECUTE | PMP_CFG_ADDRESS_MASK | PMP_CFG_LOCKED;

const NUM_CSRS: usize = 4096;

/// CSR 文件及监督模式别名映射。
pub struct Csr {
    csrs: [u64; NUM_CSRS],
    hardware_pending: u64,
}
impl Csr {
    /// 创建符合 RV64 复位约束的 CSR 文件。
    pub fn new() -> Csr {
        let mut csrs = [0; NUM_CSRS];
        csrs[MSTATUS] = MSTATUS_XLEN;
        csrs[STIMECMP] = u64::MAX;
        Self {
            csrs,
            hardware_pending: 0,
        }
    }

    /// 读取 CSR；监督模式别名只暴露委托或允许可见的位。
    pub fn load(&self, addr: usize) -> u64 {
        match addr {
            // SIE/SIP 只能看到 MIDELEG 委托给监督模式的中断位。
            SIE => self.csrs[MIE] & self.csrs[MIDELEG],
            SIP => (self.csrs[MIP] | self.hardware_pending) & self.csrs[MIDELEG],
            // SSTATUS 不是独立存储，只是 MSTATUS 的受限视图。
            SSTATUS => self.csrs[MSTATUS] & MASK_SSTATUS,
            MISA | MVENDORID | MARCHID | MIMPID | MHARTID | MCONFIGPTR => 0,
            MIP => self.csrs[MIP] | self.hardware_pending,
            _ => self.csrs[addr],
        }
    }

    /// 写入 CSR；监督模式别名会合成到底层机器级寄存器。
    pub fn store(&mut self, addr: usize, value: u64) {
        match addr {
            MSTATUS => self.csrs[MSTATUS] = sanitize_mstatus(value),
            MISA | MVENDORID | MARCHID | MIMPID | MHARTID | MCONFIGPTR => {}
            MEDELEG => self.csrs[MEDELEG] = value & MASK_MEDELEG,
            MIDELEG => self.csrs[MIDELEG] = value & MASK_MIDELEG,
            MIE => self.csrs[MIE] = value & MASK_INTERRUPTS,
            MTVEC | STVEC => self.csrs[addr] = sanitize_tvec(value),
            MCOUNTEREN | SCOUNTEREN => self.csrs[addr] = value & MASK_COUNTEREN_TM,
            MENVCFG => self.csrs[MENVCFG] = value & MASK_STCE,
            MEPC | SEPC => self.csrs[addr] = value & !1,
            MIP => {
                self.csrs[MIP] = (self.csrs[MIP] & !MASK_MIP_WRITABLE) | (value & MASK_MIP_WRITABLE)
            }
            SIE => {
                self.csrs[MIE] =
                    (self.csrs[MIE] & !self.csrs[MIDELEG]) | (value & self.csrs[MIDELEG])
            }
            SIP => {
                let writable = self.csrs[MIDELEG] & MASK_SSIP;
                self.csrs[MIP] = (self.csrs[MIP] & !writable) | (value & writable)
            }
            SSTATUS => {
                let mstatus =
                    (self.csrs[MSTATUS] & !MASK_SSTATUS_WRITABLE) | (value & MASK_SSTATUS_WRITABLE);
                self.csrs[MSTATUS] = sanitize_mstatus(mstatus)
            }
            SATP if value >> 60 == 0 || value >> 60 == 8 => self.csrs[SATP] = value,
            SATP => {}
            PMPCFG0 | PMPCFG2 => self.store_pmpcfg(addr, value),
            addr if (PMPADDR0..PMPADDR0 + PMP_ENTRIES).contains(&addr) => {
                self.store_pmpaddr(addr, value)
            }
            _ => self.csrs[addr] = value,
        }
    }

    /// 当前模拟器实现了该 CSR 地址。
    pub const fn is_implemented(addr: usize) -> bool {
        (addr >= PMPADDR0 && addr < PMPADDR0 + PMP_ENTRIES)
            || matches!(
                addr,
                MSTATUS
                    | MISA
                    | MEDELEG
                    | MIDELEG
                    | MIE
                    | MTVEC
                    | MCOUNTEREN
                    | MENVCFG
                    | MSCRATCH
                    | MEPC
                    | MCAUSE
                    | MTVAL
                    | MIP
                    | PMPCFG0
                    | PMPCFG2
                    | MVENDORID
                    | MARCHID
                    | MIMPID
                    | MHARTID
                    | MCONFIGPTR
                    | SSTATUS
                    | SIE
                    | STVEC
                    | SCOUNTEREN
                    | SSCRATCH
                    | SEPC
                    | SCAUSE
                    | STVAL
                    | SIP
                    | SATP
                    | STIMECMP
                    | TIME
            )
    }

    /// CSR 地址编码是否将该寄存器声明为只读。
    pub const fn is_read_only(addr: usize) -> bool {
        (addr >> 10) & 0b11 == 0b11
    }

    /// 更新由硬件信号驱动的中断待处理位。
    pub fn update_pending(&mut self, pending: u64) {
        self.hardware_pending = pending & MASK_INTERRUPTS;
    }

    /// 读取 CSR 读改写操作使用的软件状态，不把外部中断信号写回 `mip/sip`。
    pub(crate) fn load_for_write(&self, addr: usize) -> u64 {
        match addr {
            MIP => self.csrs[MIP],
            SIP => self.csrs[MIP] & self.csrs[MIDELEG],
            _ => self.load(addr),
        }
    }

    /// 读取一项 PMP 配置。
    pub(crate) fn pmp_config(&self, index: usize) -> u8 {
        debug_assert!(index < PMP_ENTRIES);
        let (addr, offset) = pmpcfg_location(index);
        (self.csrs[addr] >> offset) as u8
    }

    /// 读取一项 PMP 地址。
    pub(crate) fn pmp_address(&self, index: usize) -> u64 {
        debug_assert!(index < PMP_ENTRIES);
        self.csrs[PMPADDR0 + index]
    }

    /// 打印所有非零 CSR，供完整调试跟踪使用。
    pub fn dump_csr(&self) {
        for (i, &stored) in self.csrs.iter().enumerate() {
            let value = if i == MIP { self.load(MIP) } else { stored };
            if value != 0 {
                println!("CSR[0x{:03x}] = 0x{:016x}", i, value);
            }
        }
    }

    fn store_pmpcfg(&mut self, addr: usize, value: u64) {
        let mut stored = self.csrs[addr];
        for offset in 0..PMP_CONFIGS_PER_CSR {
            let shift = offset * 8;
            let old = (stored >> shift) as u8;
            if old & PMP_CFG_LOCKED != 0 {
                continue;
            }
            let entry = sanitize_pmp_config((value >> shift) as u8);
            stored = (stored & !(0xff << shift)) | (u64::from(entry) << shift);
        }
        self.csrs[addr] = stored;
    }

    fn store_pmpaddr(&mut self, addr: usize, value: u64) {
        let index = addr - PMPADDR0;
        let own_locked = self.pmp_config(index) & PMP_CFG_LOCKED != 0;
        let locked_tor_successor = index + 1 < PMP_ENTRIES
            && self.pmp_config(index + 1) & (PMP_CFG_LOCKED | PMP_CFG_ADDRESS_MASK)
                == PMP_CFG_LOCKED | PMP_CFG_TOR;
        if !own_locked && !locked_tor_successor {
            self.csrs[addr] = value & PMPADDR_MASK;
        }
    }
}

fn sanitize_mstatus(value: u64) -> u64 {
    let mut value = value & MASK_MSTATUS_WRITABLE;
    if value & MASK_MPP == 0b10 << 11 {
        value &= !MASK_MPP;
    }
    value | MSTATUS_XLEN
}

fn sanitize_tvec(value: u64) -> u64 {
    match value & 0b11 {
        0 | 1 => value,
        _ => value & !0b11,
    }
}

fn sanitize_pmp_config(value: u8) -> u8 {
    let mut entry = value & PMP_CFG_WRITABLE_MASK;
    if entry & (PMP_CFG_READ | PMP_CFG_WRITE) == PMP_CFG_WRITE {
        entry &= !PMP_CFG_WRITE;
    }
    entry
}

fn pmpcfg_location(index: usize) -> (usize, usize) {
    if index < PMP_CONFIGS_PER_CSR {
        (PMPCFG0, index * 8)
    } else {
        (PMPCFG2, (index - PMP_CONFIGS_PER_CSR) * 8)
    }
}

impl Default for Csr {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sip_write_preserves_non_delegated_pending_bits() {
        let mut csr = Csr::new();
        csr.store(MIDELEG, MASK_SSIP);
        csr.store(MIP, MASK_SSIP);
        csr.update_pending(MASK_MEIP);
        csr.store(MIE, MASK_MSIP);

        csr.store(SIP, 0);

        assert_eq!(csr.load(MIP), MASK_MEIP);
        assert_eq!(csr.load(SIP), 0);
    }

    #[test]
    fn sip_write_updates_only_delegated_pending_bits() {
        let mut csr = Csr::new();
        csr.store(MIDELEG, MASK_SSIP);
        csr.update_pending(MASK_MEIP);

        csr.store(SIP, MASK_SSIP | MASK_SEIP);

        assert_eq!(csr.load(MIP), MASK_MEIP | MASK_SSIP);
        assert_eq!(csr.load(SIP), MASK_SSIP);
    }

    #[test]
    fn sie_write_preserves_non_delegated_enable_bits() {
        let mut csr = Csr::new();
        csr.store(MIDELEG, MASK_SSIP);
        csr.store(MIE, MASK_SSIP | MASK_STIP | MASK_MEIP);

        csr.store(SIE, 0);

        assert_eq!(csr.load(MIE), MASK_STIP | MASK_MEIP);
        assert_eq!(csr.load(SIE), 0);
    }

    #[test]
    fn hardware_pending_bits_do_not_overwrite_software_state() {
        let mut csr = Csr::new();
        csr.store(MIP, MASK_MEIP | MASK_MTIP | MASK_MSIP);
        assert_eq!(csr.load(MIP), 0);

        csr.store(MIP, MASK_SSIP);
        csr.update_pending(MASK_SEIP);
        assert_eq!(csr.load(MIP), MASK_SSIP | MASK_SEIP);

        csr.update_pending(0);
        assert_eq!(csr.load(MIP), MASK_SSIP);
    }

    #[test]
    fn warl_fields_reject_unsupported_encodings() {
        let mut csr = Csr::new();
        for addr in [MVENDORID, MARCHID, MIMPID, MHARTID, MCONFIGPTR] {
            assert!(Csr::is_implemented(addr));
            assert!(Csr::is_read_only(addr));
            assert_eq!(csr.load(addr), 0);
        }

        csr.store(MSTATUS, 0b10 << 11);
        assert_eq!(csr.load(MSTATUS) & MASK_MPP, 0);
        assert_eq!(csr.load(MSTATUS) & (MASK_UXL | MASK_SXL), MSTATUS_XLEN);

        csr.store(MTVEC, 0x1003);
        assert_eq!(csr.load(MTVEC), 0x1000);
        csr.store(SATP, (8 << 60) | 7);
        csr.store(SATP, (9 << 60) | 9);
        assert_eq!(csr.load(SATP), (8 << 60) | 7);
    }

    #[test]
    fn pmp_configuration_covers_sixteen_entries_and_honors_locks() {
        let mut csr = Csr::new();
        csr.store(PMPCFG0, 0x0f | (0x0a << 8));
        assert_eq!(csr.pmp_config(0), 0x0f);
        assert_eq!(csr.pmp_config(1), 0x08);

        csr.store(PMPADDR0 + 7, 0x123);
        csr.store(PMPADDR0 + 8, 0x456);
        csr.store(PMPCFG2, 0x1f);
        assert_eq!(csr.pmp_address(7), 0x123);
        assert_eq!(csr.pmp_address(8), 0x456);
        assert_eq!(csr.pmp_config(8), 0x1f);

        csr.store(PMPADDR0, 0x100);
        csr.store(PMPADDR0 + 1, 0x200);
        csr.store(PMPCFG0, 0x0f | (0x88 << 8));
        csr.store(PMPADDR0, 0x300);
        csr.store(PMPADDR0 + 1, 0x400);
        csr.store(PMPCFG0, 0);
        assert_eq!(csr.pmp_address(0), 0x100);
        assert_eq!(csr.pmp_address(1), 0x200);
        assert_eq!(csr.pmp_config(1), 0x88);
    }
}
