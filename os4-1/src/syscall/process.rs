//! Process management syscalls

use crate::config::MAX_SYSCALL_NUM;
use crate::task::{
    exit_current_and_run_next, suspend_current_and_run_next, TaskStatus,
    current_user_token, get_task_info, malloc, mfree
};
use crate::timer::get_time_us;
use crate::mm::{VirtAddr, PhysAddr, PageTable};
#[macro_use]
use crate::console;
#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

#[derive(Clone, Copy)]
pub struct TaskInfo {
    pub status: TaskStatus,
    pub syscall_times: [u32; MAX_SYSCALL_NUM],
    pub time: usize,
}

pub fn sys_exit(exit_code: i32) -> ! {
    info!("[kernel] Application exited with code {}", exit_code);
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    suspend_current_and_run_next();
    0
}

// YOUR JOB: 引入虚地址后重写 sys_get_time
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    let _us = get_time_us();
    let va = VirtAddr::from(_ts as usize);
    let vpn = va.floor();
    let token = current_user_token();
    let page_table = PageTable::from_token(token);
    let ppn = page_table.translate(vpn).unwrap().ppn();
    let offset = va.page_offset();
    let sec = _us / 1000000;
    let usec = _us % 1000000;
    let pa: PhysAddr = ppn.into();
    unsafe {
        let time = ((pa.0 + offset) as *mut TimeVal).as_mut().unwrap();
        *time = TimeVal {
            sec: sec,
            usec: usec,
        }
    }    
    0
}

// CLUE: 从 ch4 开始不再对调度算法进行测试~
pub fn sys_set_priority(_prio: isize) -> isize {
    -1
}

// YOUR JOB: 扩展内核以实现 sys_mmap 和 sys_munmap
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    malloc(_start, _len, _port)
}

pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    mfree(_start, _len)
}

// YOUR JOB: 引入虚地址后重写 sys_task_info
pub fn sys_task_info(_ti: *mut TaskInfo) -> isize {
    let va = VirtAddr::from(_ti as usize);
    let vpn = va.floor();
    let token = current_user_token();
    let page_table = PageTable::from_token(token);
    let ppn = page_table.translate(vpn).unwrap().ppn();
    let offset = va.page_offset();
    let pa: PhysAddr = ppn.into();
    unsafe {
        let task_info = ((pa.0 + offset) as *mut TaskInfo).as_mut().unwrap();
        let tmp = get_task_info();
        *task_info = tmp;
    }
    0
}
