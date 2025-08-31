use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use uuid::Uuid;

pub type TaskId = Uuid;
pub type SessionId = Uuid;

/// Unified task abstraction supporting both immediate and spawned execution
#[derive(Debug)]
pub struct Task<T> {
    inner: TaskState<T>,
    id: TaskId,
    session_id: Option<SessionId>,
    spawn_location: &'static std::panic::Location<'static>,
}

#[derive(Debug)]
enum TaskState<T> {
    Ready(Option<T>),
    Spawned(async_task::Task<T>),
    Cancelled,
}

impl<T> Task<T> {
    /// Create task with immediate value (GPUI Ready pattern)
    #[track_caller]
    pub fn ready(value: T) -> Self {
        Self::ready_with_session(value, None)
    }

    /// Create task with session context (Cloud pattern)
    #[track_caller]
    pub fn ready_with_session(value: T, session_id: Option<SessionId>) -> Self {
        Self {
            inner: TaskState::Ready(Some(value)),
            id: TaskId::new_v4(),
            session_id,
            spawn_location: std::panic::Location::caller(),
        }
    }

    /// Create from spawned async_task (internal use)
    #[track_caller]
    pub fn from_spawned(
        async_task: async_task::Task<T>,
        task_id: TaskId,
        session_id: Option<SessionId>,
        spawn_location: &'static std::panic::Location<'static>,
    ) -> Self {
        Self {
            inner: TaskState::Spawned(async_task),
            id: task_id,
            session_id,
            spawn_location,
        }
    }

    /// Create new task from async_task (helper for SchedulerExt)
    #[track_caller]
    pub fn new(async_task: async_task::Task<T>) -> Self {
        Self {
            inner: TaskState::Spawned(async_task),
            id: TaskId::new_v4(),
            session_id: None,
            spawn_location: std::panic::Location::caller(),
        }
    }

    /// Detach task to run without returning value (GPUI pattern)
    pub fn detach(self) {
        match self.inner {
            TaskState::Spawned(task) => task.detach(),
            TaskState::Ready(_) => {
                // Already complete, nothing to detach
            }
            TaskState::Cancelled => {
                // Already cancelled, nothing to detach
            }
        }
    }

    /// Detach with error logging (GPUI pattern)
    pub fn detach_and_log_err(self)
    where
        T: std::fmt::Debug,
    {
        match self.inner {
            TaskState::Spawned(task) => {
                tracing::debug!("Task {} detached at {}", self.id, self.spawn_location);
                task.detach();
            }
            TaskState::Ready(Some(value)) => {
                tracing::debug!("Ready task {} completed with value: {:?}", self.id, value);
            }
            TaskState::Ready(None) | TaskState::Cancelled => {
                // Nothing to log
            }
        }
    }

    /// Check if task is from specific session (Cloud pattern)
    pub fn session_id(&self) -> Option<SessionId> {
        self.session_id
    }

    /// Get spawn location for debugging (Cloud pattern)
    pub fn spawn_location(&self) -> &'static std::panic::Location<'static> {
        self.spawn_location
    }

    /// Get task ID
    pub fn id(&self) -> TaskId {
        self.id
    }

    /// Check if task is finished
    pub fn is_finished(&self) -> bool {
        match &self.inner {
            TaskState::Ready(Some(_)) => true,
            TaskState::Ready(None) => true,
            TaskState::Cancelled => true,
            TaskState::Spawned(task) => task.is_finished(),
        }
    }

    /// Convert to result if completed (for testing)
    pub fn into_result(self) -> Option<T> {
        match self.inner {
            TaskState::Ready(value) => value,
            TaskState::Spawned(_) => None, // Not yet completed
            TaskState::Cancelled => None,
        }
    }
}

impl<T: Unpin> Future for Task<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        match &mut this.inner {
            TaskState::Ready(value) => {
                if let Some(value) = value.take() {
                    Poll::Ready(value)
                } else {
                    // Task was already polled to completion
                    panic!("Task polled after completion");
                }
            }
            TaskState::Spawned(task) => Pin::new(task).poll(cx),
            TaskState::Cancelled => {
                panic!("Task was cancelled");
            }
        }
    }
}

/// Task priority for scheduling decisions (unified pattern)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskPriority {
    Background,
    Normal,
    Foreground,
    Critical,
}

impl Default for TaskPriority {
    fn default() -> Self {
        TaskPriority::Normal
    }
}

/// Task label for debugging and test control (GPUI pattern)
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct TaskLabel(Arc<str>);

impl TaskLabel {
    pub fn new(label: impl Into<Arc<str>>) -> Self {
        Self(label.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for TaskLabel {
    fn from(s: &str) -> Self {
        Self(s.into())
    }
}

impl From<String> for TaskLabel {
    fn from(s: String) -> Self {
        Self(s.into())
    }
}