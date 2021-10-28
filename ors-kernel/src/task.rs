use crate::context::{Context, EntryPoint};
use crate::cpu::Cpu;
use crate::interrupts::{ticks, Cli};
use crate::sync::mutex::{Mutex, MutexGuard};
use alloc::boxed::Box;
use alloc::collections::{BTreeMap, BinaryHeap, VecDeque};
use alloc::vec;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::cmp::Reverse;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicU64, Ordering};
use log::trace;
use spin::Once;

const DEFAULT_STACK_SIZE: usize = 4096 * 256; // 1MiB

static SCHEDULER: Once<TaskScheduler> = Once::new();

pub fn initialize_scheduler() {
    SCHEDULER.call_once(|| {
        trace!("INITIALIZING Task Scheduler");
        TaskScheduler::new()
    });
}

pub fn scheduler() -> &'static TaskScheduler {
    SCHEDULER
        .get()
        .expect("task::scheduler is called before task::initialize_scheduler")
}

#[derive(Debug)]
pub struct TaskScheduler {
    queue: Mutex<TaskQueue>,
    task_id_gen: AtomicU64,
    wait_channel_gen: AtomicU64,
}

impl TaskScheduler {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(TaskQueue::new()),
            task_id_gen: AtomicU64::new(0),
            wait_channel_gen: AtomicU64::new(0),
        }
    }

    fn issue_task_id(&self) -> TaskId {
        TaskId(self.task_id_gen.fetch_add(1, Ordering::SeqCst))
    }

    pub fn issue_wait_channel(&self) -> WaitChannel {
        WaitChannel(self.wait_channel_gen.fetch_add(1, Ordering::SeqCst))
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

    pub fn switch<T>(
        &self,
        scheduling_op: impl FnOnce() -> (Option<Switch>, T),
        other_cli: u32,
    ) -> T {
        let cli = Cli::new(); // (*1)

        let cpu_state = Cpu::current().state();
        assert_eq!(cpu_state.lock().thread_state.ncli, 1 + other_cli); // To ensure that this context does not hold locks (*1)

        let cpu_task = {
            // This assignment is necessary to avoid deadlocks
            let task = cpu_state.lock().running_task.take();
            task.unwrap_or_else(|| Task::new_current(self.issue_task_id(), Priority::MIN))
        };
        // FIXME: This implicitly relies on the fact that cpu_task is retained (not dropped) by self.queue
        let current_ctx = cpu_task.ctx().get();

        let (cpu_task, ret) = {
            let mut queue_lock = self.queue.lock();
            // scheduling_op is called while self.queue is locked
            let (switch, ret) = scheduling_op();
            let task = match switch {
                Some(switch) => queue_lock.dequeue(cpu_task, switch),
                None => cpu_task,
            };
            (task, ret)
        };
        let next_ctx = cpu_task.ctx().get();
        assert!(cpu_state.lock().running_task.replace(cpu_task).is_none());

        if current_ctx != next_ctx {
            unsafe { Context::switch(next_ctx, current_ctx) };
        }

        drop(cli);
        ret
    }

    pub fn r#yield(&self) {
        self.switch(|| (Some(Switch::Yield), ()), 0)
    }

    /// Atomically release MutexGuard and block on chan.
    pub fn block<T>(&self, chan: WaitChannel, timeout: Option<usize>, guard: MutexGuard<'_, T>) {
        self.switch(
            move || {
                drop(guard);
                (Some(Switch::Blocked(chan, timeout)), ())
            },
            1,
        )
    }

    pub fn sleep(&self, ticks: usize) {
        self.switch(|| (Some(Switch::Sleep(ticks)), ()), 0)
    }

    pub fn release(&self, chan: WaitChannel) {
        self.queue.lock().release(chan);
    }

    pub fn elapse(&self) {
        self.queue.lock().elapse();
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Switch {
    Blocked(WaitChannel, Option<usize>),
    Sleep(usize),
    Yield,
}

#[derive(Debug)]
struct TaskQueue {
    pending_id_gen: u64,
    runnable_tasks: [VecDeque<Task>; Priority::SIZE],
    pending_tasks: BTreeMap<PendingId, Task>,
    blocks: BTreeMap<WaitChannel, Vec<PendingId>>,
    timeouts: BinaryHeap<Reverse<(usize, PendingId, Option<WaitChannel>)>>,
}

impl TaskQueue {
    fn new() -> Self {
        let mut runnable_tasks = MaybeUninit::uninit_array();
        for tasks in &mut runnable_tasks[..] {
            tasks.write(VecDeque::new());
        }
        Self {
            pending_id_gen: 0,
            runnable_tasks: unsafe { MaybeUninit::array_assume_init(runnable_tasks) },
            pending_tasks: BTreeMap::new(),
            blocks: BTreeMap::new(),
            timeouts: BinaryHeap::new(),
        }
    }

    fn issue_pending_id(&mut self) -> PendingId {
        let id = PendingId(self.pending_id_gen);
        self.pending_id_gen += 1;
        id
    }

    fn enqueue(&mut self, task: Task) {
        self.runnable_tasks[task.priority().index()].push_back(task);
    }

    /// Dequeuing requires a task that is currently running.
    fn dequeue(&mut self, current_task: Task, current_switch: Switch) -> Task {
        let minimum_level_index = match current_switch {
            Switch::Yield => current_task.priority().index(), // current_task is still runnable
            _ => 0,
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
            // TaskScheduler::switch -> Context::switch -> switch_context (asm.s)
            unsafe { &*current_task.ctx().get() }.mark_as_not_saved();

            match current_switch {
                Switch::Blocked(chan, timeout) => {
                    let id = self.issue_pending_id();
                    self.pending_tasks.insert(id, current_task);
                    self.blocks.entry(chan).or_default().push(id);
                    if let Some(t) = timeout {
                        self.timeouts.push(Reverse((ticks() + t, id, Some(chan))));
                    }
                }
                Switch::Sleep(t) => {
                    let id = self.issue_pending_id();
                    self.pending_tasks.insert(id, current_task);
                    self.timeouts.push(Reverse((ticks() + t, id, None)));
                }
                Switch::Yield => {
                    self.runnable_tasks[current_task.priority().index()].push_back(current_task);
                }
            }

            unsafe { &*next_task.ctx().get() }.wait_saved();
            next_task
        } else {
            current_task // There are no tasks to switch
        }
    }

    fn release(&mut self, chan: WaitChannel) {
        if let Some(ids) = self.blocks.remove(&chan) {
            for id in ids {
                if let Some(task) = self.pending_tasks.remove(&id) {
                    self.runnable_tasks[task.priority().index()].push_back(task);
                }
            }
        }
    }

    fn elapse(&mut self) {
        let ticks = ticks();
        while match self.timeouts.peek() {
            Some(Reverse((t, id, chan))) if *t <= ticks => {
                if let Some(task) = self.pending_tasks.remove(id) {
                    self.runnable_tasks[task.priority().index()].push_back(task);
                }
                if let Some(chan) = chan {
                    if let Some(ids) = self.blocks.get_mut(chan) {
                        ids.retain(|i| i != id);
                    }
                }
                let _ = self.timeouts.pop();
                true
            }
            _ => false,
        } {}
    }
}

#[repr(transparent)]
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy, Hash)]
struct PendingId(u64);

#[repr(transparent)]
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy, Hash)]
pub struct WaitChannel(u64);

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
    #[allow(dead_code)]
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
