use crate::{cpu::Cpu, trap::Exception};

pub struct Instruction {
    pub opcode: u8,
    pub rd: u8,
    pub funct3: u8,
    pub rs1: u8,
    pub rs2: u8,
    pub funct7: u8,
    pub raw: u32,
}

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

pub fn execute(cpu: &mut Cpu, inst: Instruction) -> Result<(), Exception> {
    match inst.opcode {
        0x33 => { // R-type (ADD, SUB, etc.)
            match (inst.funct3, inst.funct7) {
                (0x0, 0x00) => { // ADD
                    let result = cpu.registers[inst.rs1 as usize].wrapping_add(cpu.registers[inst.rs2 as usize]);
                    cpu.registers[inst.rd as usize] = result;
                    Ok(())
                }
                (0x0, 0x20) => { // SUB
                    let result = cpu.registers[inst.rs1 as usize].wrapping_sub(cpu.registers[inst.rs2 as usize]);
                    cpu.registers[inst.rd as usize] = result;
                    Ok(())
                }
                _ => Err(Exception::IllegalInstruction(inst.opcode as u64)),
            }
        }
        0x13 => { // I-type (ADDI, etc.)
            match inst.funct3 {
                0x0 => { // ADDI
                    let imm = ((inst.raw as i32) >> 20) as u64;
                    cpu.registers[inst.rd as usize] = cpu.registers[inst.rs1 as usize].wrapping_add(imm);
                    Ok(())
                }
                _ => Err(Exception::IllegalInstruction(inst.opcode as u64)),
            }
        }
        0x03 => { // Load
            let addr = cpu.registers[inst.rs1 as usize].wrapping_add(inst.rs2 as u64); // I-type 立即数
            match inst.funct3 {
                0x0 => { // LB
                    let val = cpu.bus.read(addr,1)?;
                    cpu.registers[inst.rd as usize] = val;
                    Ok(())
                }
                0x1 => { // LH
                    let val = cpu.bus.read(addr,2)?;
                    cpu.registers[inst.rd as usize] = val;
                    Ok(())
                }
                0x2 => { // LW
                    let val = cpu.bus.read(addr,4)?;
                    cpu.registers[inst.rd as usize] = val;
                    Ok(())
                }
                0x3 => { // LD (64-bit)
                    let val = cpu.bus.read(addr,8)?;
                    cpu.registers[inst.rd as usize] = val;
                    Ok(())
                }
                _ => Err(Exception::IllegalInstruction(inst.opcode as u64)),
            }
        }
        0x23 => { // Store
            let addr = cpu.registers[inst.rs1 as usize].wrapping_add(inst.rs2 as u64); // S-type 立即数
            match inst.funct3 {
                0x0 => cpu.bus.write(addr, 1, cpu.registers[inst.rd as usize] as usize)?,
                0x1 => cpu.bus.write(addr, 2, cpu.registers[inst.rd as usize] as usize)?,
                0x2 => cpu.bus.write(addr, 4, cpu.registers[inst.rd as usize] as usize)?,
                0x3 => cpu.bus.write(addr, 8, cpu.registers[inst.rd as usize] as usize)?,
                _ => return Err(Exception::IllegalInstruction(inst.opcode as u64)),
            };
            Ok(())
        }
        _ => Err(Exception::IllegalInstruction(cpu.pc)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_addi() {
        let inst: u32 = 0x02a00f93;
        let decoded = decode(inst);

        assert_eq!(decoded.opcode, 0x13);
        assert_eq!(decoded.rd, 31);
        assert_eq!(decoded.funct3, 0);
        assert_eq!(decoded.rs1, 0);
        assert_eq!(decoded.rs2, 10);
        assert_eq!(decoded.funct7, 0x01);
    }
}
