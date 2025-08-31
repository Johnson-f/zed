use std::sync::Arc;
use crate::scheduler::{Scheduler, TestableScheduler};

/// Arc-wrapped scheduler reference for cross-thread sharing
#[derive(Clone)]
pub struct SchedulerHandle {
    inner: Arc<dyn Scheduler>,
}

impl SchedulerHandle {
    pub fn new<S: Scheduler>(scheduler: S) -> Self {
        Self {
            inner: Arc::new(scheduler),
        }
    }

    /// Get reference to underlying scheduler
    pub fn scheduler(&self) -> &dyn Scheduler {
        &*self.inner
    }

    /// Convert to TestableSchedulerHandle if possible
    pub fn as_testable(&self) -> Option<TestableSchedulerHandle> {
        // This is a simplification - in practice we'd need proper downcasting
        None
    }
}

// Delegate all Scheduler methods to inner
impl std::ops::Deref for SchedulerHandle {
    type Target = dyn Scheduler;

    fn deref(&self) -> &Self::Target {
        &*self.inner
    }
}

/// Testable scheduler handle
#[derive(Clone)]
pub struct TestableSchedulerHandle {
    inner: Arc<dyn TestableScheduler>,
}

impl TestableSchedulerHandle {
    pub fn new<S: TestableScheduler>(scheduler: S) -> Self {
        Self {
            inner: Arc::new(scheduler),
        }
    }

    /// Convert to regular SchedulerHandle
    pub fn into_scheduler_handle(self) -> SchedulerHandle {
        SchedulerHandle {
            inner: self.inner.clone(),
        }
    }
}

impl std::ops::Deref for TestableSchedulerHandle {
    type Target = dyn TestableScheduler;

    fn deref(&self) -> &Self::Target {
        &*self.inner
    }
}

impl From<TestableSchedulerHandle> for SchedulerHandle {
    fn from(testable: TestableSchedulerHandle) -> Self {
        testable.into_scheduler_handle()
    }
}