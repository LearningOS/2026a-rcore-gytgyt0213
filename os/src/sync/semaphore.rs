//! Semaphore

// AI 学习协助：本文件的实验增量由 AI 助手协助实现，逐处说明见 Study/修改记录。
// 参考：https://learningos.cn/rCore-Tutorial-Guide/chapter8/；原始代码来自课程模板，未改动官方评测器。
use crate::sync::UPSafeCell;
use crate::task::{block_current_and_run_next, current_task, wakeup_task, TaskControlBlock};
use alloc::{collections::{BTreeMap, VecDeque}, sync::Arc};
use super::deadlock::{current_tid, Snapshot};

/// semaphore structure
pub struct Semaphore {
    /// semaphore inner
    pub inner: UPSafeCell<SemaphoreInner>,
}

pub struct SemaphoreInner {
    /// Actual grants, including units reserved for just-awakened tasks.
    pub allocations: BTreeMap<usize, usize>,
    pub count: isize,
    pub wait_queue: VecDeque<Arc<TaskControlBlock>>,
}

impl Semaphore {
    /// Create a new semaphore
    pub fn new(res_count: usize) -> Self {
        trace!("kernel: Semaphore::new");
        Self {
            inner: unsafe {
                UPSafeCell::new(SemaphoreInner {
                    allocations: BTreeMap::new(),
                    count: res_count as isize,
                    wait_queue: VecDeque::new(),
                })
            },
        }
    }

    /// up operation of semaphore
    pub fn up(&self) {
        trace!("kernel: Semaphore::up");
        let mut inner = self.inner.exclusive_access();
        if let Some(count) = inner.allocations.get_mut(&current_tid()) {
            *count = count.saturating_sub(1);
        }
        inner.count += 1;
        if inner.count <= 0 {
            if let Some(task) = inner.wait_queue.pop_front() {
                let tid = task.inner_exclusive_access().res.as_ref().unwrap().tid;
                *inner.allocations.entry(tid).or_insert(0) += 1;
                wakeup_task(task);
            }
        }
    }

    /// down operation of semaphore
    pub fn down(&self) {
        trace!("kernel: Semaphore::down");
        let mut inner = self.inner.exclusive_access();
        inner.count -= 1;
        if inner.count < 0 {
            inner.wait_queue.push_back(current_task().unwrap());
            drop(inner);
            block_current_and_run_next();
        } else {
            *inner.allocations.entry(current_tid()).or_insert(0) += 1;
        }
    }

    /// Snapshot nonnegative free units, grants and blocked requests.
    pub fn snapshot(&self, threads: usize) -> Snapshot {
        let inner = self.inner.exclusive_access();
        let mut snapshot = Snapshot::new(threads, inner.count.max(0) as usize);
        for (tid, count) in &inner.allocations {
            if *tid < threads { snapshot.allocation[*tid] = *count; }
        }
        for task in &inner.wait_queue {
            snapshot.need[task.inner_exclusive_access().res.as_ref().unwrap().tid] = 1;
        }
        snapshot
    }
}
