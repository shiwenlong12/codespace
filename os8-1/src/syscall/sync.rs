use crate::sync::{Condvar, Mutex, MutexBlocking, MutexSpin, Semaphore};
use crate::task::{block_current_and_run_next, current_process, current_task};
use crate::timer::{add_timer, get_time_ms};
use alloc::sync::Arc;
pub fn sys_sleep(ms: usize) -> isize {
    let expire_ms = get_time_ms() + ms;
    let task = current_task().unwrap();
    add_timer(expire_ms, task);
    block_current_and_run_next();
    0
}

// LAB5 HINT: you might need to maintain data structures used for deadlock detection
// during sys_mutex_* and sys_semaphore_* syscalls
pub fn sys_mutex_create(blocking: bool) -> isize {
    let process = current_process();
    let mutex: Option<Arc<dyn Mutex>> = if !blocking {
        Some(Arc::new(MutexSpin::new()))
    } else {
        Some(Arc::new(MutexBlocking::new()))
    };
    let mut process_inner = process.inner_exclusive_access();
    if let Some(id) = process_inner
        .mutex_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.mutex_list[id] = mutex;
        process_inner.available[id] = 1;
        for v in process_inner.need.iter_mut() {
            (*v)[id] = 0;
        }
        for v in process_inner.allocation.iter_mut() {
            (*v)[id] = 0;
        }
        id as isize
    } else {
        process_inner.mutex_list.push(mutex);
        process_inner.available.push(1);
        for v in process_inner.need.iter_mut() {
            (*v).push(0);
        }
        for v in process_inner.allocation.iter_mut() {
            (*v).push(0);
        }
        process_inner.mutex_list.len() as isize - 1
    }
}

// LAB5 HINT: Return -0xDEAD if deadlock is detected
pub fn sys_mutex_lock(mutex_id: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    process_inner.id = mutex_id as i32;
    let task = current_task().unwrap();
    let task_inner = task.inner_exclusive_access();
    let tid = task_inner.res.as_ref().unwrap().tid;
    drop(task_inner);
    drop(task);
    process_inner.need[tid][mutex_id] += 1;
    if process_inner.deadlock_det {
        if process_inner.deadlock_detect() {
            drop(process_inner);
            drop(process);
            -0xDEAD
        }
        else {
            let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
            //process_inner.available[mutex_id] -= 1;
            drop(process_inner);
            drop(process);
            mutex.lock();
            0
        }
    }
    else {
        let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
        //process_inner.available[mutex_id] -= 1;
        drop(process_inner);
        drop(process);
        mutex.lock();
        0
    }
}

pub fn sys_mutex_unlock(mutex_id: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    process_inner.available[mutex_id] += 1;
    let task = current_task().unwrap();
    let task_inner = task.inner_exclusive_access();
    let tid = task_inner.res.as_ref().unwrap().tid;
    process_inner.allocation[tid][mutex_id] -= 1;
    drop(process_inner);
    drop(process);
    mutex.unlock();
    0
}

pub fn sys_semaphore_create(res_count: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .semaphore_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.semaphore_list[id] = Some(Arc::new(Semaphore::new(res_count)));
        process_inner._available[id] = res_count as i32;
        for v in process_inner._need.iter_mut() {
            (*v)[id] = 0;
        }
        for v in process_inner._allocation.iter_mut() {
            (*v)[id] = 0;
        }
        id
    } else {
        process_inner
            .semaphore_list
            .push(Some(Arc::new(Semaphore::new(res_count))));
        process_inner._available.push(res_count as i32);
        //process_inner._need.iter_mut().map(|v| (*v).push(0));
        for v in process_inner._need.iter_mut() {
            (*v).push(0);
        }
        for v in process_inner._allocation.iter_mut() {
            (*v).push(0);
        }
        process_inner.semaphore_list.len() - 1
    };
    id as isize
}

pub fn sys_semaphore_up(sem_id: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    process_inner.id = sem_id as i32;
    process_inner._available[sem_id] += 1;
    let task = current_task().unwrap();
    let task_inner = task.inner_exclusive_access();
    let tid = task_inner.res.as_ref().unwrap().tid;
    process_inner._allocation[tid][sem_id] -= 1;
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    drop(process_inner);
    drop(process);
    drop(task_inner);
    drop(task);
    sem.up();
    0
}

// LAB5 HINT: Return -0xDEAD if deadlock is detected
pub fn sys_semaphore_down(sem_id: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    process_inner.id = sem_id as i32;
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    let task = current_task().unwrap();
    let task_inner = task.inner_exclusive_access();
    let tid = task_inner.res.as_ref().unwrap().tid;
    drop(task_inner);
    drop(task);
    process_inner._need[tid][sem_id] += 1;
    if process_inner.deadlock_det {
        if process_inner.deadlock_detect() {
            drop(process_inner);
            drop(process);
            -0xDEAD
        }
        else {
            //process_inner._available[sem_id] -= 1;
            drop(process_inner);
            drop(process);
            sem.down();
            0
        }
    }
    else {
        //process_inner._available[sem_id] -= 1;
        drop(process_inner);
        sem.down();
        0
    }
}

pub fn sys_condvar_create(_arg: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .condvar_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.condvar_list[id] = Some(Arc::new(Condvar::new()));
        id
    } else {
        process_inner
            .condvar_list
            .push(Some(Arc::new(Condvar::new())));
        process_inner.condvar_list.len() - 1
    };
    id as isize
}

pub fn sys_condvar_signal(condvar_id: usize) -> isize {
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    drop(process_inner);
    condvar.signal();
    0
}

pub fn sys_condvar_wait(condvar_id: usize, mutex_id: usize) -> isize {
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    condvar.wait(mutex);
    0
}

// LAB5 YOUR JOB: Implement deadlock detection, but might not all in this syscall
pub fn sys_enable_deadlock_detect(_enabled: usize) -> isize {
    if _enabled == 1 {
        let process = current_process();
        let mut process_inner = process.inner_exclusive_access();
        process_inner.deadlock_det = true;
        0
    }
    else if _enabled == 0 {
        let process = current_process();
        let mut process_inner = process.inner_exclusive_access();
        process_inner.deadlock_det = false;
        0
    }
    else {
        -1
    }
}
