pub struct Cpu {
    regs: [u64; 32],      // 32 个通用寄存器 x0-x31
    pc: u64,              // 程序计数器 (Program Counter)
    // csr: Csr,             // CSR 寄存器组
    // bus: Bus,             // 连接到总线
    // mode: PrivilegeMode,  // 当前特权级 (User, Supervisor, Machine)
}
