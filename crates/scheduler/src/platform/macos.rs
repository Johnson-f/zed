use std::time::Duration;
use crate::platform::{PlatformDispatcher, DispatcherKind};
use crate::task::TaskLabel;

pub struct MacOSDispatcher {
    kind: DispatcherKind,
}

impl MacOSDispatcher {
    pub fn new(kind: DispatcherKind) -> Self {
        Self { kind }
    }
}

impl PlatformDispatcher for MacOSDispatcher {
    fn is_main_thread(&self) -> bool {
        // For now, use a simple heuristic
        // In a real implementation, this would use NSThread::isMainThread()
        std::thread::current().name().map_or(true, |name| name == "main")
    }
    
    fn dispatch(&self, runnable: async_task::Runnable, _label: Option<TaskLabel>) {
        match self.kind {
            DispatcherKind::Foreground => {
                // In a real implementation, this would dispatch to the main GCD queue
                // For now, run directly if we're on main thread, spawn otherwise
                if self.is_main_thread() {
                    runnable.run();
                } else {
                    // Would use dispatch_async to main queue
                    std::thread::spawn(move || runnable.run());
                }
            }
            DispatcherKind::Background => {
                // In a real implementation, this would dispatch to a concurrent GCD queue
                std::thread::spawn(move || runnable.run());
            }
        }
    }
    
    fn dispatch_after(&self, duration: Duration, runnable: async_task::Runnable) {
        // In a real implementation, this would use dispatch_after
        std::thread::spawn(move || {
            std::thread::sleep(duration);
            runnable.run();
        });
    }
}