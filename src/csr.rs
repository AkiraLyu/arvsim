pub struct PrivilegeMode {
    pub mstatus: u64, // Machine Status Register
    pub mie: u64,     // Machine Interrupt Enable
    pub mip: u64,     // Machine Interrupt Pending
    pub medeleg: u64, // Machine Exception Delegation
    pub mideleg: u64, // Machine Interrupt Delegation
    pub mepc: u64,    // Machine Exception Program Counter
    pub mcause: u64,  // Machine Cause Register
    pub mtval: u64,   // Machine Trap Value
    pub mtvec: u64,   // Machine Trap-Vector Base Address
}

