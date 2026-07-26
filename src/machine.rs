//! CPU 与平台地址空间的组装层。
//!
//! [`Platform`] 在 CPU 创建前持有 DRAM 和待挂载设备，统一检查物理区域并完成总线组装。
//! [`Machine`] 作为唯一运行入口，按同一平台周期推进设备与 CPU，并统一处理复位和连续执行。

use crate::bus::{Bus, MemDevice};
use crate::cpu::{CYCLES_PER_STEP, Cpu};
use crate::dram::Dram;
use crate::trap::Exception;
use crate::uart::Uart;
use std::error::Error;
use std::fmt::{Display, Formatter};

pub use crate::cpu::DebugLevel;

const RESET_VECTOR_ALIGNMENT: u64 = 2;
const STACK_ALIGNMENT: u64 = 16;

/// [`Machine::run`] 的停止条件和调试配置。
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
pub struct RunOptions {
    /// 最多成功执行的步数；`None` 表示不设上限。
    pub max_steps: Option<u64>,
    /// 每步执行前输出的状态详细程度。
    pub debug: DebugLevel,
}

/// [`Machine::run`] 离开执行循环的原因。
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

struct MappedDevice {
    base: u64,
    size: u64,
    dev: Box<dyn MemDevice>,
}

/// 平台布局或设备映射不满足组装约束。
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum PlatformError {
    /// DRAM 容量必须大于零。
    EmptyDram,
    /// MMIO 设备区域必须大于零。
    EmptyDeviceRegion,
    /// 地址区间的末端超出 `u64`。
    AddressOverflow,
    /// 新设备与 DRAM 或已有设备区域重叠。
    RegionOverlap { base: u64, end: u64 },
    /// CPU 复位向量不位于 DRAM 中。
    ResetVectorOutsideDram { reset_vector: u64 },
    /// CPU 复位向量不满足当前 16 位指令对齐要求。
    ResetVectorMisaligned { reset_vector: u64 },
}

impl Display for PlatformError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyDram => f.write_str("DRAM size must be non-zero"),
            Self::EmptyDeviceRegion => f.write_str("device region size must be non-zero"),
            Self::AddressOverflow => f.write_str("platform address range overflows u64"),
            Self::RegionOverlap { base, end } => {
                write!(
                    f,
                    "device range {base:#x}..{end:#x} overlaps another region"
                )
            }
            Self::ResetVectorOutsideDram { reset_vector } => {
                write!(f, "reset vector {reset_vector:#x} is outside DRAM")
            }
            Self::ResetVectorMisaligned { reset_vector } => {
                write!(f, "reset vector {reset_vector:#x} is not 2-byte aligned")
            }
        }
    }
}

impl Error for PlatformError {}

/// 尚未创建 CPU 的平台地址空间。
///
/// 镜像应通过 [`Platform::dram_mut`] 装载；设备必须在 [`Platform::build`] 前完成挂载。
pub struct Platform {
    dram: Dram,
    dram_end: u64,
    devices: Vec<MappedDevice>,
}

impl Platform {
    /// 创建只含 DRAM 的平台。
    pub fn new(dram_base: u64, dram_size: usize) -> Result<Self, PlatformError> {
        let size = u64::try_from(dram_size).map_err(|_| PlatformError::AddressOverflow)?;
        if size == 0 {
            return Err(PlatformError::EmptyDram);
        }
        let dram_end = dram_base
            .checked_add(size)
            .ok_or(PlatformError::AddressOverflow)?;
        Ok(Self {
            dram: Dram::with_layout(dram_base, dram_size),
            dram_end,
            devices: Vec::new(),
        })
    }

    /// 返回平台主存，供镜像装载器读取布局。
    pub fn dram(&self) -> &Dram {
        &self.dram
    }

    /// 返回平台主存的可变引用；CPU 创建后应改由总线访问内存。
    pub fn dram_mut(&mut self) -> &mut Dram {
        &mut self.dram
    }

    /// 挂载一个通用 MMIO 设备。
    ///
    /// 构建后，设备会自动参与复位、时钟推进和 CPU 中断轮询。
    pub fn attach_device(
        &mut self,
        base: u64,
        size: u64,
        dev: Box<dyn MemDevice>,
    ) -> Result<(), PlatformError> {
        if size == 0 {
            return Err(PlatformError::EmptyDeviceRegion);
        }
        let end = base
            .checked_add(size)
            .ok_or(PlatformError::AddressOverflow)?;
        let overlaps_dram = ranges_overlap(base, end, self.dram.base, self.dram_end);
        let overlaps_device = self
            .devices
            .iter()
            .any(|region| ranges_overlap(base, end, region.base, region.base + region.size));
        if overlaps_dram || overlaps_device {
            return Err(PlatformError::RegionOverlap { base, end });
        }
        self.devices.push(MappedDevice { base, size, dev });
        Ok(())
    }

    /// 挂载正式库中的简化 UART。
    pub fn attach_uart(&mut self, base: u64) -> Result<(), PlatformError> {
        self.attach_device(base, 0x100, Box::new(Uart::new(base)))
    }

    /// 验证复位向量并完成总线、CPU、时钟状态和中断源的组装。
    pub fn build(self, reset_vector: u64) -> Result<Machine, PlatformError> {
        if !(self.dram.base..self.dram_end).contains(&reset_vector) {
            return Err(PlatformError::ResetVectorOutsideDram { reset_vector });
        }
        if reset_vector & (RESET_VECTOR_ALIGNMENT - 1) != 0 {
            return Err(PlatformError::ResetVectorMisaligned { reset_vector });
        }

        let dram_base = self.dram.base;
        let dram_size = self.dram_end - dram_base;
        // RISC-V psABI 要求栈指针保持 16 字节对齐；向下取整可确保栈顶不越过 DRAM 末端。
        let initial_sp = self.dram_end & !(STACK_ALIGNMENT - 1);
        let mut bus = Bus::new();
        bus.attach_device(dram_base, dram_size, Box::new(self.dram));
        for region in self.devices {
            bus.attach_device(region.base, region.size, region.dev);
        }

        Ok(Machine::from_address_space(
            Box::new(bus),
            reset_vector,
            initial_sp,
        ))
    }
}

/// 已组装地址空间、可以执行 guest 的机器容器。
pub struct Machine {
    /// 可观察和配置的 CPU 状态；复位与执行必须通过 [`Machine`] 的方法完成。
    pub cpu: Cpu,
}

impl Machine {
    /// 用一个已经实现完整物理地址空间的设备创建机器。
    ///
    /// 该入口用于测试平台等自行实现地址分发、中断控制和生命周期协议的场景；常规平台应
    /// 优先使用 [`Platform::build`] 以获得区域冲突检查。
    pub fn from_address_space(
        address_space: Box<dyn MemDevice>,
        reset_vector: u64,
        initial_sp: u64,
    ) -> Self {
        Self {
            cpu: Cpu::with_reset_vector(address_space, reset_vector, initial_sp),
        }
    }

    /// 恢复 CPU 与所有支持复位协议的总线设备。
    ///
    /// RAM 等没有易失控制状态的设备可以沿用 [`MemDevice::reset`] 的默认空实现。
    pub fn reset(&mut self) {
        self.cpu.bus.reset();
        self.cpu.reset();
    }

    /// 推进一个机器步骤。
    ///
    /// 设备先观察本步经过的平台周期，再由 CPU 更新时间、查询中断并执行或进入 trap。
    pub fn step(&mut self) -> Result<(), Exception> {
        self.cpu.bus.tick(CYCLES_PER_STEP);
        self.cpu.step()
    }

    /// 重复调用 [`Machine::step`]，直到达到步数上限或出现未处理异常。
    pub fn run(&mut self, options: RunOptions) -> RunOutcome {
        let mut steps = 0;
        loop {
            if options.max_steps.is_some_and(|limit| steps >= limit) {
                return RunOutcome::StepLimitReached { steps };
            }

            match options.debug {
                DebugLevel::Off => {}
                DebugLevel::Pc => self.cpu.dump_pc(),
                DebugLevel::Full => {
                    self.cpu.dump_pc();
                    self.cpu.dump_registers();
                    self.cpu.csr.dump_csr();
                }
            }

            let pc = self.cpu.pc;
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
}

fn ranges_overlap(lhs_start: u64, lhs_end: u64, rhs_start: u64, rhs_end: u64) -> bool {
    lhs_start < rhs_end && rhs_start < lhs_end
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    struct InterruptSource;

    impl MemDevice for InterruptSource {
        fn read(&mut self, addr: u64, _size: usize) -> Result<u64, Exception> {
            Err(Exception::LoadAccessFault(addr))
        }

        fn write(&mut self, addr: u64, _value: u64, _size: usize) -> Result<(), Exception> {
            Err(Exception::StoreAMOAccessFault(addr))
        }

        fn pending_interrupt(&mut self) -> Option<u64> {
            Some((1 << 63) | 9)
        }
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct LifecycleState {
        resets: u64,
        cycles: u64,
    }

    struct LifecycleDevice(Rc<RefCell<LifecycleState>>);

    impl MemDevice for LifecycleDevice {
        fn read(&mut self, addr: u64, _size: usize) -> Result<u64, Exception> {
            Err(Exception::LoadAccessFault(addr))
        }

        fn write(&mut self, addr: u64, _value: u64, _size: usize) -> Result<(), Exception> {
            Err(Exception::StoreAMOAccessFault(addr))
        }

        fn pending_interrupt(&mut self) -> Option<u64> {
            (self.0.borrow().cycles > 0).then_some((1 << 63) | 9)
        }

        fn reset(&mut self) {
            self.0.borrow_mut().resets += 1;
        }

        fn tick(&mut self, cycles: u64) {
            self.0.borrow_mut().cycles += cycles;
        }
    }

    #[test]
    fn machine_run_advances_cpu_and_device_clocks_and_reset_reaches_devices() {
        let base = 0x8000_0000;
        let mut platform = Platform::new(base, 16).unwrap();
        let lifecycle = Rc::new(RefCell::new(LifecycleState::default()));
        platform
            .attach_device(
                0x1000_0000,
                0x100,
                Box::new(LifecycleDevice(Rc::clone(&lifecycle))),
            )
            .unwrap();
        platform
            .dram_mut()
            .load_bytes(base, &[0x93, 0x0f, 0xa0, 0x02])
            .unwrap();
        let mut machine = platform.build(base).unwrap();
        machine.reset();

        let outcome = machine.run(RunOptions {
            max_steps: Some(1),
            debug: DebugLevel::Off,
        });

        assert!(matches!(outcome, RunOutcome::StepLimitReached { steps: 1 }));
        assert_eq!(machine.cpu.registers[31], 42);
        assert_eq!(machine.cpu.cycles, CYCLES_PER_STEP);
        assert_ne!(
            machine.cpu.csr.load(crate::csr::MIP) & crate::csr::MASK_SEIP,
            0
        );
        assert_eq!(
            *lifecycle.borrow(),
            LifecycleState {
                resets: 1,
                cycles: CYCLES_PER_STEP,
            }
        );
    }

    #[test]
    fn platform_rejects_invalid_regions_and_reset_vectors() {
        assert!(matches!(
            Platform::new(0x8000_0000, 0),
            Err(PlatformError::EmptyDram)
        ));
        assert!(matches!(
            Platform::new(u64::MAX, 2),
            Err(PlatformError::AddressOverflow)
        ));

        let mut platform = Platform::new(0x8000_0000, 0x1000).unwrap();
        assert!(matches!(
            platform.attach_device(0x1000_0000, 0, Box::new(InterruptSource)),
            Err(PlatformError::EmptyDeviceRegion)
        ));
        assert!(matches!(
            platform.attach_device(u64::MAX, 2, Box::new(InterruptSource)),
            Err(PlatformError::AddressOverflow)
        ));
        assert!(matches!(
            platform.attach_device(0x8000_0800, 0x100, Box::new(InterruptSource)),
            Err(PlatformError::RegionOverlap { .. })
        ));
        platform
            .attach_device(0x1000_0000, 0x100, Box::new(InterruptSource))
            .unwrap();
        assert!(matches!(
            platform.attach_device(0x1000_0080, 0x100, Box::new(InterruptSource)),
            Err(PlatformError::RegionOverlap { .. })
        ));
        assert!(matches!(
            platform.build(0x9000_0000),
            Err(PlatformError::ResetVectorOutsideDram { .. })
        ));

        let platform = Platform::new(0x8000_0000, 0x1000).unwrap();
        assert!(matches!(
            platform.build(0x8000_0001),
            Err(PlatformError::ResetVectorMisaligned { .. })
        ));
    }

    #[test]
    fn platform_aligns_the_initial_stack_pointer_down_without_crossing_dram_end() {
        let base = 0x8000_0000;
        let machine = Platform::new(base, 0x23).unwrap().build(base).unwrap();

        assert_eq!(machine.cpu.registers[2], base + 0x20);
        assert_eq!(machine.cpu.registers[2] & 0xf, 0);
    }

    #[test]
    fn mapped_interrupt_sources_reach_the_cpu_bus() {
        let mut platform = Platform::new(0x8000_0000, 0x1000).unwrap();
        platform
            .attach_device(0x1000_0000, 0x100, Box::new(InterruptSource))
            .unwrap();
        let mut machine = platform.build(0x8000_0000).unwrap();

        assert_eq!(machine.cpu.bus.pending_interrupt(), Some((1 << 63) | 9));
    }
}
