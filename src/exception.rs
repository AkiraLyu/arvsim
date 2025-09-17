#[derive(Debug)]
pub enum Exception {
    LoadAccessFault(u64),   // 读内存出错（带地址信息）
    StoreAccessFault(u64),  // 写内存出错（带地址信息）
    IllegalInstruction(u64),    // 地址不合法
}