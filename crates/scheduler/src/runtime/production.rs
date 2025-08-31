use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use crate::scheduler::{Scheduler, TimerHandle};
use crate::task::{TaskId, SessionId, TaskLabel};
use crate::platform::{PlatformDispatcher, DispatcherKind, create_platform_dispatcher};

pub struct ProductionScheduler {
    background_dispatcher: Arc<dyn PlatformDispatcher>,
    foreground_dispatcher: Arc<dyn PlatformDispatcher>, 
    sessions: Arc<Mutex<HashMap<SessionId, SessionState>>>,
    config: RuntimeConfig,
    start_time: Instant,
}

struct SessionState {
    spawned_tasks: std::collections::HashSet<TaskId>,
    cleanup_registered: bool,
}

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub max_background_threads: Option<usize>,
    pub enable_session_tracking: bool,
    pub platform_optimizations: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            max_background_threads: None,
            enable_session_tracking: false,
            platform_optimizations: true,
        }
    }
}

impl ProductionScheduler {
    pub fn new(config: RuntimeConfig) -> Self {
        let background_dispatcher = create_platform_dispatcher(DispatcherKind::Background);
        let foreground_dispatcher = create_platform_dispatcher(DispatcherKind::Foreground);
        
        Self {
            background_dispatcher,
            foreground_dispatcher,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            config,
            start_time: Instant::now(),
        }
    }
}

impl Scheduler for ProductionScheduler {
    fn dispatch_background(&self, runnable: async_task::Runnable, label: Option<TaskLabel>) {
        self.background_dispatcher.dispatch(runnable, label);
    }

    fn dispatch_foreground(&self, runnable: async_task::Runnable) {
        self.foreground_dispatcher.dispatch(runnable, None);
    }

    fn dispatch_after(&self, duration: Duration, runnable: async_task::Runnable) {
        self.background_dispatcher.dispatch_after(duration, runnable);
    }

    fn create_timer(&self, duration: Duration) -> TimerHandle {
        let (runnable, task) = async_task::spawn(
            async move {
                // Simple timer implementation using thread sleep
                let start = Instant::now();
                while start.elapsed() < duration {
                    std::thread::yield_now();
                }
            },
            |runnable: async_task::Runnable| {
                // Schedule on background dispatcher
                std::thread::spawn(move || runnable.run());
            }
        );
        runnable.schedule();
        TimerHandle::new(task)
    }

    fn is_main_thread(&self) -> bool {
        self.foreground_dispatcher.is_main_thread()
    }

    fn now(&self) -> Instant {
        Instant::now()
    }

    fn clone_scheduler(&self) -> Box<dyn Scheduler> {
        Box::new(ProductionScheduler {
            background_dispatcher: self.background_dispatcher.clone(),
            foreground_dispatcher: self.foreground_dispatcher.clone(),
            sessions: self.sessions.clone(),
            config: self.config.clone(),
            start_time: self.start_time,
        })
    }

    fn create_session(&self) -> SessionId {
        let session_id = SessionId::new_v4();
        if self.config.enable_session_tracking {
            let mut sessions = self.sessions.lock().unwrap();
            sessions.insert(session_id, SessionState {
                spawned_tasks: std::collections::HashSet::new(),
                cleanup_registered: false,
            });
        }
        session_id
    }

    fn validate_session_cleanup(&self, session_id: SessionId) -> anyhow::Result<()> {
        if !self.config.enable_session_tracking {
            return Ok(());
        }

        let sessions = self.sessions.lock().unwrap();
        let session = sessions.get(&session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;

        if !session.spawned_tasks.is_empty() && !session.cleanup_registered {
            return Err(anyhow::anyhow!(
                "Session {} has {} untracked tasks",
                session_id,
                session.spawned_tasks.len()
            ));
        }

        Ok(())
    }
}