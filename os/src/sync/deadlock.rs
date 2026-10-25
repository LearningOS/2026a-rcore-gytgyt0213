//! Detect cycles using current availability, allocations and pending requests.
// AI 学习协助：本文件的实验增量由 AI 助手协助实现，逐处说明见 Study/修改记录。
// 参考：https://learningos.cn/rCore-Tutorial-Guide/chapter8/；原始代码来自课程模板，未改动官方评测器。
use alloc::{vec, vec::Vec};
use crate::task::{current_process, current_task};

/// A single resource type's state, sampled without changing its real state.
pub struct Snapshot {
    /// Units that can be granted immediately.
    pub available: usize,
    /// Units already granted to each thread, including transfers to woken tasks.
    pub allocation: Vec<usize>,
    /// Outstanding requests for each thread.
    pub need: Vec<usize>,
}

impl Snapshot {
    /// Start a resource snapshot with no holders or waiters.
    pub fn new(threads: usize, available: usize) -> Self {
        Self { available, allocation: vec![0; threads], need: vec![0; threads] }
    }
}

/// Current user thread ID; callers do not hold this task's inner borrow.
pub fn current_tid() -> usize {
    current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid
}

/// Test a tentative request without placing it in any real wait queue.
pub fn would_deadlock(semaphore: bool, requested: usize) -> bool {
    let process = current_process();
    let inner = process.inner_exclusive_access();
    if !inner.deadlock_detect { return false; }
    let threads = inner.tasks.len();
    let mut resources: Vec<Snapshot> = if semaphore {
        inner.semaphore_list.iter().map(|s| s.as_ref().map_or_else(
            || Snapshot::new(threads, 0), |s| s.snapshot(threads))).collect()
    } else {
        inner.mutex_list.iter().map(|m| m.as_ref().map_or_else(
            || Snapshot::new(threads, 0), |m| m.snapshot(threads))).collect()
    };
    let tid = current_tid();
    resources[requested].need[tid] += 1;
    let mut finish: Vec<bool> = inner.tasks.iter().map(|task| task.as_ref().map_or(true,
        |task| task.inner_exclusive_access().exit_code.is_some())).collect();
    loop {
        let mut progress = false;
        for thread in 0..threads {
            if !finish[thread] && resources.iter().all(|r| r.need[thread] <= r.available) {
                for resource in &mut resources { resource.available += resource.allocation[thread]; }
                finish[thread] = true;
                progress = true;
            }
        }
        if !progress { break; }
    }
    finish.iter().any(|done| !done)
}
