use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::collections::VecDeque;
use crate::platform::{PlatformDispatcher, DispatcherKind};
use crate::task::TaskLabel;

pub struct TestDispatcher {
    kind: DispatcherKind,
    queue: Arc<Mutex<VecDeque<async_task::Runnable>>>,
    delayed_queue: Arc<Mutex<VecDeque<(Duration, async_task::Runnable)>>>,
}

impl TestDispatcher {
    pub fn new(kind: DispatcherKind) -> Self {
        Self {
            kind,
            queue: Arc::new(Mutex::new(VecDeque::new())),
            delayed_queue: Arc::new(Mutex::new(VecDeque::new())),
        }
    }
    
    /// Execute all queued runnables (for testing)
    pub fn run_all(&self) {
        loop {
            let runnable = {
                let mut queue = self.queue.lock().unwrap();
                queue.pop_front()
            };
            
            match runnable {
                Some(runnable) => { runnable.run(); },
                None => break,
            }
        }
    }
    
    /// Get number of queued items
    pub fn queue_len(&self) -> usize {
        self.queue.lock().unwrap().len()
    }
}

impl PlatformDispatcher for TestDispatcher {
    fn is_main_thread(&self) -> bool {
        // In tests, we can assume we're always on the main thread
        true
    }
    
    fn dispatch(&self, runnable: async_task::Runnable, _label: Option<TaskLabel>) {
        let mut queue = self.queue.lock().unwrap();
        queue.push_back(runnable);
    }
    
    fn dispatch_after(&self, duration: Duration, runnable: async_task::Runnable) {
        let mut delayed_queue = self.delayed_queue.lock().unwrap();
        delayed_queue.push_back((duration, runnable));
    }
}