use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use rand_chacha::{ChaCha8Rng, rand_core::SeedableRng};
use rand::Rng;
use crate::scheduler::{Scheduler, TestableScheduler, TimerHandle};
use crate::task::{TaskId, SessionId, TaskLabel, TaskPriority};

pub struct SimulationScheduler {
    pub(crate) inner: Arc<Mutex<SimulationState>>,
}

struct SimulationState {
    // Task queues (Cloud pattern)
    ready_queue: VecDeque<TaskId>,
    tasks: HashMap<TaskId, SimulatedTask>,
    delay_queue: VecDeque<DelayedTask>,
    
    // Session management (Cloud pattern)
    sessions: HashMap<SessionId, SessionInfo>,
    task_to_session: HashMap<TaskId, SessionId>,
    current_session: Option<SessionId>,
    
    // Randomization (Cloud pattern)
    rng: ChaCha8Rng,
    config: SimulationConfig,
    
    // Time simulation
    current_time: Duration,
    step_count: usize,
    start_time: Instant,
}

#[derive(Debug, Clone)]
pub struct SimulationConfig {
    pub seed: u64,
    pub randomize_order: bool,
    pub randomize_delays: bool,
    pub max_steps: usize,
    pub log_operations: bool,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            seed: 42,
            randomize_order: false,
            randomize_delays: false,
            max_steps: 100_000,
            log_operations: false,
        }
    }
}

impl SimulationConfig {
    pub fn with_seed(seed: u64) -> Self {
        Self {
            seed,
            ..Default::default()
        }
    }
}

struct SimulatedTask {
    runnable: Option<async_task::Runnable>,
    spawn_location: &'static std::panic::Location<'static>,
    priority: TaskPriority,
    label: Option<TaskLabel>,
}

struct DelayedTask {
    task_id: TaskId,
    ready_time: Duration,
    time_range: Option<(Duration, Duration)>, // Cloud time-range delays
}

pub(crate) struct SessionInfo {
    pub(crate) spawned_tasks: HashSet<TaskId>,
    pub(crate) wait_until_tasks: HashSet<TaskId>, // Cloud pattern
    pub(crate) cleanup_registered: bool,
}

impl SimulationScheduler {
    pub fn new() -> Self {
        Self::with_config(SimulationConfig::default())
    }

    pub fn with_config(config: SimulationConfig) -> Self {
        let start_time = Instant::now();
        Self {
            inner: Arc::new(Mutex::new(SimulationState {
                ready_queue: VecDeque::new(),
                tasks: HashMap::new(),
                delay_queue: VecDeque::new(),
                sessions: HashMap::new(),
                task_to_session: HashMap::new(),
                current_session: None,
                rng: ChaCha8Rng::seed_from_u64(config.seed),
                config,
                current_time: Duration::ZERO,
                step_count: 0,
                start_time,
            })),
        }
    }
    
    /// Single step execution (Cloud pattern)
    fn step_internal(&self) -> bool {
        let mut state = self.inner.lock().unwrap();
        
        // Process delayed tasks
        self.process_delay_queue(&mut state);
        
        // Select next ready task (randomized or FIFO)
        let task_id = if state.config.randomize_order && !state.ready_queue.is_empty() {
            let queue_len = state.ready_queue.len();
            let index = state.rng.gen_range(0..queue_len);
            state.ready_queue.remove(index)
        } else {
            state.ready_queue.pop_front()
        };
        
        if let Some(task_id) = task_id {
            self.execute_task(&mut state, task_id)
        } else if !state.delay_queue.is_empty() {
            // Advance time if no ready tasks but delays pending
            self.advance_to_next_delay(&mut state);
            true
        } else {
            false // No work remaining
        }
    }
    
    fn process_delay_queue(&self, state: &mut SimulationState) {
        while let Some(delayed_task) = state.delay_queue.front() {
            if delayed_task.ready_time <= state.current_time {
                let delayed_task = state.delay_queue.pop_front().unwrap();
                state.ready_queue.push_back(delayed_task.task_id);
            } else {
                break;
            }
        }
    }
    
    fn execute_task(&self, state: &mut SimulationState, task_id: TaskId) -> bool {
        if let Some(task) = state.tasks.remove(&task_id) {
            if let Some(runnable) = task.runnable {
                if state.config.log_operations {
                    tracing::debug!("Executing task {} at step {}", task_id, state.step_count);
                }
                
                // Release the lock before running the task
                std::mem::drop(state);
                runnable.run();
                
                // Reacquire lock and increment step count
                let mut state = self.inner.lock().unwrap();
                state.step_count += 1;
                true
            } else {
                false
            }
        } else {
            false
        }
    }
    
    fn advance_to_next_delay(&self, state: &mut SimulationState) {
        if let Some(next_delay) = state.delay_queue.front() {
            state.current_time = next_delay.ready_time;
        }
    }
    
    /// Multi-seed testing (Cloud pattern)  
    pub fn test_many<F, T>(iterations: usize, test_fn: F) -> Vec<anyhow::Result<T>>
    where
        F: Fn(Arc<dyn TestableScheduler>) -> anyhow::Result<T> + Send + Sync,
        T: Send,
    {
        (0..iterations)
            .map(|i| {
                let config = SimulationConfig::with_seed(i as u64);
                let scheduler = Arc::new(SimulationScheduler::with_config(config));
                test_fn(scheduler)
            })
            .collect()
    }
}

impl Scheduler for SimulationScheduler {
    fn dispatch_background(&self, runnable: async_task::Runnable, label: Option<TaskLabel>) {
        let mut state = self.inner.lock().unwrap();
        let task_id = TaskId::new_v4();
        
        let task = SimulatedTask {
            runnable: Some(runnable),
            spawn_location: std::panic::Location::caller(),
            priority: TaskPriority::Background,
            label,
        };
        
        state.tasks.insert(task_id, task);
        state.ready_queue.push_back(task_id);
        
        // Track in current session if one exists
        if let Some(session_id) = state.current_session {
            state.task_to_session.insert(task_id, session_id);
            if let Some(session) = state.sessions.get_mut(&session_id) {
                session.spawned_tasks.insert(task_id);
            }
        }
    }

    fn dispatch_foreground(&self, runnable: async_task::Runnable) {
        // In simulation, foreground and background are the same
        self.dispatch_background(runnable, None);
    }

    fn dispatch_after(&self, duration: Duration, runnable: async_task::Runnable) {
        let mut state = self.inner.lock().unwrap();
        let task_id = TaskId::new_v4();
        
        let ready_time = if state.config.randomize_delays {
            // Add some randomization to the delay
            let jitter = state.rng.gen_range(-100..=100); // +/- 100ms jitter
            state.current_time + duration + Duration::from_millis(jitter.max(0) as u64)
        } else {
            state.current_time + duration
        };
        
        let task = SimulatedTask {
            runnable: Some(runnable),
            spawn_location: std::panic::Location::caller(),
            priority: TaskPriority::Background,
            label: None,
        };
        
        state.tasks.insert(task_id, task);
        state.delay_queue.push_back(DelayedTask {
            task_id,
            ready_time,
            time_range: Some((ready_time, ready_time + Duration::from_millis(10))),
        });
        
        // Sort delay queue by ready time
        state.delay_queue.make_contiguous().sort_by_key(|t| t.ready_time);
    }

    fn create_timer(&self, duration: Duration) -> TimerHandle {
        let (runnable, task) = async_task::spawn(
            async move {
                // Timer implementation - just a delay
            },
            |_runnable: async_task::Runnable| {
                // This will be scheduled via dispatch_after
            }
        );
        
        self.dispatch_after(duration, runnable);
        TimerHandle::new(task)
    }

    fn is_main_thread(&self) -> bool {
        // In simulation, we're always on the "main" thread
        true
    }

    fn now(&self) -> Instant {
        let state = self.inner.lock().unwrap();
        state.start_time + state.current_time
    }

    fn clone_scheduler(&self) -> Box<dyn Scheduler> {
        Box::new(SimulationScheduler {
            inner: self.inner.clone(),
        })
    }

    fn create_session(&self) -> SessionId {
        let session_id = SessionId::new_v4();
        let mut state = self.inner.lock().unwrap();
        
        state.sessions.insert(session_id, SessionInfo {
            spawned_tasks: HashSet::new(),
            wait_until_tasks: HashSet::new(),
            cleanup_registered: false,
        });
        
        session_id
    }

    fn validate_session_cleanup(&self, session_id: SessionId) -> anyhow::Result<()> {
        let state = self.inner.lock().unwrap();
        
        let session = state.sessions.get(&session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", session_id))?;
        
        // Check for dangling tasks (Cloud pattern)
        let dangling_tasks: HashSet<_> = session.spawned_tasks
            .difference(&session.wait_until_tasks)
            .copied()
            .collect();
        
        if !dangling_tasks.is_empty() {
            let mut error_msg = format!(
                "Session {} has {} dangling tasks not registered with wait_until:",
                session_id,
                dangling_tasks.len()
            );
            
            for task_id in &dangling_tasks {
                if let Some(task) = state.tasks.get(task_id) {
                    error_msg.push_str(&format!(
                        "\n  Task {} spawned at {}",
                        task_id,
                        task.spawn_location
                    ));
                }
            }
            
            return Err(anyhow::anyhow!(error_msg));
        }
        
        Ok(())
    }
}

impl TestableScheduler for SimulationScheduler {
    fn advance_time(&self, duration: Duration) {
        let mut state = self.inner.lock().unwrap();
        state.current_time += duration;
        // Process any delays that are now ready
        drop(state);
        self.process_delay_queue(&mut self.inner.lock().unwrap());
    }

    fn run_until_idle(&self) {
        while self.step_internal() {
            // Continue until no work remains
        }
    }

    fn step(&self) -> bool {
        self.step_internal()
    }

    fn set_randomization(&self, enabled: bool) {
        let mut state = self.inner.lock().unwrap();
        state.config.randomize_order = enabled;
        state.config.randomize_delays = enabled;
    }

    fn with_seed(seed: u64) -> Self {
        Self::with_config(SimulationConfig::with_seed(seed))
    }
}

pub mod sessions;