use std::future::Future;
use std::rc::Rc;
use std::marker::PhantomData;
use std::time::Duration;
use crate::{SchedulerHandle, Task, TaskLabel, TaskPriority};
use crate::scheduler::SchedulerExt;

/// GPUI-compatible executor interface
pub struct BackgroundExecutor {
    scheduler: SchedulerHandle,
}

impl BackgroundExecutor {
    pub fn new(scheduler: SchedulerHandle) -> Self {
        Self { scheduler }
    }
    
    /// Spawn background task (GPUI pattern)
    pub fn spawn<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> Task<T> {
        self.scheduler.spawn_with_priority(
            future,
            TaskPriority::Background,
            None,
        )
    }
    
    /// Spawn with label for test control (GPUI pattern)
    pub fn spawn_labeled<T: Send + 'static>(
        &self,
        label: TaskLabel,
        future: impl Future<Output = T> + Send + 'static,
    ) -> Task<T> {
        self.scheduler.spawn_with_priority(
            future,
            TaskPriority::Background,
            Some(label),
        )
    }
    
    /// Create timer task (GPUI pattern)
    pub fn timer(&self, duration: Duration) -> Task<()> {
        self.scheduler.timer(duration)
    }
}

/// GPUI-compatible foreground executor
pub struct ForegroundExecutor {
    scheduler: SchedulerHandle,
    _not_send: PhantomData<Rc<()>>, // GPUI !Send pattern
}

impl ForegroundExecutor {
    pub fn new(scheduler: SchedulerHandle) -> Self {
        Self {
            scheduler,
            _not_send: PhantomData,
        }
    }
    
    /// Spawn foreground task (GPUI pattern)
    pub fn spawn<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> Task<T> {
        // For now, assume Send futures for foreground tasks
        // In a real GPUI implementation, this would handle !Send futures differently
        self.scheduler.spawn_with_priority(
            future,
            TaskPriority::Foreground,
            None,
        )
    }
}

// Note: In a real GPUI implementation, the foreground executor would handle
// !Send futures by ensuring they execute on the main thread using thread-local
// storage or similar mechanisms. For this compatibility layer, we require Send futures.

/// GPUI compatibility async context  
pub struct AsyncApp {
    background_executor: BackgroundExecutor,
    foreground_executor: ForegroundExecutor,
}

impl AsyncApp {
    pub fn new(scheduler: SchedulerHandle) -> Self {
        Self {
            background_executor: BackgroundExecutor::new(scheduler.clone()),
            foreground_executor: ForegroundExecutor::new(scheduler),
        }
    }

    pub fn spawn<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> Task<T> {
        self.foreground_executor.scheduler.spawn(future)
    }
    
    pub fn background_spawn<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> Task<T> {
        self.background_executor.spawn(future)
    }
}