use async_task::Runnable;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use crate::task::{Task, TaskId, SessionId, TaskLabel, TaskPriority};

/// Core object-safe scheduler interface that dispatches Runnables
///
/// This trait is object-safe because all methods use concrete types
/// and avoid generic parameters. The async_task crate handles all
/// type erasure internally through Runnable.
///
/// KEY INSIGHT: async_task::Runnable is already type-erased, object-safe,
/// and provides zero-cost abstraction. No additional boxing required.
pub trait Scheduler: Send + Sync + 'static {
    /// Dispatch a runnable for background execution
    ///
    /// This is the core primitive that all other operations build upon.
    /// The runnable is already type-erased by async_task::spawn.
    fn dispatch_background(&self, runnable: Runnable, label: Option<TaskLabel>);

    /// Dispatch a runnable for foreground/main-thread execution
    fn dispatch_foreground(&self, runnable: Runnable);

    /// Dispatch a runnable after a delay
    fn dispatch_after(&self, duration: Duration, runnable: Runnable);

    /// Create a timer that produces a runnable when ready
    fn create_timer(&self, duration: Duration) -> TimerHandle;

    /// Check if we're on the main thread
    fn is_main_thread(&self) -> bool;

    /// Get current time (for testing schedulers, this may be simulated)
    fn now(&self) -> Instant;

    /// Clone the scheduler handle (for Arc<dyn Scheduler> usage)
    fn clone_scheduler(&self) -> Box<dyn Scheduler>;

    /// Create session for task grouping (Cloud pattern)
    fn create_session(&self) -> SessionId;

    /// Validate session cleanup with detailed error reporting
    fn validate_session_cleanup(&self, session_id: SessionId) -> anyhow::Result<()>;
}

/// Extension trait providing convenient spawn methods
///
/// This trait is NOT object-safe due to generic methods, but provides
/// ergonomic APIs that compile down to the object-safe core operations.
pub trait SchedulerExt: Scheduler {
    /// Spawn a future on background threads (GPUI pattern)
    fn spawn_background<F, T>(&self, future: F) -> Task<T>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let scheduler = self.clone_scheduler();
        let (runnable, async_task) = async_task::spawn(future, move |runnable| {
            scheduler.dispatch_background(runnable, None)
        });
        runnable.schedule();
        Task::new(async_task)
    }

    /// Spawn with explicit label for testing
    fn spawn_labeled<F, T>(&self, future: F, label: TaskLabel) -> Task<T>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let scheduler = self.clone_scheduler();
        let label_clone = label.clone();
        let (runnable, async_task) = async_task::spawn(future, move |runnable| {
            scheduler.dispatch_background(runnable, Some(label_clone.clone()))
        });
        runnable.schedule();
        Task::new(async_task)
    }

    /// Spawn foreground task
    fn spawn_foreground<F, T>(&self, future: F) -> Task<T>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let scheduler = self.clone_scheduler();
        let (runnable, async_task) = async_task::spawn(future, move |runnable| {
            scheduler.dispatch_foreground(runnable)
        });
        runnable.schedule();
        Task::new(async_task)
    }

    /// Spawn with priority and optional label
    fn spawn_with_priority<F, T>(
        &self,
        future: F,
        priority: TaskPriority,
        label: Option<TaskLabel>,
    ) -> Task<T>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let task_id = TaskId::new_v4();
        let location = std::panic::Location::caller();

        let scheduler = self.clone_scheduler();
        let label_clone = label.clone();
        let (runnable, async_task) = async_task::spawn(future, move |runnable| {
            match priority {
                TaskPriority::Foreground | TaskPriority::Critical => {
                    scheduler.dispatch_foreground(runnable)
                }
                _ => scheduler.dispatch_background(runnable, label_clone.clone()),
            }
        });

        runnable.schedule();
        Task::from_spawned(async_task, task_id, None, location)
    }

    /// Create a basic spawn method (most common usage)
    fn spawn<F, T>(&self, future: F) -> Task<T>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        self.spawn_background(future)
    }

    /// Create timer (helper method)
    fn timer(&self, duration: Duration) -> Task<()> {
        let timer_handle = self.create_timer(duration);
        let scheduler = self.clone_scheduler();
        let (runnable, async_task) = async_task::spawn(timer_handle, move |runnable| {
            scheduler.dispatch_background(runnable, None)
        });
        runnable.schedule();
        Task::new(async_task)
    }
}

// Blanket implementation for all Schedulers
impl<T: Scheduler + ?Sized> SchedulerExt for T {}

/// Session management for task grouping and cleanup validation
pub trait SessionScheduler: Scheduler {
    /// Dispatch runnable within a session context
    fn dispatch_in_session(&self, runnable: Runnable, session_id: SessionId);

    /// Register task for cleanup validation
    fn register_task_for_session(&self, task_id: TaskId, session_id: SessionId);

    /// Cleanup session resources
    fn cleanup_session(&self, session_id: SessionId);

}

/// Extended interface for testable schedulers (object-safe)
pub trait TestableScheduler: Scheduler {
    /// Advance simulated time
    fn advance_time(&self, duration: Duration);

    /// Run until no more work (Cloud block_on pattern)
    fn run_until_idle(&self);

    /// Execute single step and return whether work was done
    fn step(&self) -> bool;

    /// Enable/disable randomization (Cloud pattern)
    fn set_randomization(&self, enabled: bool);

    /// Create with specific seed (where Self: Sized)
    fn with_seed(seed: u64) -> Self
    where
        Self: Sized;
}

/// Extension trait for TestableScheduler with generic methods
pub trait TestableSchedulerExt: TestableScheduler {
    /// Block on a future until completion
    fn block_on<T>(&self, future: impl Future<Output = T> + Send + 'static) -> T 
    where
        T: Send + Unpin + 'static,
    {
        let task = self.spawn(future);
        self.run_until_idle();
        futures::executor::block_on(async { task.await })
    }
}

// Blanket implementation for all TestableSchedulers
impl<T: TestableScheduler + ?Sized> TestableSchedulerExt for T {}

/// Timer handle that can be awaited
pub struct TimerHandle {
    task: async_task::Task<()>,
}

impl TimerHandle {
    pub fn new(task: async_task::Task<()>) -> Self {
        Self { task }
    }
}

impl Future for TimerHandle {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.task).poll(cx)
    }
}

/// Fuzzing-compatible scheduler interface
#[cfg(feature = "fuzzing")]
pub trait FuzzableScheduler: TestableScheduler {
    /// Apply fuzz-driven scheduling decisions
    fn apply_fuzz_decisions(&self, data: &[u8]);

    /// Create from fuzz data (where Self: Sized)
    fn from_fuzz_data(data: &[u8]) -> Self
    where
        Self: Sized;
}