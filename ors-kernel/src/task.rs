use crate::context::{Context, EntryPoint};
use crate::cpu::Cpu;
use crate::mutex::Mutex;
use crate::x64;
use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Lazy;

const DEFAULT_STACK_SIZE: usize = 4096 * 256; // 1MiB

static TASK_MANAGER: Lazy<TaskManager> = Lazy::new(|| TaskManager::new());

pub fn task_manager() -> &'static TaskManager {
    &*TASK_MANAGER
}

#[derive(Debug)]
pub struct TaskManager {
    queue: Mutex<TaskQueue>,
    task_id_gen: AtomicU64,
}

impl TaskManager {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(TaskQueue::new()),
            task_id_gen: AtomicU64::new(0),
        }
    }

    fn issue_task_id(&self) -> TaskId {
        TaskId(self.task_id_gen.fetch_add(1, Ordering::SeqCst))
    }

    pub fn add(
        &self,
        priority: Priority,
        entry_point: extern "C" fn(u64) -> !,
        entry_arg: u64,
    ) -> TaskId {
        let id = self.issue_task_id();
        let entry_point = TaskEntryPoint(entry_point);
        let task = Task::new(id, priority, entry_point, entry_arg);
        self.queue.lock().enqueue(task);
        id
    }

    pub unsafe fn switch(&self, sleep: Option<Chan>) {
        assert!(
            !x64::interrupts::are_enabled(),
            "TaskManager::switch must be called with interrupts disabled"
        );
        let cpu_info = Cpu::current().info();

        let cpu_task = cpu_info.lock().running_task.take();
        let cpu_task =
            cpu_task.unwrap_or_else(|| Task::new_current(self.issue_task_id(), Priority::MIN));
        // FIXME: This implicitly relies on the fact that cpu_task is retained by a TaskQueue
        let current_ctx = cpu_task.ctx().get();

        let cpu_task = self.queue.lock().dequeue(cpu_task, sleep);
        let next_ctx = cpu_task.ctx().get();
        assert!(cpu_info.lock().running_task.replace(cpu_task).is_none());

        assert_eq!(cpu_info.lock().ncli, 0); // We don't need to save and restore cpu_info.zcli
        if current_ctx != next_ctx {
            Context::switch(next_ctx, current_ctx);
        }
    }

    pub fn wakeup(&self, chan: Chan) {
        self.queue.lock().wakeup(chan);
    }
}

#[derive(Debug)]
struct TaskQueue {
    sleeping_tasks: Vec<(Chan, Task)>, // sleeping on chan (TODO)
    runnable_tasks: [VecDeque<Task>; Priority::SIZE],
}

impl TaskQueue {
    fn new() -> Self {
        let mut runnable_tasks = MaybeUninit::uninit_array();
        for tasks in &mut runnable_tasks[..] {
            tasks.write(VecDeque::new());
        }
        Self {
            sleeping_tasks: Vec::new(),
            runnable_tasks: unsafe { MaybeUninit::array_assume_init(runnable_tasks) },
        }
    }

    fn enqueue(&mut self, task: Task) {
        self.runnable_tasks[task.priority().index()].push_back(task);
    }

    /// Dequeuing requires a task that is currently running.
    fn dequeue(&mut self, current_task: Task, current_sleep: Option<Chan>) -> Task {
        let minimum_level_index = match current_sleep {
            Some(_) => 0,
            None => current_task.priority().index(), // current_task is still runnable
        };

        // next_task is runnable, has the highest priority, and is at the front of the queue
        if let Some(next_task) = self
            .runnable_tasks
            .iter_mut()
            .enumerate()
            .rev()
            .take_while(|(i, _)| minimum_level_index <= *i)
            .find_map(|(_, queue)| queue.pop_front())
        {
            // current_task.ctx will be saved "after" dequeuing:
            // TaskManager::switch -> Context::switch -> switch_context (asm.s)
            unsafe { &*current_task.ctx().get() }.mark_as_not_saved();

            if let Some(chan) = current_sleep {
                self.sleeping_tasks.push((chan, current_task));
            } else {
                self.runnable_tasks[current_task.priority().index()].push_back(current_task);
            }

            unsafe { &*next_task.ctx().get() }.wait_saved();
            next_task
        } else {
            current_task // There are no tasks to switch
        }
    }

    fn wakeup(&mut self, chan: Chan) {
        for (_, task) in self.sleeping_tasks.drain_filter(|(c, _)| chan == *c) {
            self.runnable_tasks[task.priority().index()].push_back(task);
        }
    }
}

pub type Chan = (); // TODO

#[repr(transparent)]
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy, Hash)]
pub struct TaskId(u64);

#[derive(Debug)]
pub struct Task(Box<TaskData>);

impl Task {
    fn new(id: TaskId, priority: Priority, entry_point: TaskEntryPoint, entry_arg: u64) -> Self {
        let mut stack = vec![0; DEFAULT_STACK_SIZE].into_boxed_slice();
        let stack_end = unsafe { stack.as_mut_ptr().add(DEFAULT_STACK_SIZE) };
        let ctx = Context::new(stack_end, entry_point, (id, entry_arg));
        Self(Box::new(TaskData {
            id,
            priority,
            stack,
            ctx: UnsafeCell::new(ctx),
        }))
    }

    /// Used to treat a context that is currently running as a task.
    fn new_current(id: TaskId, priority: Priority) -> Self {
        Self(Box::new(TaskData {
            id,
            priority,
            stack: Default::default(),
            ctx: UnsafeCell::new(Context::uninitialized()),
        }))
    }

    pub fn id(&self) -> TaskId {
        self.0.id
    }

    pub fn priority(&self) -> Priority {
        self.0.priority
    }

    fn ctx(&self) -> &UnsafeCell<Context> {
        &self.0.ctx
    }
}

#[derive(Debug)]
struct TaskData {
    id: TaskId,
    priority: Priority,
    stack: Box<[u8]>,
    ctx: UnsafeCell<Context>,
}

#[derive(Debug)]
struct TaskEntryPoint(extern "C" fn(u64) -> !);

impl EntryPoint for TaskEntryPoint {
    type Arg = (TaskId, u64);

    fn prepare_context(self, ctx: &mut Context, arg: Self::Arg) {
        ctx.rip = task_init as u64;
        ctx.rdi = self.0 as u64;
        ctx.rsi = arg.0 .0;
        ctx.rdx = arg.1;
    }
}

extern "C" fn task_init(f: extern "C" fn(u64) -> !, _: TaskId, task_arg: u64) -> ! {
    // TODO: Some initialization routine?
    f(task_arg)
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy, Hash)]
pub enum Priority {
    L0,
    L1,
    L2,
    L3,
}

impl Priority {
    pub fn index(self) -> usize {
        match self {
            Self::L0 => 0,
            Self::L1 => 1,
            Self::L2 => 2,
            Self::L3 => 3,
        }
    }

    pub const MIN: Self = Self::L0;
    pub const MAX: Self = Self::L3;
    pub const SIZE: usize = 4;
}
