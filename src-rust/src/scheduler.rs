// scheduler.rs - Task Scheduler for IndigoLispOS
// Simple round-robin cooperative scheduler

use alloc::vec::Vec;
use alloc::vec;
use alloc::boxed::Box;
use core::sync::atomic::{AtomicUsize, Ordering};

const TASK_STACK_SIZE: usize = 8192; // 8KB per task

// Task states
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TaskState {
    Ready,
    Running,
    Blocked,
    Terminated,
}

// CPU context saved/restored on context switch
#[repr(C)]
#[derive(Debug, Clone)]
pub struct TaskContext {
    pub x19: u64,
    pub x20: u64,
    pub x21: u64,
    pub x22: u64,
    pub x23: u64,
    pub x24: u64,
    pub x25: u64,
    pub x26: u64,
    pub x27: u64,
    pub x28: u64,
    pub x29: u64, // FP
    pub x30: u64, // LR
    pub sp: u64,
}

impl TaskContext {
    pub fn new() -> Self {
        Self {
            x19: 0, x20: 0, x21: 0, x22: 0, x23: 0, x24: 0,
            x25: 0, x26: 0, x27: 0, x28: 0, x29: 0, x30: 0,
            sp: 0,
        }
    }
}

// Task Control Block
pub struct Task {
    pub id: usize,
    pub state: TaskState,
    pub context: TaskContext,
    pub stack: Box<[u8]>,
    pub name: &'static str,
}

impl Task {
    pub fn new(id: usize, entry: extern "C" fn(), name: &'static str) -> Self {
        let mut stack = vec![0u8; TASK_STACK_SIZE].into_boxed_slice();
        let stack_top = stack.as_ptr() as usize + TASK_STACK_SIZE;
        
        let mut context = TaskContext::new();
        context.sp = stack_top as u64;
        context.x30 = entry as u64; // LR points to entry function
        
        Self {
            id,
            state: TaskState::Ready,
            context,
            stack,
            name,
        }
    }
}

// Global scheduler state
pub struct Scheduler {
    tasks: Vec<Task>,
    current_task: AtomicUsize,
}

impl Scheduler {
    pub const fn new() -> Self {
        Self {
            tasks: Vec::new(),
            current_task: AtomicUsize::new(0),
        }
    }
    
    pub fn init(&mut self) {
        // Create idle task (task 0)
        extern "C" fn idle_task() {
            loop {
                unsafe {
                    core::arch::asm!("wfe");
                }
            }
        }
        
        let idle = Task::new(0, idle_task, "idle");
        self.tasks.push(idle);
        self.tasks[0].state = TaskState::Running;
    }
    
    pub fn spawn(&mut self, entry: extern "C" fn(), name: &'static str) -> usize {
        let id = self.tasks.len();
        let task = Task::new(id, entry, name);
        self.tasks.push(task);
        id
    }
    
    pub fn get_current_task(&self) -> usize {
        self.current_task.load(Ordering::Relaxed)
    }
    
    pub fn schedule(&mut self) {
        let current = self.current_task.load(Ordering::Relaxed);
        
        // Find next ready task (round-robin)
        let mut next = (current + 1) % self.tasks.len();
        while next != current {
            if self.tasks[next].state == TaskState::Ready {
                break;
            }
            next = (next + 1) % self.tasks.len();
        }
        
        if next == current {
            return; // No other ready tasks
        }
        
        // Switch tasks
        self.tasks[current].state = TaskState::Ready;
        self.tasks[next].state = TaskState::Running;
        self.current_task.store(next, Ordering::Relaxed);
        
        // Perform context switch
        unsafe {
            switch_context(
                &mut self.tasks[current].context as *mut TaskContext,
                &self.tasks[next].context as *const TaskContext,
            );
        }
    }
}

// External assembly function for context switching
extern "C" {
    fn switch_context(old: *mut TaskContext, new: *const TaskContext);
}

// Global scheduler instance
static mut SCHEDULER: Scheduler = Scheduler::new();

pub fn init() {
    unsafe {
        SCHEDULER.init();
    }
}

pub fn spawn(entry: extern "C" fn(), name: &'static str) -> usize {
    unsafe {
        SCHEDULER.spawn(entry, name)
    }
}

pub fn schedule() {
    unsafe {
        SCHEDULER.schedule();
    }
}

pub fn get_current_task() -> usize {
    unsafe {
        SCHEDULER.get_current_task()
    }
}
