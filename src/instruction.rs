//! RISC-V 指令字段译码和执行。
//!
//! 模块先把 32 位取指窗口拆成 [`Instruction`] 的公共字段，再按 opcode 分派执行。
//! 低两位不是 `0b11` 时改走 16 位压缩指令路径。执行器直接更新 [`Cpu`] 的寄存器、PC、CSR 和总线状态，
//! 并在每条指令结束时恢复零号寄存器约束。

use crate::{
    cpu::{Cpu, MemoryAccess, PrivilegeMode},
    csr,
    trap::Exception,
};

/// 从 32 位取指窗口提取出的通用指令字段。
///
/// `raw` 必须保留原始编码，因为立即数和部分子操作需要跨字段重新拼接位段。
pub struct Instruction {
    pub opcode: u8,
    pub rd: u8,
    pub funct3: u8,
    pub rs1: u8,
    pub rs2: u8,
    pub funct7: u8,
    pub raw: u32,
}

/// 提取 opcode、寄存器编号和功能字段，不在这一阶段判断组合是否合法。
pub fn decode(instruction: u32) -> Instruction {
    let opcode = (instruction & 0x7f) as u8;
    let rd = ((instruction >> 7) & 0x1f) as u8;
    let funct3 = ((instruction >> 12) & 0x07) as u8;
    let rs1 = ((instruction >> 15) & 0x1f) as u8;
    let rs2 = ((instruction >> 20) & 0x1f) as u8;
    let funct7 = ((instruction >> 25) & 0x7f) as u8;

    Instruction {
        opcode,
        rd,
        funct3,
        rs1,
        rs2,
        funct7,
        raw: instruction,
    }
}

/// 执行一条已译码指令。
///
/// 32 位和压缩指令共享此入口；不支持的编码返回 [`Exception::IllegalInstruction`]。
pub fn execute(cpu: &mut Cpu, inst: Instruction) -> Result<(), Exception> {
    // RISC-V 以低两位区分 16 位压缩编码与普通 32 位编码。
    if inst.raw & 0b11 != 0b11 {
        return execute_compressed(cpu, inst.raw as u16);
    }

    let result = match inst.opcode {
        0x03 => execute_load(cpu, &inst),
        0x0f => match inst.funct3 {
            // FENCE/FENCE.I 在单硬件线程模型中保守地视为空操作。
            0x0 | 0x1 => Ok(()),
            _ => Err(Exception::IllegalInstruction(inst.raw as u64)),
        },
        0x13 => execute_op_imm(cpu, &inst),
        0x17 => {
            write_reg(cpu, inst.rd, cpu.pc.wrapping_add(imm_u(inst.raw)));
            Ok(())
        }
        0x1b => execute_op_imm_32(cpu, &inst),
        0x23 => execute_store(cpu, &inst),
        0x2f => execute_amo(cpu, &inst),
        0x33 => execute_op(cpu, &inst),
        0x37 => {
            write_reg(cpu, inst.rd, imm_u(inst.raw));
            Ok(())
        }
        0x3b => execute_op_32(cpu, &inst),
        0x63 => execute_branch(cpu, &inst),
        0x67 => execute_jalr(cpu, &inst),
        0x6f => execute_jal(cpu, &inst),
        0x73 => execute_system(cpu, &inst),
        _ => Err(Exception::IllegalInstruction(inst.raw as u64)),
    };

    finish(cpu, result)
}

fn execute_load(cpu: &mut Cpu, inst: &Instruction) -> Result<(), Exception> {
    let size = match inst.funct3 {
        0x0 | 0x4 => 1,
        0x1 | 0x5 => 2,
        0x2 | 0x6 => 4,
        0x3 => 8,
        _ => return Err(Exception::IllegalInstruction(inst.raw as u64)),
    };
    // 有效地址先按 XLEN 环绕相加，再由 MMU 决定物理地址和读取权限。
    let virtual_addr = reg(cpu, inst.rs1).wrapping_add(imm_i(inst.raw));
    require_load_alignment(virtual_addr, size)?;
    let addr = cpu.translate_sized(virtual_addr, MemoryAccess::Load, size)?;
    let value = read_load(cpu, addr, virtual_addr, size)?;
    let value = if inst.funct3 < 0x4 {
        sign_extend(value, (size * 8) as u32)
    } else {
        value
    };
    write_reg(cpu, inst.rd, value);
    Ok(())
}

fn execute_store(cpu: &mut Cpu, inst: &Instruction) -> Result<(), Exception> {
    let size = match inst.funct3 {
        0x0 => 1,
        0x1 => 2,
        0x2 => 4,
        0x3 => 8,
        _ => return Err(Exception::IllegalInstruction(inst.raw as u64)),
    };
    let virtual_addr = reg(cpu, inst.rs1).wrapping_add(imm_s(inst.raw));
    require_store_alignment(virtual_addr, size)?;
    let addr = cpu.translate_sized(virtual_addr, MemoryAccess::Store, size)?;
    let value = reg(cpu, inst.rs2);
    write_mem(cpu, addr, virtual_addr, value, size)
}

fn execute_op_imm(cpu: &mut Cpu, inst: &Instruction) -> Result<(), Exception> {
    let lhs = reg(cpu, inst.rs1);
    let imm = imm_i(inst.raw);
    let value = match inst.funct3 {
        0x0 => lhs.wrapping_add(imm),
        0x2 => (signed(lhs) < signed(imm)) as u64,
        0x3 => (lhs < imm) as u64,
        0x4 => lhs ^ imm,
        0x6 => lhs | imm,
        0x7 => lhs & imm,
        0x1 if inst.raw >> 26 == 0x00 => lhs.wrapping_shl(shamt64(inst.raw)),
        0x5 if inst.raw >> 26 == 0x00 => lhs.wrapping_shr(shamt64(inst.raw)),
        0x5 if inst.raw >> 26 == 0x10 => (signed(lhs) >> shamt64(inst.raw)) as u64,
        _ => return Err(Exception::IllegalInstruction(inst.raw as u64)),
    };
    write_reg(cpu, inst.rd, value);
    Ok(())
}

fn execute_op_imm_32(cpu: &mut Cpu, inst: &Instruction) -> Result<(), Exception> {
    let lhs = reg(cpu, inst.rs1);
    let value = match inst.funct3 {
        0x0 => sign_extend32(lhs.wrapping_add(imm_i(inst.raw)) as u32),
        0x1 if inst.funct7 == 0x00 => sign_extend32((lhs as u32).wrapping_shl(shamt32(inst.raw))),
        0x5 if inst.funct7 == 0x00 => sign_extend32((lhs as u32).wrapping_shr(shamt32(inst.raw))),
        0x5 if inst.funct7 == 0x20 => {
            sign_extend32(((lhs as u32 as i32) >> shamt32(inst.raw)) as u32)
        }
        _ => return Err(Exception::IllegalInstruction(inst.raw as u64)),
    };
    write_reg(cpu, inst.rd, value);
    Ok(())
}

fn execute_op(cpu: &mut Cpu, inst: &Instruction) -> Result<(), Exception> {
    let lhs = reg(cpu, inst.rs1);
    let rhs = reg(cpu, inst.rs2);
    let value = match (inst.funct7, inst.funct3) {
        (0x00, 0x0) => lhs.wrapping_add(rhs),
        (0x20, 0x0) => lhs.wrapping_sub(rhs),
        (0x00, 0x1) => lhs.wrapping_shl((rhs & 0x3f) as u32),
        (0x00, 0x2) => (signed(lhs) < signed(rhs)) as u64,
        (0x00, 0x3) => (lhs < rhs) as u64,
        (0x00, 0x4) => lhs ^ rhs,
        (0x00, 0x5) => lhs.wrapping_shr((rhs & 0x3f) as u32),
        (0x20, 0x5) => (signed(lhs) >> (rhs & 0x3f)) as u64,
        (0x00, 0x6) => lhs | rhs,
        (0x00, 0x7) => lhs & rhs,
        (0x01, _) => execute_mul_div(lhs, rhs, inst.funct3),
        _ => return Err(Exception::IllegalInstruction(inst.raw as u64)),
    };
    write_reg(cpu, inst.rd, value);
    Ok(())
}

fn execute_op_32(cpu: &mut Cpu, inst: &Instruction) -> Result<(), Exception> {
    let lhs = reg(cpu, inst.rs1);
    let rhs = reg(cpu, inst.rs2);
    let value = match (inst.funct7, inst.funct3) {
        (0x00, 0x0) => sign_extend32((lhs as u32).wrapping_add(rhs as u32)),
        (0x20, 0x0) => sign_extend32((lhs as u32).wrapping_sub(rhs as u32)),
        (0x00, 0x1) => sign_extend32((lhs as u32).wrapping_shl((rhs & 0x1f) as u32)),
        (0x00, 0x5) => sign_extend32((lhs as u32).wrapping_shr((rhs & 0x1f) as u32)),
        (0x20, 0x5) => sign_extend32(((lhs as u32 as i32) >> (rhs & 0x1f)) as u32),
        (0x01, 0x0 | 0x4 | 0x5 | 0x6 | 0x7) => execute_mul_div_32(lhs, rhs, inst.funct3),
        _ => return Err(Exception::IllegalInstruction(inst.raw as u64)),
    };
    write_reg(cpu, inst.rd, value);
    Ok(())
}

fn execute_branch(cpu: &mut Cpu, inst: &Instruction) -> Result<(), Exception> {
    let lhs = reg(cpu, inst.rs1);
    let rhs = reg(cpu, inst.rs2);
    let taken = match inst.funct3 {
        0x0 => lhs == rhs,
        0x1 => lhs != rhs,
        0x4 => signed(lhs) < signed(rhs),
        0x5 => signed(lhs) >= signed(rhs),
        0x6 => lhs < rhs,
        0x7 => lhs >= rhs,
        _ => return Err(Exception::IllegalInstruction(inst.raw as u64)),
    };

    if taken {
        if try_accelerate_memset_loop(cpu, inst)? {
            return Ok(());
        }
        cpu.write_pc(cpu.pc.wrapping_add(imm_b(inst.raw)));
    }
    Ok(())
}

fn execute_jal(cpu: &mut Cpu, inst: &Instruction) -> Result<(), Exception> {
    // 链接地址始终指向当前 32 位指令之后，目标地址则相对当前 PC 计算。
    let link = cpu.pc.wrapping_add(4);
    let target = cpu.pc.wrapping_add(imm_j(inst.raw));
    write_reg(cpu, inst.rd, link);
    cpu.write_pc(target);
    Ok(())
}

fn execute_jalr(cpu: &mut Cpu, inst: &Instruction) -> Result<(), Exception> {
    if inst.funct3 != 0x0 {
        return Err(Exception::IllegalInstruction(inst.raw as u64));
    }
    let link = cpu.pc.wrapping_add(4);
    // JALR 规定目标最低位清零；这不是通用的地址对齐修正。
    let target = reg(cpu, inst.rs1).wrapping_add(imm_i(inst.raw)) & !1;
    write_reg(cpu, inst.rd, link);
    cpu.write_pc(target);
    Ok(())
}

fn execute_system(cpu: &mut Cpu, inst: &Instruction) -> Result<(), Exception> {
    match inst.raw {
        0x0000_0073 => {
            return Err(match cpu.privilege {
                PrivilegeMode::User => Exception::EnvironmentCallFromUMode(cpu.pc),
                PrivilegeMode::Supervisor => Exception::EnvironmentCallFromSMode(cpu.pc),
                PrivilegeMode::Machine => Exception::EnvironmentCallFromMMode(cpu.pc),
            });
        }
        0x0010_0073 => return Err(Exception::Breakpoint(cpu.pc)),
        0x1020_0073 => {
            let illegal = cpu.privilege == PrivilegeMode::User
                || (cpu.privilege == PrivilegeMode::Supervisor
                    && cpu.csr.load(csr::MSTATUS) & csr::MASK_TSR != 0);
            if illegal {
                return Err(Exception::IllegalInstruction(inst.raw as u64));
            }
            cpu.supervisor_return();
            return Ok(());
        }
        0x1050_0073 => {
            let illegal = cpu.privilege == PrivilegeMode::User
                || (cpu.privilege == PrivilegeMode::Supervisor
                    && cpu.csr.load(csr::MSTATUS) & csr::MASK_TW != 0);
            return if illegal {
                Err(Exception::IllegalInstruction(inst.raw as u64))
            } else {
                Ok(())
            };
        }
        0x3020_0073 => {
            if cpu.privilege != PrivilegeMode::Machine {
                return Err(Exception::IllegalInstruction(inst.raw as u64));
            }
            cpu.machine_return();
            return Ok(());
        }
        _ => {}
    }

    if inst.raw & 0xfe00_7fff == 0x1200_0073 {
        let illegal = cpu.privilege == PrivilegeMode::User
            || (cpu.privilege == PrivilegeMode::Supervisor
                && cpu.csr.load(csr::MSTATUS) & csr::MASK_TVM != 0);
        return if illegal {
            Err(Exception::IllegalInstruction(inst.raw as u64))
        } else {
            Ok(())
        };
    }

    let csr_addr = ((inst.raw >> 20) & 0x0fff) as usize;
    let rs1_value = reg(cpu, inst.rs1);
    let uimm = inst.rs1 as u64;
    let writes = match inst.funct3 {
        0x1 | 0x5 => true,
        0x2 | 0x3 => inst.rs1 != 0,
        0x6 | 0x7 => uimm != 0,
        _ => return Err(Exception::IllegalInstruction(inst.raw as u64)),
    };
    if !cpu.csr_access_allowed(csr_addr, writes) {
        return Err(Exception::IllegalInstruction(inst.raw as u64));
    }

    // CSRRW/CSRRWI 在 rd=x0 时不读取 CSR；其余形式需要旧值作为结果或写入输入。
    let reads = inst.rd != 0 || !matches!(inst.funct3, 0x1 | 0x5);
    let old = if reads { cpu.csr.load(csr_addr) } else { 0 };
    let write_old = if matches!(inst.funct3, 0x2 | 0x3 | 0x6 | 0x7) {
        cpu.csr.load_for_write(csr_addr)
    } else {
        old
    };

    let value = match inst.funct3 {
        0x1 => rs1_value,
        0x2 => write_old | rs1_value,
        0x3 => write_old & !rs1_value,
        0x5 => uimm,
        0x6 => write_old | uimm,
        0x7 => write_old & !uimm,
        _ => unreachable!(),
    };
    if writes {
        cpu.csr.store(csr_addr, value);
    }
    if inst.rd != 0 {
        write_reg(cpu, inst.rd, old);
    }
    Ok(())
}

fn execute_amo(cpu: &mut Cpu, inst: &Instruction) -> Result<(), Exception> {
    let width = match inst.funct3 {
        0x2 => 4,
        0x3 => 8,
        _ => return Err(Exception::IllegalInstruction(inst.raw as u64)),
    };
    let funct5 = (inst.raw >> 27) & 0x1f;
    if !matches!(
        funct5,
        0x00 | 0x01 | 0x02 | 0x03 | 0x04 | 0x08 | 0x0c | 0x10 | 0x14 | 0x18 | 0x1c
    ) || (funct5 == 0x02 && inst.rs2 != 0)
    {
        return Err(Exception::IllegalInstruction(inst.raw as u64));
    }

    let virtual_addr = reg(cpu, inst.rs1);
    if funct5 == 0x02 {
        cpu.clear_reservation();
        require_load_alignment(virtual_addr, width)?;
        let addr = cpu.translate_sized(virtual_addr, MemoryAccess::Load, width)?;
        if !cpu.set_reservation(addr, width) {
            return Err(Exception::LoadAccessFault(virtual_addr));
        }
        let old_raw = cpu
            .bus
            .read(addr, width)
            .map_err(|_| Exception::LoadAccessFault(virtual_addr))?;
        let old = if width == 4 {
            sign_extend(old_raw, 32)
        } else {
            old_raw
        };
        write_reg(cpu, inst.rd, old);
        return Ok(());
    }

    if funct5 == 0x03 {
        let reservation = cpu.take_reservation();
        require_store_alignment(virtual_addr, width)?;
        let addr = cpu.translate_sized(virtual_addr, MemoryAccess::Store, width)?;
        if reservation != Some((addr, width)) {
            write_reg(cpu, inst.rd, 1);
            return Ok(());
        }
        write_mem(cpu, addr, virtual_addr, reg(cpu, inst.rs2), width)?;
        write_reg(cpu, inst.rd, 0);
        return Ok(());
    }

    cpu.clear_reservation();
    require_store_alignment(virtual_addr, width)?;
    let addr = cpu.translate_sized(virtual_addr, MemoryAccess::Store, width)?;
    // 返回 rd 的 AMO.W 旧值需要符号扩展，而参与无符号运算时仍保留原始位型。
    let old_raw = cpu
        .bus
        .read(addr, width)
        .map_err(|_| Exception::StoreAMOAccessFault(virtual_addr))?;
    let old = if width == 4 {
        sign_extend(old_raw, 32)
    } else {
        old_raw
    };
    let rhs = reg(cpu, inst.rs2);

    let value = match funct5 {
        0x01 => rhs,
        0x00 => old_raw.wrapping_add(rhs),
        0x04 => old_raw ^ rhs,
        0x08 => old_raw | rhs,
        0x0c => old_raw & rhs,
        0x10 => amo_min(old_raw, rhs, width),
        0x14 => amo_max(old_raw, rhs, width),
        0x18 => amo_minu(old_raw, rhs, width),
        0x1c => amo_maxu(old_raw, rhs, width),
        _ => unreachable!(),
    };

    write_mem(cpu, addr, virtual_addr, value, width)?;
    write_reg(cpu, inst.rd, old);
    Ok(())
}

fn execute_compressed(cpu: &mut Cpu, raw: u16) -> Result<(), Exception> {
    if raw == 0 {
        return Err(Exception::IllegalInstruction(raw as u64));
    }

    let result = match (raw & 0x3, (raw >> 13) & 0x7) {
        (0b00, 0b000) => c_addi4spn(cpu, raw),
        (0b00, 0b010) => c_load(
            cpu,
            raw,
            4,
            c_lw_imm(raw),
            c_rd_prime(raw),
            c_rs1_prime(raw),
            true,
        ),
        (0b00, 0b011) => c_load(
            cpu,
            raw,
            8,
            c_ld_imm(raw),
            c_rd_prime(raw),
            c_rs1_prime(raw),
            false,
        ),
        (0b00, 0b110) => c_store(
            cpu,
            raw,
            4,
            c_lw_imm(raw),
            c_rs2_prime(raw),
            c_rs1_prime(raw),
        ),
        (0b00, 0b111) => c_store(
            cpu,
            raw,
            8,
            c_ld_imm(raw),
            c_rs2_prime(raw),
            c_rs1_prime(raw),
        ),
        (0b01, 0b000) => c_addi(cpu, raw),
        (0b01, 0b001) => c_addiw(cpu, raw),
        (0b01, 0b010) => c_li(cpu, raw),
        (0b01, 0b011) => c_lui_addi16sp(cpu, raw),
        (0b01, 0b100) => c_misc_alu(cpu, raw),
        (0b01, 0b101) => c_j(cpu, raw),
        (0b01, 0b110) => c_branch_zero(cpu, raw, true),
        (0b01, 0b111) => c_branch_zero(cpu, raw, false),
        (0b10, 0b000) => c_slli(cpu, raw),
        (0b10, 0b010) => c_load(cpu, raw, 4, c_lwsp_imm(raw), c_rd(raw), 2, true),
        (0b10, 0b011) => c_load(cpu, raw, 8, c_ldsp_imm(raw), c_rd(raw), 2, false),
        (0b10, 0b100) => c_jr_mv_add(cpu, raw),
        (0b10, 0b110) => c_store(cpu, raw, 4, c_swsp_imm(raw), c_rs2(raw), 2),
        (0b10, 0b111) => c_store(cpu, raw, 8, c_sdsp_imm(raw), c_rs2(raw), 2),
        _ => Err(Exception::IllegalInstruction(raw as u64)),
    };

    finish(cpu, result)
}

fn c_addi4spn(cpu: &mut Cpu, raw: u16) -> Result<(), Exception> {
    let imm = ((raw as u64 >> 7) & 0x30)
        | ((raw as u64 >> 1) & 0x3c0)
        | ((raw as u64 >> 4) & 0x4)
        | ((raw as u64 >> 2) & 0x8);
    if imm == 0 {
        return Err(Exception::IllegalInstruction(raw as u64));
    }
    write_reg(cpu, c_rd_prime(raw), reg(cpu, 2).wrapping_add(imm));
    advance_compressed_pc(cpu);
    Ok(())
}

fn c_load(
    cpu: &mut Cpu,
    raw: u16,
    size: usize,
    imm: u64,
    rd: u8,
    rs1: u8,
    sign: bool,
) -> Result<(), Exception> {
    if rd == 0 {
        return Err(Exception::IllegalInstruction(raw as u64));
    }
    let virtual_addr = reg(cpu, rs1).wrapping_add(imm);
    require_load_alignment(virtual_addr, size)?;
    let addr = cpu.translate_sized(virtual_addr, MemoryAccess::Load, size)?;
    let value = read_load(cpu, addr, virtual_addr, size)?;
    let value = if sign {
        sign_extend(value, (size * 8) as u32)
    } else {
        value
    };
    write_reg(cpu, rd, value);
    advance_compressed_pc(cpu);
    Ok(())
}

fn c_store(
    cpu: &mut Cpu,
    _raw: u16,
    size: usize,
    imm: u64,
    rs2: u8,
    rs1: u8,
) -> Result<(), Exception> {
    let virtual_addr = reg(cpu, rs1).wrapping_add(imm);
    require_store_alignment(virtual_addr, size)?;
    let addr = cpu.translate_sized(virtual_addr, MemoryAccess::Store, size)?;
    write_mem(cpu, addr, virtual_addr, reg(cpu, rs2), size)?;
    advance_compressed_pc(cpu);
    Ok(())
}

fn c_addi(cpu: &mut Cpu, raw: u16) -> Result<(), Exception> {
    let rd = c_rd(raw);
    let imm = c_imm6(raw);
    write_reg(cpu, rd, reg(cpu, rd).wrapping_add(imm));
    advance_compressed_pc(cpu);
    Ok(())
}

fn c_addiw(cpu: &mut Cpu, raw: u16) -> Result<(), Exception> {
    let rd = c_rd(raw);
    if rd == 0 {
        return Err(Exception::IllegalInstruction(raw as u64));
    }
    write_reg(
        cpu,
        rd,
        sign_extend32(reg(cpu, rd).wrapping_add(c_imm6(raw)) as u32),
    );
    advance_compressed_pc(cpu);
    Ok(())
}

fn c_li(cpu: &mut Cpu, raw: u16) -> Result<(), Exception> {
    write_reg(cpu, c_rd(raw), c_imm6(raw));
    advance_compressed_pc(cpu);
    Ok(())
}

fn c_lui_addi16sp(cpu: &mut Cpu, raw: u16) -> Result<(), Exception> {
    let rd = c_rd(raw);
    if rd == 2 {
        let imm = c_addi16sp_imm(raw);
        if imm == 0 {
            return Err(Exception::IllegalInstruction(raw as u64));
        }
        write_reg(cpu, 2, reg(cpu, 2).wrapping_add(imm));
    } else {
        let imm = c_imm6(raw);
        if imm == 0 {
            return Err(Exception::IllegalInstruction(raw as u64));
        }
        // rd=x0 且立即数非零是 HINT，由 write_reg 屏蔽为空操作。
        write_reg(cpu, rd, sign_extend((imm & 0x3f) << 12, 18));
    }
    advance_compressed_pc(cpu);
    Ok(())
}

fn c_misc_alu(cpu: &mut Cpu, raw: u16) -> Result<(), Exception> {
    let rd = c_rs1_prime(raw);
    let shamt = c_shamt(raw);
    match (raw >> 10) & 0x3 {
        0b00 => write_reg(cpu, rd, reg(cpu, rd).wrapping_shr(shamt)),
        0b01 => write_reg(cpu, rd, (signed(reg(cpu, rd)) >> shamt) as u64),
        0b10 => write_reg(cpu, rd, reg(cpu, rd) & c_imm6(raw)),
        0b11 => {
            let rhs = reg(cpu, c_rs2_prime(raw));
            let value = match ((raw >> 12) & 0x1, (raw >> 5) & 0x3) {
                (0, 0b00) => reg(cpu, rd).wrapping_sub(rhs),
                (0, 0b01) => reg(cpu, rd) ^ rhs,
                (0, 0b10) => reg(cpu, rd) | rhs,
                (0, 0b11) => reg(cpu, rd) & rhs,
                (1, 0b00) => sign_extend32((reg(cpu, rd) as u32).wrapping_sub(rhs as u32)),
                (1, 0b01) => sign_extend32((reg(cpu, rd) as u32).wrapping_add(rhs as u32)),
                _ => return Err(Exception::IllegalInstruction(raw as u64)),
            };
            write_reg(cpu, rd, value);
        }
        _ => unreachable!(),
    }
    advance_compressed_pc(cpu);
    Ok(())
}

fn c_j(cpu: &mut Cpu, raw: u16) -> Result<(), Exception> {
    cpu.write_pc(cpu.pc.wrapping_add(c_j_imm(raw)));
    Ok(())
}

fn c_branch_zero(cpu: &mut Cpu, raw: u16, branch_on_zero: bool) -> Result<(), Exception> {
    let is_zero = reg(cpu, c_rs1_prime(raw)) == 0;
    if is_zero == branch_on_zero {
        cpu.write_pc(cpu.pc.wrapping_add(c_b_imm(raw)));
    } else {
        advance_compressed_pc(cpu);
    }
    Ok(())
}

fn c_slli(cpu: &mut Cpu, raw: u16) -> Result<(), Exception> {
    let rd = c_rd(raw);
    write_reg(cpu, rd, reg(cpu, rd).wrapping_shl(c_shamt(raw)));
    advance_compressed_pc(cpu);
    Ok(())
}

fn c_jr_mv_add(cpu: &mut Cpu, raw: u16) -> Result<(), Exception> {
    let rd = c_rd(raw);
    let rs2 = c_rs2(raw);
    match ((raw >> 12) & 1, rd, rs2) {
        (0, 0, 0) => Err(Exception::IllegalInstruction(raw as u64)),
        (0, _, 0) => {
            cpu.write_pc(reg(cpu, rd) & !1);
            Ok(())
        }
        (0, _, _) => {
            write_reg(cpu, rd, reg(cpu, rs2));
            advance_compressed_pc(cpu);
            Ok(())
        }
        (1, 0, 0) => Err(Exception::Breakpoint(cpu.pc)),
        (1, _, 0) => {
            let link = cpu.pc.wrapping_add(2);
            cpu.write_pc(reg(cpu, rd) & !1);
            write_reg(cpu, 1, link);
            Ok(())
        }
        (1, _, _) => {
            write_reg(cpu, rd, reg(cpu, rd).wrapping_add(reg(cpu, rs2)));
            advance_compressed_pc(cpu);
            Ok(())
        }
        _ => unreachable!(),
    }
}

fn execute_mul_div(lhs: u64, rhs: u64, funct3: u8) -> u64 {
    match funct3 {
        0x0 => lhs.wrapping_mul(rhs),
        0x1 => (((lhs as i64 as i128) * (rhs as i64 as i128)) >> 64) as u64,
        0x2 => (((lhs as i64 as i128) * (rhs as u128 as i128)) >> 64) as u64,
        0x3 => (((lhs as u128) * (rhs as u128)) >> 64) as u64,
        0x4 => div_signed(lhs, rhs),
        0x5 => lhs.checked_div(rhs).unwrap_or(u64::MAX),
        0x6 => rem_signed(lhs, rhs),
        0x7 => {
            if rhs == 0 {
                lhs
            } else {
                lhs % rhs
            }
        }
        _ => unreachable!(),
    }
}

fn execute_mul_div_32(lhs: u64, rhs: u64, funct3: u8) -> u64 {
    match funct3 {
        0x0 => sign_extend32((lhs as u32).wrapping_mul(rhs as u32)),
        0x4 => div_signed_32(lhs, rhs),
        0x5 => div_unsigned_32(lhs, rhs),
        0x6 => rem_signed_32(lhs, rhs),
        0x7 => rem_unsigned_32(lhs, rhs),
        _ => unreachable!(),
    }
}

fn div_signed(lhs: u64, rhs: u64) -> u64 {
    let lhs = lhs as i64;
    let rhs = rhs as i64;
    if rhs == 0 {
        u64::MAX
    } else if lhs == i64::MIN && rhs == -1 {
        lhs as u64
    } else {
        lhs.wrapping_div(rhs) as u64
    }
}

fn rem_signed(lhs: u64, rhs: u64) -> u64 {
    let lhs = lhs as i64;
    let rhs = rhs as i64;
    if rhs == 0 {
        lhs as u64
    } else if lhs == i64::MIN && rhs == -1 {
        0
    } else {
        lhs.wrapping_rem(rhs) as u64
    }
}

fn div_signed_32(lhs: u64, rhs: u64) -> u64 {
    let lhs = lhs as u32 as i32;
    let rhs = rhs as u32 as i32;
    let value = if rhs == 0 {
        -1
    } else if lhs == i32::MIN && rhs == -1 {
        lhs
    } else {
        lhs.wrapping_div(rhs)
    };
    sign_extend32(value as u32)
}

fn div_unsigned_32(lhs: u64, rhs: u64) -> u64 {
    let lhs = lhs as u32;
    let rhs = rhs as u32;
    sign_extend32(lhs.checked_div(rhs).unwrap_or(u32::MAX))
}

fn rem_signed_32(lhs: u64, rhs: u64) -> u64 {
    let lhs = lhs as u32 as i32;
    let rhs = rhs as u32 as i32;
    let value = if rhs == 0 {
        lhs
    } else if lhs == i32::MIN && rhs == -1 {
        0
    } else {
        lhs.wrapping_rem(rhs)
    };
    sign_extend32(value as u32)
}

fn rem_unsigned_32(lhs: u64, rhs: u64) -> u64 {
    let lhs = lhs as u32;
    let rhs = rhs as u32;
    sign_extend32(if rhs == 0 { lhs } else { lhs % rhs })
}

fn amo_min(lhs: u64, rhs: u64, width: usize) -> u64 {
    if signed_width(lhs, width) < signed_width(rhs, width) {
        lhs
    } else {
        rhs
    }
}

fn amo_max(lhs: u64, rhs: u64, width: usize) -> u64 {
    if signed_width(lhs, width) > signed_width(rhs, width) {
        lhs
    } else {
        rhs
    }
}

fn amo_minu(lhs: u64, rhs: u64, width: usize) -> u64 {
    let mask = width_mask(width);
    if lhs & mask < rhs & mask { lhs } else { rhs }
}

fn amo_maxu(lhs: u64, rhs: u64, width: usize) -> u64 {
    let mask = width_mask(width);
    if lhs & mask > rhs & mask { lhs } else { rhs }
}

fn read_load(cpu: &mut Cpu, addr: u64, virtual_addr: u64, size: usize) -> Result<u64, Exception> {
    cpu.bus
        .read(addr, size)
        .map_err(|_| Exception::LoadAccessFault(virtual_addr))
}

fn require_load_alignment(addr: u64, size: usize) -> Result<(), Exception> {
    if addr & (size as u64 - 1) == 0 {
        Ok(())
    } else {
        Err(Exception::LoadAddrMisaligned(addr))
    }
}

fn require_store_alignment(addr: u64, size: usize) -> Result<(), Exception> {
    if addr & (size as u64 - 1) == 0 {
        Ok(())
    } else {
        Err(Exception::StoreAMOAddrMisaligned(addr))
    }
}

fn write_mem(
    cpu: &mut Cpu,
    addr: u64,
    virtual_addr: u64,
    value: u64,
    size: usize,
) -> Result<(), Exception> {
    cpu.clear_reservation();
    cpu.bus
        .write(addr, value, size)
        .map_err(|_| Exception::StoreAMOAccessFault(virtual_addr))
}

fn try_accelerate_memset_loop(cpu: &mut Cpu, inst: &Instruction) -> Result<bool, Exception> {
    // xv6 会用逐字节循环清零或填毒内存；这里只批处理精确匹配且完全位于 DRAM 的循环，
    // 既缩短启动步数，又避免把相似但带 MMIO 副作用的循环错误合并。
    if !cpu.xv6_acceleration_enabled()
        || cpu.privilege != PrivilegeMode::Supervisor
        || inst.funct3 != 0x1
        || imm_b(inst.raw) != u64::MAX - 5
    {
        return Ok(false);
    }

    let target = cpu.pc.wrapping_sub(6);
    // 快速检查不跨页拼接指令，跨页循环交给普通取指路径。
    if target & (crate::paging::PAGE_SIZE - 1) > crate::paging::PAGE_SIZE - 4 || inst.rs1 == 0 {
        return Ok(false);
    }
    let store_addr = match cpu.translate_sized(target, MemoryAccess::Fetch, 4) {
        Ok(addr) => addr,
        Err(_) => return Ok(false),
    };
    let addi_addr = match cpu.translate_sized(target.wrapping_add(4), MemoryAccess::Fetch, 2) {
        Ok(addr) => addr,
        Err(_) => return Ok(false),
    };
    let store_raw = match cpu.bus.read(store_addr, 4) {
        Ok(raw) => raw as u32,
        Err(_) => return Ok(false),
    };
    let addi_raw = match cpu.bus.read(addi_addr, 2) {
        Ok(raw) => raw as u16,
        Err(_) => return Ok(false),
    };

    let store = decode(store_raw);
    let is_zero_offset_sb = is_constant_byte_store(&store, inst.rs1);
    let is_addi_one = addi_raw & 0x3 == 0x1
        && (addi_raw >> 13) & 0x7 == 0
        && c_rd(addi_raw) == inst.rs1
        && c_imm6(addi_raw) == 1;
    if !is_zero_offset_sb || !is_addi_one {
        return Ok(false);
    }

    let start = reg(cpu, inst.rs1);
    let loop_end = reg(cpu, inst.rs2);
    let Some((dram_base, dram_end)) = cpu.xv6_dram_range() else {
        return Ok(false);
    };
    if start >= loop_end || start < dram_base || loop_end > dram_end {
        return Ok(false);
    }
    let end = loop_end.min(start.saturating_add(crate::cpu::XV6_FAST_PATH_MAX_BYTES));
    if !identity_store_range(cpu, start, end) {
        return Ok(false);
    }

    fill_dram_bytes(cpu, start, end, reg(cpu, store.rs2) as u8)?;
    write_reg(cpu, inst.rs1, end);
    cpu.write_pc(if end == loop_end {
        cpu.pc.wrapping_add(4)
    } else {
        target
    });
    Ok(true)
}

fn is_constant_byte_store(store: &Instruction, pointer: u8) -> bool {
    store.opcode == 0x23
        && store.funct3 == 0
        && store.rs1 == pointer
        // 源寄存器若也是循环指针，每轮写入值都会变化，不能合并成常量填充。
        && store.rs2 != pointer
        && imm_s(store.raw) == 0
}

fn identity_store_range(cpu: &mut Cpu, start: u64, end: u64) -> bool {
    let mut addr = start;
    while addr < end {
        let page_end = (addr | (crate::paging::PAGE_SIZE - 1))
            .saturating_add(1)
            .min(end);
        let size = (page_end - addr) as usize;
        match cpu.translate_sized(addr, MemoryAccess::Store, size) {
            Ok(physical) if physical == addr => addr = page_end,
            _ => return false,
        }
    }
    true
}

fn fill_dram_bytes(cpu: &mut Cpu, start: u64, end: u64, byte: u8) -> Result<(), Exception> {
    cpu.clear_reservation();
    let mut addr = start;
    let pattern = u32::from_le_bytes([byte; 4]);

    while addr < end && addr & 0x3 != 0 {
        write_mem(cpu, addr, addr, byte as u64, 1)?;
        addr = addr.wrapping_add(1);
    }
    while addr.wrapping_add(4) <= end {
        cpu.bus.write(addr, u64::from(pattern), 4)?;
        addr = addr.wrapping_add(4);
    }
    while addr < end {
        write_mem(cpu, addr, addr, byte as u64, 1)?;
        addr = addr.wrapping_add(1);
    }
    Ok(())
}

fn finish(cpu: &mut Cpu, result: Result<(), Exception>) -> Result<(), Exception> {
    // 即使执行路径误写了 x0，指令边界也必须恢复架构规定的常零值。
    cpu.registers[0] = 0;
    result
}

fn reg(cpu: &Cpu, reg: u8) -> u64 {
    cpu.registers[reg as usize]
}

fn write_reg(cpu: &mut Cpu, reg: u8, value: u64) {
    // 在写入口同时屏蔽 x0，可避免大多数路径短暂破坏零号寄存器。
    if reg != 0 {
        cpu.registers[reg as usize] = value;
    }
}

fn advance_compressed_pc(cpu: &mut Cpu) {
    // 压缩指令自行前进 2 字节，CPU 外层看到 PC 已变化后不会再追加 4。
    cpu.write_pc(cpu.pc.wrapping_add(2));
}

fn signed(value: u64) -> i64 {
    value as i64
}

fn signed_width(value: u64, width: usize) -> i64 {
    if width == 4 {
        value as u32 as i32 as i64
    } else {
        value as i64
    }
}

fn width_mask(width: usize) -> u64 {
    if width == 4 {
        u32::MAX as u64
    } else {
        u64::MAX
    }
}

fn sign_extend(value: u64, bits: u32) -> u64 {
    // 先把源符号位移到 bit63，再做算术右移；结果保持 RV64 的二进制补码位型。
    ((value << (64 - bits)) as i64 >> (64 - bits)) as u64
}

fn sign_extend32(value: u32) -> u64 {
    value as i32 as i64 as u64
}

fn imm_i(raw: u32) -> u64 {
    sign_extend((raw >> 20) as u64, 12)
}

fn imm_s(raw: u32) -> u64 {
    // S 型立即数被 rs2 两侧字段分开存放，拼接后再按 12 位符号扩展。
    sign_extend((((raw >> 25) << 5) | ((raw >> 7) & 0x1f)) as u64, 12)
}

fn imm_b(raw: u32) -> u64 {
    // 分支偏移最低位恒为零，编码中的高低位需要按规范位置重新排列。
    let imm = ((raw >> 31) << 12)
        | (((raw >> 7) & 0x1) << 11)
        | (((raw >> 25) & 0x3f) << 5)
        | (((raw >> 8) & 0x0f) << 1);
    sign_extend(imm as u64, 13)
}

fn imm_u(raw: u32) -> u64 {
    // RV64 的 U 型结果先形成 32 位值，再从 bit31 符号扩展到 XLEN。
    sign_extend((raw & 0xffff_f000) as u64, 32)
}

fn imm_j(raw: u32) -> u64 {
    // J 型偏移同样隐含最低零位，其余位在指令中并非连续排列。
    let imm = ((raw >> 31) << 20)
        | (((raw >> 12) & 0xff) << 12)
        | (((raw >> 20) & 0x1) << 11)
        | (((raw >> 21) & 0x03ff) << 1);
    sign_extend(imm as u64, 21)
}

fn shamt64(raw: u32) -> u32 {
    (raw >> 20) & 0x3f
}

fn shamt32(raw: u32) -> u32 {
    (raw >> 20) & 0x1f
}

fn c_rd(raw: u16) -> u8 {
    ((raw >> 7) & 0x1f) as u8
}

fn c_rs2(raw: u16) -> u8 {
    ((raw >> 2) & 0x1f) as u8
}

fn c_rd_prime(raw: u16) -> u8 {
    8 + (((raw >> 2) & 0x7) as u8)
}

fn c_rs1_prime(raw: u16) -> u8 {
    8 + (((raw >> 7) & 0x7) as u8)
}

fn c_rs2_prime(raw: u16) -> u8 {
    8 + (((raw >> 2) & 0x7) as u8)
}

fn c_imm6(raw: u16) -> u64 {
    // 压缩立即数的符号位位于 bit12，其余五位位于 bit6:2。
    sign_extend(((raw as u64 >> 7) & 0x20) | ((raw as u64 >> 2) & 0x1f), 6)
}

fn c_shamt(raw: u16) -> u32 {
    (((raw >> 7) & 0x20) | ((raw >> 2) & 0x1f)) as u32
}

fn c_addi16sp_imm(raw: u16) -> u64 {
    // C.ADDI16SP 的非连续位段隐含低四位为零，因此拼接后按 10 位数符号扩展。
    let imm = ((raw as u64 >> 3) & 0x200)
        | ((raw as u64 >> 2) & 0x10)
        | (((raw as u64) << 1) & 0x40)
        | (((raw as u64) << 4) & 0x180)
        | (((raw as u64) << 3) & 0x20);
    sign_extend(imm, 10)
}

fn c_j_imm(raw: u16) -> u64 {
    // 压缩跳转偏移的位序经过重排，最低位同样由指令格式隐含为零。
    let imm = ((raw as u64 >> 1) & 0x800)
        | ((raw as u64 >> 7) & 0x10)
        | ((raw as u64 >> 1) & 0x300)
        | (((raw as u64) << 2) & 0x400)
        | ((raw as u64 >> 1) & 0x40)
        | (((raw as u64) << 1) & 0x80)
        | ((raw as u64 >> 2) & 0x0e)
        | (((raw as u64) << 3) & 0x20);
    sign_extend(imm, 12)
}

fn c_b_imm(raw: u16) -> u64 {
    // 压缩分支使用 9 位有符号偏移；掩码同时完成位段搬移和最低零位保留。
    let imm = ((raw as u64 >> 4) & 0x100)
        | (((raw as u64) << 1) & 0xc0)
        | (((raw as u64) << 3) & 0x20)
        | ((raw as u64 >> 7) & 0x18)
        | ((raw as u64 >> 2) & 0x06);
    sign_extend(imm, 9)
}

fn c_lw_imm(raw: u16) -> u64 {
    ((raw as u64 >> 7) & 0x38) | ((raw as u64 >> 4) & 0x04) | (((raw as u64) << 1) & 0x40)
}

fn c_ld_imm(raw: u16) -> u64 {
    ((raw as u64 >> 7) & 0x38) | (((raw as u64) << 1) & 0xc0)
}

fn c_lwsp_imm(raw: u16) -> u64 {
    ((raw as u64 >> 7) & 0x20) | ((raw as u64 >> 2) & 0x1c) | (((raw as u64) << 4) & 0xc0)
}

fn c_ldsp_imm(raw: u16) -> u64 {
    ((raw as u64 >> 7) & 0x20) | ((raw as u64 >> 2) & 0x18) | (((raw as u64) << 4) & 0x1c0)
}

fn c_swsp_imm(raw: u16) -> u64 {
    ((raw as u64 >> 7) & 0x3c) | ((raw as u64 >> 1) & 0xc0)
}

fn c_sdsp_imm(raw: u16) -> u64 {
    ((raw as u64 >> 7) & 0x38) | ((raw as u64 >> 1) & 0x1c0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dram::Dram;

    const TEST_BASE: u64 = 0x8000_0000;

    fn test_cpu() -> Cpu {
        Cpu::with_reset_vector(
            Box::new(Dram::with_layout(TEST_BASE, 128)),
            TEST_BASE,
            TEST_BASE + 128,
        )
    }

    fn run_instruction(cpu: &mut Cpu, raw: u32) {
        cpu.pc = TEST_BASE;
        cpu.bus.write(TEST_BASE, u64::from(raw), 4).unwrap();
        cpu.step().unwrap();
    }

    #[test]
    fn negative_immediates_update_registers_memory_and_control_flow() {
        let mut cpu = test_cpu();
        run_instruction(&mut cpu, 0xfff0_0093); // addi ra, zero, -1
        assert_eq!(cpu.registers[1], u64::MAX);
        run_instruction(&mut cpu, 0xffff_e7b7); // lui a5, 0xffffe
        assert_eq!(cpu.registers[15], 0xffff_ffff_ffff_e000);

        cpu.registers[2] = TEST_BASE + 16;
        cpu.bus.write(TEST_BASE + 8, 0xff, 1).unwrap();
        run_instruction(&mut cpu, 0xfe01_0c23); // sb zero, -8(sp)
        assert_eq!(cpu.bus.read(TEST_BASE + 8, 1), Ok(0));
        run_instruction(&mut cpu, 0xfe00_0ce3); // beq zero, zero, -8
        assert_eq!(cpu.pc, TEST_BASE - 8);
        run_instruction(&mut cpu, 0xfe9f_f0ef); // jal ra, -24
        assert_eq!(cpu.pc, TEST_BASE - 24);
        assert_eq!(cpu.registers[1], TEST_BASE + 4);
    }

    #[test]
    fn compressed_instructions_use_the_encoded_stack_offsets_and_branch_targets() {
        let mut cpu = test_cpu();
        cpu.registers[2] = TEST_BASE + 32;
        run_instruction(&mut cpu, 0x1141); // c.addi sp, -16
        assert_eq!(cpu.registers[2], TEST_BASE + 16);
        run_instruction(&mut cpu, 0x6109); // c.addi16sp sp, 128
        assert_eq!(cpu.registers[2], TEST_BASE + 144);

        cpu.registers[2] = TEST_BASE + 32;
        for (raw, destination, offset) in [(0x47b2, 15, 12), (0x4502, 10, 0), (0x4412, 8, 4)] {
            cpu.bus
                .write(TEST_BASE + 32 + offset, 0x8000_0007, 4)
                .unwrap();
            run_instruction(&mut cpu, raw); // c.lwsp，加载结果须符号扩展。
            assert_eq!(cpu.registers[destination], 0xffff_ffff_8000_0007);
        }
        let value = 0x0123_4567_89ab_cdef;
        cpu.bus.write(TEST_BASE + 40, value, 8).unwrap();
        run_instruction(&mut cpu, 0x60a2); // c.ldsp ra, 8(sp)
        assert_eq!(cpu.registers[1], value);
        cpu.registers[10] = value;
        run_instruction(&mut cpu, 0xc62a); // c.swsp a0, 12(sp)
        assert_eq!(cpu.bus.read(TEST_BASE + 44, 4), Ok(value as u32 as u64));
        run_instruction(&mut cpu, 0xe406); // c.sdsp ra, 8(sp)
        assert_eq!(cpu.bus.read(TEST_BASE + 40, 8), Ok(value));

        run_instruction(&mut cpu, 0xa001); // c.j 0
        assert_eq!(cpu.pc, TEST_BASE);
        run_instruction(&mut cpu, 0xb761); // c.j -120
        assert_eq!(cpu.pc, TEST_BASE - 120);
        cpu.registers[15] = 0;
        run_instruction(&mut cpu, 0xdfe5); // c.beqz a5, -8
        assert_eq!(cpu.pc, TEST_BASE - 8);
    }

    #[test]
    fn reserved_misc_mem_and_compressed_encodings_follow_the_spec() {
        let mut cpu = test_cpu();
        let reserved_misc_mem = 0x0000_200f;
        assert_eq!(
            execute(&mut cpu, decode(reserved_misc_mem)),
            Err(Exception::IllegalInstruction(u64::from(reserved_misc_mem)))
        );

        let reserved_c_lui = 0x6001;
        assert_eq!(
            execute(&mut cpu, decode(reserved_c_lui)),
            Err(Exception::IllegalInstruction(u64::from(reserved_c_lui)))
        );

        cpu.pc = TEST_BASE;
        assert_eq!(execute(&mut cpu, decode(0x8006)), Ok(())); // c.mv x0, x1 是 HINT。
        assert_eq!(cpu.pc, TEST_BASE + 2);
        assert_eq!(
            execute(&mut cpu, decode(0x8002)),
            Err(Exception::IllegalInstruction(0x8002))
        );
    }
}
