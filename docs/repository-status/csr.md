# `src/csr.rs`：CSR 状态

## 设计

- 用固定大小数组保存 4096 个 CSR 地址空间。
- 对监督模式 CSR 提供简化别名行为，使 xv6 能通过 `sstatus/sie/sip` 访问相应机器模式状态。

## 实现

- 定义了 xv6 启动和运行需要的机器模式、监督模式以及 `time` 相关 CSR 常量。
- `load(SIE)` 返回 `MIE & MIDELEG`。
- `load(SIP)` 返回 `MIP & MIDELEG`。
- `load(SSTATUS)` 返回 `MSTATUS & MASK_SSTATUS`。
- `store(SIE)`、`store(SIP)`、`store(SSTATUS)` 会写回对应机器模式 CSR 的相关位。
- 其他 CSR 地址直接读写数组。

## 接口

- CSR 地址常量，例如 `MSTATUS`、`MIDELEG`、`SSTATUS`、`STVEC`、`SEPC`、`SATP`、`STIMECMP`、`TIME`。
- 状态位掩码，例如 `MASK_SIE`、`MASK_SPIE`、`MASK_SPP`、`MASK_STIP`、`MASK_SEIP`。
- `struct Csr`
  - `Csr::new()`
  - `load(addr)`
  - `store(addr, value)`
  - `dump_csr()`

## 限制

- 没有完整实现 WARL/WPRI、只读位、CSR 权限等级或非法 CSR 访问检查。
- `SIP` 写入逻辑是简化模型，只服务当前 xv6 路径。
