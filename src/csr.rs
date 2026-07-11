//! 控制与状态寄存器（CSR）存储。
//!
//! [`Csr`] 用 4096 项数组覆盖 12 位 CSR 地址空间，并为 `SSTATUS`、`SIE`、`SIP`
//! 提供机器级寄存器的监督模式视图。除这些映射外，当前实现直接保存调用方写入的值。

/// hart 标识寄存器。
pub const MHARTID: usize = 0xf14;
/// 机器模式状态寄存器。
pub const MSTATUS: usize = 0x300;
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

// 监督模式 CSR。
/// 监督模式状态寄存器，是 `MSTATUS` 中监督模式可见位的视图。
pub const SSTATUS: usize = 0x100;
/// 监督模式中断使能寄存器。
pub const SIE: usize = 0x104;
/// 监督模式 trap 入口寄存器。
pub const STVEC: usize = 0x105;
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

const NUM_CSRS: usize = 4096;

/// CSR 文件及监督模式别名映射。
pub struct Csr {
    csrs: [u64; NUM_CSRS],
}
impl Csr {
    /// 创建全部清零的 CSR 文件。
    pub fn new() -> Csr {
        Self {
            csrs: [0; NUM_CSRS],
        }
    }

    /// 读取 CSR；监督模式别名只暴露委托或允许可见的位。
    pub fn load(&self, addr: usize) -> u64 {
        match addr {
            // SIE/SIP 只能看到 MIDELEG 委托给监督模式的中断位。
            SIE => self.csrs[MIE] & self.csrs[MIDELEG],
            SIP => self.csrs[MIP] & self.csrs[MIDELEG],
            // SSTATUS 不是独立存储，只是 MSTATUS 的受限视图。
            SSTATUS => self.csrs[MSTATUS] & MASK_SSTATUS,
            _ => self.csrs[addr],
        }
    }

    /// 写入 CSR；监督模式别名按当前简化映射合成到底层机器级寄存器。
    pub fn store(&mut self, addr: usize, value: u64) {
        match addr {
            SIE => {
                self.csrs[MIE] =
                    (self.csrs[MIE] & !self.csrs[MIDELEG]) | (value & self.csrs[MIDELEG])
            }
            SIP => {
                self.csrs[MIP] =
                    (self.csrs[MIP] & !self.csrs[MIDELEG]) | (value & self.csrs[MIDELEG])
            }
            SSTATUS => {
                self.csrs[MSTATUS] = (self.csrs[MSTATUS] & !MASK_SSTATUS) | (value & MASK_SSTATUS)
            }
            _ => self.csrs[addr] = value,
        }
    }

    /// 打印所有非零 CSR，供完整调试跟踪使用。
    pub fn dump_csr(&self) {
        for (i, &value) in self.csrs.iter().enumerate() {
            if value != 0 {
                println!("CSR[0x{:03x}] = 0x{:016x}", i, value);
            }
        }
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
        csr.store(MIP, MASK_MEIP | MASK_SSIP);
        csr.store(MIE, MASK_MSIP);

        csr.store(SIP, 0);

        assert_eq!(csr.load(MIP), MASK_MEIP);
        assert_eq!(csr.load(SIP), 0);
    }

    #[test]
    fn sip_write_updates_only_delegated_pending_bits() {
        let mut csr = Csr::new();
        csr.store(MIDELEG, MASK_SSIP);
        csr.store(MIP, MASK_MEIP);

        csr.store(SIP, MASK_SSIP | MASK_SEIP);

        assert_eq!(csr.load(MIP), MASK_MEIP | MASK_SSIP);
        assert_eq!(csr.load(SIP), MASK_SSIP);
    }
}
