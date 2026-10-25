//! Mutex implementations with allocation and waiter metadata for detection.
// AI 学习协助：本文件的实验增量由 AI 助手协助实现，逐处说明见 Study/修改记录。
// 参考：https://learningos.cn/rCore-Tutorial-Guide/chapter8/；原始代码来自课程模板，未改动官方评测器。
use super::UPSafeCell;
use super::deadlock::{current_tid, Snapshot};
use crate::task::{TaskControlBlock, block_current_and_run_next, suspend_current_and_run_next,
                  current_task, wakeup_task};
use alloc::{collections::VecDeque, sync::Arc, vec::Vec};

/// Common mutex operations and a read-only resource snapshot.
pub trait Mutex: Sync + Send {
    /// Acquire, sleeping or yielding while the mutex is busy.
    fn lock(&self);
    /// Release or transfer ownership directly to the next waiter.
    fn unlock(&self);
    /// Sample real owners and pending requests.
    fn snapshot(&self, threads: usize) -> Snapshot;
}

/// A yielding spin mutex (the kernel remains uniprocessor).
pub struct MutexSpin { inner: UPSafeCell<SpinInner> }
struct SpinInner { owner: Option<usize>, waiters: Vec<usize> }
impl MutexSpin {
    /// Create an unlocked spin mutex.
    pub fn new() -> Self {
        Self { inner: unsafe { UPSafeCell::new(SpinInner { owner: None, waiters: Vec::new() }) } }
    }
}
impl Mutex for MutexSpin {
    fn lock(&self) {
        let tid = current_tid();
        loop {
            let mut inner = self.inner.exclusive_access();
            if inner.owner.is_none() {
                inner.owner = Some(tid);
                inner.waiters.retain(|waiter| *waiter != tid);
                return;
            }
            if !inner.waiters.contains(&tid) { inner.waiters.push(tid); }
            drop(inner);
            suspend_current_and_run_next();
        }
    }
    fn unlock(&self) { self.inner.exclusive_access().owner = None; }
    fn snapshot(&self, threads: usize) -> Snapshot {
        let inner = self.inner.exclusive_access();
        let mut snapshot = Snapshot::new(threads, usize::from(inner.owner.is_none()));
        if let Some(tid) = inner.owner { snapshot.allocation[tid] = 1; }
        for tid in &inner.waiters { snapshot.need[*tid] = 1; }
        snapshot
    }
}

/// Mutex whose busy acquirers block in a FIFO wait queue.
pub struct MutexBlocking { inner: UPSafeCell<BlockingInner> }
struct BlockingInner { owner: Option<usize>, wait_queue: VecDeque<Arc<TaskControlBlock>> }
impl MutexBlocking {
    /// Create an unlocked blocking mutex.
    pub fn new() -> Self {
        Self { inner: unsafe { UPSafeCell::new(BlockingInner { owner: None, wait_queue: VecDeque::new() }) } }
    }
}
impl Mutex for MutexBlocking {
    fn lock(&self) {
        let mut inner = self.inner.exclusive_access();
        if inner.owner.is_some() {
            inner.wait_queue.push_back(current_task().unwrap());
            drop(inner);
            block_current_and_run_next();
        } else { inner.owner = Some(current_tid()); }
    }
    fn unlock(&self) {
        let mut inner = self.inner.exclusive_access();
        assert!(inner.owner.is_some());
        if let Some(task) = inner.wait_queue.pop_front() {
            // Reserve the mutex before waking the task; no phantom free unit.
            inner.owner = Some(task.inner_exclusive_access().res.as_ref().unwrap().tid);
            wakeup_task(task);
        } else { inner.owner = None; }
    }
    fn snapshot(&self, threads: usize) -> Snapshot {
        let inner = self.inner.exclusive_access();
        let mut snapshot = Snapshot::new(threads, usize::from(inner.owner.is_none()));
        if let Some(tid) = inner.owner { snapshot.allocation[tid] = 1; }
        for task in &inner.wait_queue {
            snapshot.need[task.inner_exclusive_access().res.as_ref().unwrap().tid] = 1;
        }
        snapshot
    }
}
