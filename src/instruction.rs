#[derive(Debug, Clone, Copy)]
pub enum Instruction {
    Lui { rd: usize, imm: u32 },
    Auipc { rd: usize, imm: u32 },
    Jal { rd: usize, imm: i32 },
    Jalr { rd: usize, rs1: usize, imm: i32 },

    Branch { funct3: u32, rs1: usize, rs2: usize, imm: i32 },
    Load { funct3: u32, rd: usize, rs1: usize, imm: i32 },
    Store { funct3: u32, rs1: usize, rs2: usize, imm: i32 },

    OpImm { funct3: u32, rd: usize, rs1: usize, imm: i32 },
    Op { funct3: u32, rd: usize, rs1: usize, rs2: usize, funct7: u32 },

    System { funct3: u32, csr: u32, rd: usize, rs1: usize },
    Ecall,
    Ebreak,

    Illegal(u32),
}

fn get_bits(word: u32, high: u32, low: u32) -> u32 {
    (word >> low) & ((1 << (high - low + 1)) - 1)
}
fn sign_extend(value: u32, bits: u32) -> i32 {
    let shift = 32 - bits;
    ((value << shift) as i32) >> shift
}

pub fn decode(word: u32) -> Instruction {
    let opcode = get_bits(word, 6, 0);
    let rd = get_bits(word, 11, 7) as usize;
    let funct3 = get_bits(word, 14, 12);
    let rs1 = get_bits(word, 19, 15) as usize;
    let rs2 = get_bits(word, 24, 20) as usize;
    let funct7 = get_bits(word, 31, 25);

    match opcode {
        0b0110111 => { // LUI
            let imm = word & 0xfffff000;
            Instruction::Lui { rd, imm }
        }
        0b0010111 => { // AUIPC
            let imm = word & 0xfffff000;
            Instruction::Auipc { rd, imm }
        }
        0b1101111 => { // JAL
            let imm = ((word >> 21) & 0x3ff) << 1
                    | ((word >> 20) & 0x1) << 11
                    | ((word >> 12) & 0xff) << 12
                    | ((word as i32 >> 31) << 20) as u32;
            Instruction::Jal { rd, imm: sign_extend(imm, 21) }
        }
        0b1100111 => { // JALR
            let imm = sign_extend(get_bits(word, 31, 20), 12);
            Instruction::Jalr { rd, rs1, imm }
        }
        0b1100011 => { // BRANCH
            let imm = ((word >> 8) & 0xf) << 1
                    | ((word >> 25) & 0x3f) << 5
                    | ((word >> 7) & 0x1) << 11
                    | ((word as i32 >> 31) << 12) as u32;
            Instruction::Branch { funct3, rs1, rs2, imm: sign_extend(imm, 13) }
        }
        0b0000011 => { // LOAD
            let imm = sign_extend(get_bits(word, 31, 20), 12);
            Instruction::Load { funct3, rd, rs1, imm }
        }
        0b0100011 => { // STORE
            let imm = (get_bits(word, 11, 7)) | (get_bits(word, 31, 25) << 5);
            Instruction::Store { funct3, rs1, rs2, imm: sign_extend(imm, 12) }
        }
        0b0010011 => { // OP-IMM
            let imm = sign_extend(get_bits(word, 31, 20), 12);
            Instruction::OpImm { funct3, rd, rs1, imm }
        }
        0b0110011 => { // OP
            Instruction::Op { funct3, rd, rs1, rs2, funct7 }
        }
        0b1110011 => { // SYSTEM
            match word {
                0x00000073 => Instruction::Ecall,
                0x00100073 => Instruction::Ebreak,
                _ => {
                    let csr = get_bits(word, 31, 20);
                    Instruction::System { funct3, csr, rd, rs1 }
                }
            }
        }
        _ => Instruction::Illegal(word),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_decode() {
        let instruction = decode(0x00000013);
        match instruction {
            Instruction::OpImm { funct3, rd, rs1, imm } => {
                assert_eq!(funct3, 0);
                assert_eq!(rd, 0);
                assert_eq!(rs1, 0);
                assert_eq!(imm, 0);
            }
            _ => panic!("Unexpected instruction: {:?}", instruction),
        }
    }
}
