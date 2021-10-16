use crate::task;
use heapless::mpmc::MpMcQueue;
use spin::Lazy;

/// `heapless::mpmc::MpMcQueue` with task scheduler integration.
pub struct Queue<T, const N: usize> {
    inner: MpMcQueue<T, N>,
    empty_chan: Lazy<task::Chan>,
    full_chan: Lazy<task::Chan>,
}

impl<T, const N: usize> Queue<T, N> {
    pub const fn new() -> Self {
        Self {
            inner: MpMcQueue::new(),
            empty_chan: Lazy::new(|| task::task_scheduler().issue_chan()),
            full_chan: Lazy::new(|| task::task_scheduler().issue_chan()),
        }
    }

    pub fn enqueue(&self, mut item: T) {
        loop {
            match self.inner.enqueue(item).or_else(|item| unsafe {
                task::task_scheduler().switch(|| match self.inner.enqueue(item) {
                    Ok(()) => (task::Switch::Cancel, Ok(())),
                    Err(item) => (task::Switch::Sleep(*self.full_chan), Err(item)),
                })
            }) {
                Ok(()) => break,
                Err(i) => item = i,
            }
        }
        task::task_scheduler().wakeup(*self.empty_chan);
    }

    pub fn try_enqueue(&self, item: T) -> Result<(), T> {
        self.inner.enqueue(item)?;
        task::task_scheduler().wakeup(*self.empty_chan);
        Ok(())
    }

    pub fn dequeue(&self) -> T {
        let item = loop {
            match self.inner.dequeue().or_else(|| unsafe {
                task::task_scheduler().switch(|| match self.inner.dequeue() {
                    Some(item) => (task::Switch::Cancel, Some(item)),
                    None => (task::Switch::Sleep(*self.empty_chan), None),
                })
            }) {
                Some(item) => break item,
                None => {}
            }
        };
        task::task_scheduler().wakeup(*self.full_chan);
        item
    }

    pub fn try_dequeue(&self) -> Option<T> {
        let value = self.inner.dequeue()?;
        task::task_scheduler().wakeup(*self.full_chan);
        Some(value)
    }
}

unsafe impl<T, const N: usize> Send for Queue<T, N> where T: Send {}
unsafe impl<T, const N: usize> Sync for Queue<T, N> where T: Send {}
