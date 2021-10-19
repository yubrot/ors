use crate::sync::lazy::Lazy;
use crate::task;
use heapless::mpmc::MpMcQueue;

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
            empty_chan: Lazy::new(|| task::scheduler().issue_wait_channel()),
            full_chan: Lazy::new(|| task::scheduler().issue_wait_channel()),
        }
    }

    pub fn enqueue(&self, mut item: T) {
        loop {
            match self.inner.enqueue(item).or_else(|item| {
                task::scheduler().switch(|| {
                    let ret = self.inner.enqueue(item);
                    let switch = match ret {
                        Ok(_) => None,
                        Err(_) => Some(task::Switch::Blocked(*self.full_chan, None)),
                    };
                    (switch, ret)
                })
            }) {
                Ok(()) => break,
                Err(i) => item = i,
            }
        }
        task::scheduler().release(*self.empty_chan);
    }

    pub fn enqueue_timeout(&self, item: T, timeout: usize) -> Result<(), T> {
        self.inner
            .enqueue(item)
            .or_else(|item| {
                task::scheduler().switch(|| {
                    let ret = self.inner.enqueue(item);
                    let switch = match ret {
                        Ok(_) => None,
                        Err(_) => Some(task::Switch::Blocked(*self.full_chan, Some(timeout))),
                    };
                    (switch, ret)
                })
            })
            .or_else(|item| self.inner.enqueue(item))?;
        task::scheduler().release(*self.empty_chan);
        Ok(())
    }

    pub fn try_enqueue(&self, item: T) -> Result<(), T> {
        self.inner.enqueue(item)?;
        task::scheduler().release(*self.empty_chan);
        Ok(())
    }

    pub fn dequeue(&self) -> T {
        let item = loop {
            match self.inner.dequeue().or_else(|| {
                task::scheduler().switch(|| {
                    let ret = self.inner.dequeue();
                    let switch = match ret {
                        Some(_) => None,
                        None => Some(task::Switch::Blocked(*self.empty_chan, None)),
                    };
                    (switch, ret)
                })
            }) {
                Some(item) => break item,
                None => {}
            }
        };
        task::scheduler().release(*self.full_chan);
        item
    }

    pub fn dequeue_timeout(&self, timeout: usize) -> Option<T> {
        let item = self
            .inner
            .dequeue()
            .or_else(|| {
                task::scheduler().switch(|| {
                    let ret = self.inner.dequeue();
                    let switch = match ret {
                        Some(_) => None,
                        None => Some(task::Switch::Blocked(*self.empty_chan, Some(timeout))),
                    };
                    (switch, ret)
                })
            })
            .or_else(|| self.inner.dequeue())?;
        task::scheduler().release(*self.full_chan);
        Some(item)
    }

    pub fn try_dequeue(&self) -> Option<T> {
        let value = self.inner.dequeue()?;
        task::scheduler().release(*self.full_chan);
        Some(value)
    }
}

unsafe impl<T, const N: usize> Send for Queue<T, N> where T: Send {}
unsafe impl<T, const N: usize> Sync for Queue<T, N> where T: Send {}
