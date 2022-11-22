use crate::sync::UPSafeCell;
use crate::task::{add_task, block_current_and_run_next, current_task, TaskControlBlock, current_process};
use alloc::{collections::VecDeque, sync::Arc};

pub struct Semaphore {
    pub inner: UPSafeCell<SemaphoreInner>,
}

pub struct SemaphoreInner {
    pub count: isize,
    pub wait_queue: VecDeque<Arc<TaskControlBlock>>,
}

impl Semaphore {
    pub fn new(res_count: usize) -> Self {
        Self {
            inner: unsafe {
                UPSafeCell::new(SemaphoreInner {
                    count: res_count as isize,
                    wait_queue: VecDeque::new(),
                })
            },
        }
    }

    pub fn up(&self) {
        let process = current_process();
        let mut process_inner = process.inner_exclusive_access();
        let id = process_inner.get_id() as usize;
        let task = current_task().unwrap();
        let task_inner = task.inner_exclusive_access();
        let tid = task_inner.res.as_ref().unwrap().tid;
        let mut inner = self.inner.exclusive_access();
        inner.count += 1;
        process_inner._allocation[tid][id] -= 1;
        process_inner._available[id] += 1;
        drop(process_inner);
        drop(process);
        drop(task_inner);
        drop(task);
        if inner.count <= 0 {
            if let Some(task) = inner.wait_queue.pop_front() {
                add_task(task);
            }
        }
    }

    pub fn down(&self) {
        let process = current_process();
        let mut process_inner = process.inner_exclusive_access();
        let id = process_inner.get_id() as usize;
        let task = current_task().unwrap();
        let task_inner = task.inner_exclusive_access();
        let tid = task_inner.res.as_ref().unwrap().tid;
        //process_inner._need[tid][id] += 1;
        let mut inner = self.inner.exclusive_access();
        inner.count -= 1;
        if inner.count < 0 {
            inner.wait_queue.push_back(current_task().unwrap());
            drop(inner);
            drop(process_inner);
            drop(task_inner);
            block_current_and_run_next();
        }
        else {
            process_inner._available[id] -= 1;
            process_inner._need[tid][id] -= 1;
            process_inner._allocation[tid][id] += 1;
            drop(inner);
            drop(process_inner);
            drop(task_inner);
        }
    }
}
