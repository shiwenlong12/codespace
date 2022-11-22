//! Task management implementation
//!
//! Everything about task management, like starting and switching tasks is
//! implemented here.
//!
//! A single global instance of [`TaskManager`] called `TASK_MANAGER` controls
//! all the tasks in the operating system.
//!
//! Be careful when you see [`__switch`]. Control flow around this function
//! might not be what you expect.

mod context;
mod switch;
#[allow(clippy::module_inception)]
mod task;

use crate::loader::{get_app_data, get_num_app};
use crate::sync::UPSafeCell;
use crate::trap::TrapContext;
use alloc::vec::Vec;
use lazy_static::*;

use crate::config::PAGE_SIZE;
use crate::mm::{MapPermission, VirtAddr};
use::crate::syscall::TaskInfo;
use crate::timer::get_time_us;
use crate::console;

pub use switch::__switch;
pub use task::{TaskControlBlock, TaskStatus};

pub use context::TaskContext;

/// The task manager, where all the tasks are managed.
///
/// Functions implemented on `TaskManager` deals with all task state transitions
/// and task context switching. For convenience, you can find wrappers around it
/// in the module level.
///
/// Most of `TaskManager` are hidden behind the field `inner`, to defer
/// borrowing checks to runtime. You can see examples on how to use `inner` in
/// existing functions on `TaskManager`.
pub struct TaskManager {
    /// total number of tasks
    num_app: usize,
    /// use inner value to get mutable access
    inner: UPSafeCell<TaskManagerInner>,
}

/// The task manager inner in 'UPSafeCell'
struct TaskManagerInner {
    /// task list
    tasks: Vec<TaskControlBlock>,
    /// id of current `Running` task
    current_task: usize,
}

lazy_static! {
    /// a `TaskManager` instance through lazy_static!
    pub static ref TASK_MANAGER: TaskManager = {
        info!("init TASK_MANAGER");
        let num_app = get_num_app();
        info!("num_app = {}", num_app);
        let mut tasks: Vec<TaskControlBlock> = Vec::new();
        for i in 0..num_app {
            tasks.push(TaskControlBlock::new(get_app_data(i), i));
        }
        TaskManager {
            num_app,
            inner: unsafe {
                UPSafeCell::new(TaskManagerInner {
                    tasks,
                    current_task: 0,
                })
            },
        }
    };
}

impl TaskManager {
    /// Run the first task in task list.
    ///
    /// Generally, the first task in task list is an idle task (we call it zero process later).
    /// But in ch4, we load apps statically, so the first task is a real app.
    fn run_first_task(&self) -> ! {
        let mut inner = self.inner.exclusive_access();
        let next_task = &mut inner.tasks[0];
        next_task.task_status = TaskStatus::Running;
        let next_task_cx_ptr = &next_task.task_cx as *const TaskContext;
        drop(inner);
        let mut _unused = TaskContext::zero_init();
        // before this, we should drop local variables that must be dropped manually
        unsafe {
            __switch(&mut _unused as *mut _, next_task_cx_ptr);
        }
        panic!("unreachable in run_first_task!");
    }

    /// Change the status of current `Running` task into `Ready`.
    fn mark_current_suspended(&self) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].task_status = TaskStatus::Ready;
    }

    /// Change the status of current `Running` task into `Exited`.
    fn mark_current_exited(&self) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].task_status = TaskStatus::Exited;
    }

    /// Find next task to run and return task id.
    ///
    /// In this case, we only return the first `Ready` task in task list.
    fn find_next_task(&self) -> Option<usize> {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        (current + 1..current + self.num_app + 1)
            .map(|id| id % self.num_app)
            .find(|id| inner.tasks[*id].task_status == TaskStatus::Ready)
    }

    /// Get the current 'Running' task's token.
    fn get_current_token(&self) -> usize {
        let inner = self.inner.exclusive_access();
        inner.tasks[inner.current_task].get_user_token()
    }

    #[allow(clippy::mut_from_ref)]
    /// Get the current 'Running' task's trap contexts.
    fn get_current_trap_cx(&self) -> &mut TrapContext {
        let inner = self.inner.exclusive_access();
        inner.tasks[inner.current_task].get_trap_cx()
    }

    /// Switch current `Running` task to the task we have found,
    /// or there is no `Ready` task and we can exit with all applications completed
    fn run_next_task(&self) {
        if let Some(next) = self.find_next_task() {
            let mut inner = self.inner.exclusive_access();
            let current = inner.current_task;
            inner.tasks[next].task_status = TaskStatus::Running;
            inner.current_task = next;
            let current_task_cx_ptr = &mut inner.tasks[current].task_cx as *mut TaskContext;
            let next_task_cx_ptr = &inner.tasks[next].task_cx as *const TaskContext;
            drop(inner);
            // before this, we should drop local variables that must be dropped manually
            unsafe {
                __switch(current_task_cx_ptr, next_task_cx_ptr);
            }
            // go back to user mode
        } else {
            panic!("All applications completed!");
        }
    }
    fn update_syscall_times(&self, syscall_id: usize) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].syscall_times[syscall_id] += 1;
    }

    fn get_task_info(&self) -> TaskInfo {
        let inner = self.inner.exclusive_access();
        let current_task_re = &inner.tasks[inner.current_task];
        TaskInfo {
            status: current_task_re.task_status,
            syscall_times: current_task_re.syscall_times,
            time: get_time_us() / 1000 - current_task_re.start_time,
        }
    }
}

/// Run the first task in task list.
pub fn run_first_task() {
    TASK_MANAGER.run_first_task();
}

/// Switch current `Running` task to the task we have found,
/// or there is no `Ready` task and we can exit with all applications completed
fn run_next_task() {
    TASK_MANAGER.run_next_task();
}

/// Change the status of current `Running` task into `Ready`.
fn mark_current_suspended() {
    TASK_MANAGER.mark_current_suspended();
}

/// Change the status of current `Running` task into `Exited`.
fn mark_current_exited() {
    TASK_MANAGER.mark_current_exited();
}

/// Suspend the current 'Running' task and run the next task in task list.
pub fn suspend_current_and_run_next() {
    mark_current_suspended();
    run_next_task();
}

/// Exit the current 'Running' task and run the next task in task list.
pub fn exit_current_and_run_next() {
    mark_current_exited();
    run_next_task();
}

/// Get the current 'Running' task's token.
pub fn current_user_token() -> usize {
    TASK_MANAGER.get_current_token()
}

/// Get the current 'Running' task's trap contexts.
pub fn current_trap_cx() -> &'static mut TrapContext {
    TASK_MANAGER.get_current_trap_cx()
}

pub fn update_syscall_times(syscall_id: usize) {
    TASK_MANAGER.update_syscall_times(syscall_id);
}

pub fn get_task_info() -> TaskInfo {
    TASK_MANAGER.get_task_info()
}

pub fn mmap(_start: usize, _len: usize, _port: usize) -> isize {
    if _len == 0 {
        0
    }
    else if (_port >> 3) != 0 || (_port & 0x7) == 0 || _start % 4096 != 0 {
        -1
    }
    else {
        let mut inner = TASK_MANAGER.inner.exclusive_access();
        let current = inner.current_task;
        let l: VirtAddr = _start.into();
        let r: VirtAddr = (_start + _len).into();
        let lvpn = l.floor();
        let rvpn = r.ceil();
        for area in &inner.tasks[current].memory_set.areas {
            if lvpn <= area.vpn_range.get_start() && rvpn > area.vpn_range.get_start() {
                return -1;
            }
        }
        let mut permission = MapPermission::from_bits((_port as u8) << 1).unwrap();
        permission.set(MapPermission::U, true);
        let mut start = _start;
        let end = start + _len;
        while start < end {
            let endr;
            if start + PAGE_SIZE > end {
                endr = end;
            }
            else {
                endr = start + PAGE_SIZE; 
            }
            inner.tasks[current].memory_set.insert_framed_area(start.into(), endr.into(), permission);
            start += PAGE_SIZE;
        }
        0
    }

}
pub fn munmap(_start: usize, _len: usize) -> isize {
    if _len == 0 {
        0
    }
    else if _start % 4096 != 0 {
        -1
    }
    else {
        let mut inner = TASK_MANAGER.inner.exclusive_access();
        let current = inner.current_task;
        let l: VirtAddr = _start.into();
        let r: VirtAddr = (_start + _len).into();
        let lvpn = l.floor();
        let rvpn = r.ceil();
        let mut cnt = 0;
        for area in &inner.tasks[current].memory_set.areas {
            if lvpn <= area.vpn_range.get_start() && rvpn >= area.vpn_range.get_start()
            {
                cnt += 1;
            }
        }
        if cnt < rvpn.0 - lvpn.0 {
            -1
        }
        else {
            let arealen = inner.tasks[current].memory_set.areas.len();
            let memory_set = &mut inner.tasks[current].memory_set;
            for i in 0..arealen {
                if !memory_set.areas.get(i).is_some() {
                    continue;
                }
                if lvpn <= memory_set.areas[i].vpn_range.get_start() && rvpn > memory_set.areas[i].vpn_range.get_start() {
                    memory_set.areas[i].unmap(&mut memory_set.page_table);
                    memory_set.areas.remove(i);
                }
            }
            0
        }
    }
}