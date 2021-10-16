use crate::task;
use heapless::mpmc::MpMcQueue;
use spin::Lazy;

/// `heapless::mpmc::MpMcQueue` with task scheduler integration.
pub struct Queue<T, const N: usize> {
    inner: MpMcQueue<T, N>,
    empty_chan: Lazy<task::WaitChannel>,
    full_chan: Lazy<task::WaitChannel>,
}

impl<T, const N: usize> Queue<T, N> {
    pub const fn new() -> Self {
        Self {
            inner: MpMcQueue::new(),
            empty_chan: Lazy::new(|| task::task_scheduler().issue_wait_channel()),
            full_chan: Lazy::new(|| task::task_scheduler().issue_wait_channel()),
        }
    }

    pub fn enqueue(&self, mut item: T, timeout: Option<usize>) {
        loop {
            match self.inner.enqueue(item).or_else(|item| {
                task::task_scheduler().switch(|| {
                    let ret = self.inner.enqueue(item);
                    let switch = match ret {
                        Ok(_) => None,
                        Err(_) => Some(task::Switch::Blocked(*self.full_chan, timeout)),
                    };
                    (switch, ret)
                })
            }) {
                Ok(()) => break,
                Err(i) => item = i,
            }
        }
        task::task_scheduler().release(*self.empty_chan);
    }

    pub fn try_enqueue(&self, item: T) -> Result<(), T> {
        self.inner.enqueue(item)?;
        task::task_scheduler().release(*self.empty_chan);
        Ok(())
    }

    pub fn dequeue(&self, timeout: Option<usize>) -> T {
        let item = loop {
            match self.inner.dequeue().or_else(|| {
                task::task_scheduler().switch(|| {
                    let ret = self.inner.dequeue();
                    let switch = match ret {
                        Some(_) => None,
                        None => Some(task::Switch::Blocked(*self.empty_chan, timeout)),
                    };
                    (switch, ret)
                })
            }) {
                Some(item) => break item,
                None => {}
            }
        };
        task::task_scheduler().release(*self.full_chan);
        item
    }

    pub fn try_dequeue(&self) -> Option<T> {
        let value = self.inner.dequeue()?;
        task::task_scheduler().release(*self.full_chan);
        Some(value)
    }
}

unsafe impl<T, const N: usize> Send for Queue<T, N> where T: Send {}
unsafe impl<T, const N: usize> Sync for Queue<T, N> where T: Send {}
