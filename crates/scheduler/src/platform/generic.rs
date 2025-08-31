use std::time::Duration;
use crate::platform::{PlatformDispatcher, DispatcherKind};
use crate::task::TaskLabel;

pub struct GenericDispatcher {
    kind: DispatcherKind,
}

impl GenericDispatcher {
    pub fn new(kind: DispatcherKind) -> Self {
        Self { kind }
    }
}

impl PlatformDispatcher for GenericDispatcher {
    fn is_main_thread(&self) -> bool {
        // Simple heuristic - assume we're on main thread if thread name contains "main"
        std::thread::current().name().map_or(false, |name| name.contains("main"))
    }
    
    fn dispatch(&self, runnable: async_task::Runnable, _label: Option<TaskLabel>) {
        // For generic implementation, just run directly
        // In a real implementation, this might use a thread pool
        runnable.run();
    }
    
    fn dispatch_after(&self, duration: Duration, runnable: async_task::Runnable) {
        // Simple implementation using thread sleep
        std::thread::spawn(move || {
            std::thread::sleep(duration);
            runnable.run();
        });
    }
}