//! Process management syscalls
// AI 学习协助：本文件的实验增量由 AI 助手协助实现，逐处说明见 Study/修改记录。
// 参考：https://learningos.cn/rCore-Tutorial-Guide/chapter4/；原始代码来自课程模板，未改动官方评测器。
use crate::task::{change_program_brk, exit_current_and_run_next, suspend_current_and_run_next};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    let us = crate::timer::get_time_us();
    let value = TimeVal { sec: us / 1_000_000, usec: us % 1_000_000 };
    let bytes = unsafe { core::slice::from_raw_parts(
        &value as *const TimeVal as *const u8, core::mem::size_of::<TimeVal>()) };
    if crate::mm::copy_to_user(crate::task::current_user_token(), ts as usize, bytes) { 0 } else { -1 }
}

/// TODO: Finish sys_trace to pass testcases
/// HINT: You might reimplement it with virtual memory management.
pub fn sys_trace(trace_request: usize, id: usize, data: usize) -> isize {
    if trace_request == 2 { return crate::task::syscall_count(id) as isize; }
    if trace_request > 1 { return -1; }
    let Some(mut buffers) = crate::mm::checked_user_buffer(
        crate::task::current_user_token(), id, 1, trace_request == 1) else { return -1; };
    if trace_request == 0 { buffers[0][0] as isize }
    else { buffers[0][0] = data as u8; 0 }
}

// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, prot: usize) -> isize {
    crate::task::mmap_current(start, len, prot)
}

// YOUR JOB: Implement munmap.
pub fn sys_munmap(start: usize, len: usize) -> isize {
    crate::task::munmap_current(start, len)
}
/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
