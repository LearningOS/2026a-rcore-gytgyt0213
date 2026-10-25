//!Implementation of [`TaskManager`]
// AI 学习协助：本文件的实验增量由 AI 助手协助实现，逐处说明见 Study/修改记录。
// 参考：https://learningos.cn/rCore-Tutorial-Guide/chapter5/；原始代码来自课程模板，未改动官方评测器。
use super::TaskControlBlock;
use crate::sync::UPSafeCell;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use lazy_static::*;
///A array of `TaskControlBlock` that is thread-safe
pub struct TaskManager {
    ready_queue: VecDeque<Arc<TaskControlBlock>>,
}

/// Choose the ready task with the least accumulated virtual runtime.
impl TaskManager {
    ///Creat an empty TaskManager
    pub fn new() -> Self {
        Self {
            ready_queue: VecDeque::new(),
        }
    }
    /// Add process back to ready queue
    pub fn add(&mut self, task: Arc<TaskControlBlock>) {
        self.ready_queue.push_back(task);
    }
    /// Take a process out of the ready queue
    pub fn fetch(&mut self) -> Option<Arc<TaskControlBlock>> {
        let (index, minimum) = self.ready_queue.iter().enumerate()
            .map(|(i, task)| (i, task.inner_exclusive_access().stride))
            .min_by_key(|(_, stride)| *stride)?;
        // Rebase only near u128 overflow; all runnable tasks keep their order.
        if minimum > u128::MAX / 2 {
            for task in &self.ready_queue {
                task.inner_exclusive_access().stride -= minimum;
            }
        }
        let task = self.ready_queue.remove(index).unwrap();
        {
            let mut inner = task.inner_exclusive_access();
            let pass = ((1u128 << 32) / inner.priority as u128).max(1);
            inner.stride += pass;
        }
        Some(task)
    }
}

lazy_static! {
    /// TASK_MANAGER instance through lazy_static!
    pub static ref TASK_MANAGER: UPSafeCell<TaskManager> =
        unsafe { UPSafeCell::new(TaskManager::new()) };
}

/// Add process to ready queue
pub fn add_task(task: Arc<TaskControlBlock>) {
    //trace!("kernel: TaskManager::add_task");
    TASK_MANAGER.exclusive_access().add(task);
}

/// Take a process out of the ready queue
pub fn fetch_task() -> Option<Arc<TaskControlBlock>> {
    //trace!("kernel: TaskManager::fetch_task");
    TASK_MANAGER.exclusive_access().fetch()
}
