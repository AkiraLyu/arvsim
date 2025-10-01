pub struct Instruction {
    pub opcode: u8,
    pub rd: u8,
    pub funct3: u8,
    pub rs1: u8,
    pub rs2: u8,
    pub funct7: u8,
}
pub fn decode(instruction:u32) -> Instruction {
    let opcode = (instruction & 0x0000007f) as u8;
    let rd = ((instruction & 0x00000f80) >> 7) as u8;
    let rs1 = ((instruction & 0x000f8000) >> 15) as u8;
    let rs2 = ((instruction & 0x01f00000) >> 20) as u8;
    let funct3 = ((instruction & 0x00007000) >> 12) as u8;
    let funct7 = ((instruction & 0xfe000000) >> 25) as u8;
    Instruction { opcode, rd, rs1, rs2, funct3, funct7 }
}

pub fn execute(cpu: &mut crate::cpu::Cpu, inst: Instruction) -> Result<(), crate::exception::Exception> {
    match inst.opcode {
        _ => {todo!()}
    }
}