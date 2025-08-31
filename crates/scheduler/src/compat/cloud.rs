use std::future::Future;
use std::time::Duration;
use crate::{SchedulerHandle, TestableSchedulerHandle, TaskId, SessionId, SimulationScheduler, SimulationConfig};
use crate::scheduler::{SchedulerExt, TestableSchedulerExt};

/// Cloud-compatible platform runtime
pub struct PlatformRuntime {
    scheduler: SchedulerHandle,
}

impl PlatformRuntime {
    pub fn new(scheduler: SchedulerHandle) -> Self {
        Self { scheduler }
    }
    
    /// Cloud block_on pattern
    pub fn block_on<T>(&self, future: impl Future<Output = T>) -> T {
        // For now, use futures executor
        // In a real implementation, this would check if scheduler is testable
        // and use simulation vs production accordingly
        futures::executor::block_on(future)
    }
    
    /// Cloud spawn pattern
    pub fn spawn(&self, future: impl Future<Output = ()> + Send + 'static) -> TaskId {
        let task = self.scheduler.spawn(future);
        task.id()
    }
}

/// Cloud-compatible execution context
pub struct SimulatedExecutionContext {
    scheduler: SchedulerHandle,
    session_id: Option<SessionId>,
}

impl SimulatedExecutionContext {
    pub fn new(scheduler: SchedulerHandle) -> Self {
        let session_id = scheduler.create_session();
        Self {
            scheduler,
            session_id: Some(session_id),
        }
    }
    
    /// Cloud wait_until pattern
    pub async fn wait_until(
        &self,
        future: impl Future<Output = anyhow::Result<()>> + Send + 'static,
    ) -> anyhow::Result<()> {
        let _session_id = self.session_id.ok_or_else(|| {
            anyhow::anyhow!("No session context")
        })?;
        
        // Add deterministic delay (Cloud pattern)
        self.scheduler.timer(Duration::from_millis(1)).await;
        
        // Spawn background task
        let task = self.scheduler.spawn(future);
        
        // Note: In a real implementation, we'd register this task for session cleanup
        // For now, just await the task
        task.await
    }
}

/// Cloud simulator interface
pub struct Simulator {
    scheduler: TestableSchedulerHandle,
    platform_runtime: PlatformRuntime,
}

impl Simulator {
    /// Cloud single-seed test pattern
    pub fn once<F, Fut, T>(seed: u64, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(Self) -> Fut,
        Fut: Future<Output = anyhow::Result<T>> + Send + 'static,
        T: Send + Unpin + 'static,
    {
        let scheduler = TestableSchedulerHandle::new(
            SimulationScheduler::with_config(SimulationConfig::with_seed(seed))
        );
        let platform_runtime = PlatformRuntime::new(scheduler.clone().into());
        let simulator = Self { scheduler: scheduler.clone(), platform_runtime };
        
        let result = f(simulator);
        scheduler.run_until_idle();
        
        // Block on the future using the scheduler's block_on method
        scheduler.block_on(result)
    }
    
    /// Cloud multi-seed test pattern  
    pub fn many<F, Fut, T>(iterations: usize, f: F) -> Vec<anyhow::Result<T>>
    where
        F: Fn(Self) -> Fut + Send + Sync,
        Fut: Future<Output = anyhow::Result<T>> + Send + 'static,
        T: Send + Unpin + 'static,
    {
        (0..iterations)
            .map(|i| Self::once(i as u64, &f))
            .collect()
    }

    /// Get the scheduler handle
    pub fn scheduler(&self) -> &TestableSchedulerHandle {
        &self.scheduler
    }
    
    /// Get the platform runtime
    pub fn platform_runtime(&self) -> &PlatformRuntime {
        &self.platform_runtime
    }
}