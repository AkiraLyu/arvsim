//! 平台设备之间使用的电平触发中断线。
//!
//! 中断源只负责拉高或拉低线路；PLIC 负责锁存请求、仲裁并把结果转换为 hart 的外部中断。

use std::cell::Cell;
use std::rc::Rc;

/// 可由一个设备驱动、由中断控制器采样的共享电平信号。
#[derive(Clone, Default)]
pub struct InterruptLine(Rc<Cell<bool>>);

impl InterruptLine {
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置设备当前是否请求服务。
    pub fn set(&self, asserted: bool) {
        self.0.set(asserted);
    }

    pub fn assert(&self) {
        self.set(true);
    }

    pub fn deassert(&self) {
        self.set(false);
    }

    /// 返回控制器当前采样到的线路电平。
    pub fn is_asserted(&self) -> bool {
        self.0.get()
    }
}
