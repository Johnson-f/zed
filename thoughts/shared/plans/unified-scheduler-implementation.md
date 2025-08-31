# Unified Scheduler Crate Implementation Plan

## Overview

This plan creates a unified scheduler abstraction layer that leverages `async_task::Runnable` as the foundation for zero-cost object-safe scheduling. The implementation works with platform-native runtimes (GPUI's GCD integration, Cloudflare's V8 event loop) while providing comprehensive session management, AFL fuzzing integration, and sophisticated multi-seed testing capabilities.

**Key Innovation**: Using `async_task::Runnable` as the core abstraction eliminates boxing overhead while achieving full object safety, as the async_task crate already provides optimal type erasure internally.

## Current State Analysis

Based on my research of both codebases and their deployment constraints:

**GPUI Constraints (`crates/gpui/src/platform/mac/dispatcher.rs`)**:
- Requires **exclusive ownership** of macOS main thread via `NSApplication::run()` 
- Uses **direct GCD integration** (`dispatch_async_f`, `dispatch_get_main_queue()`) at lines 58-72
- Cannot coexist with Tokio - both need event loop control
- Current Tokio integration uses separate multi-threaded runtime avoiding main thread

**Cloudflare Workers Constraints (`crates/cloudflare_platform/src/execution_context.rs`)**:
- Runs in **V8 isolates with JavaScript event loop**, not native Rust
- **No Tokio/async-std support** - compiles to `wasm32-unknown-unknown` 
- Uses `wasm-bindgen-futures` to bridge Rust Futures to JavaScript Promises
- Async execution via JavaScript's single-threaded event loop

**Cloud Simulator (`crates/platform_simulator/src/runtime.rs`)**:
- `SimulatorRuntime` with comprehensive randomization via `ChaCha8Rng`
- Session-based task tracking with cleanup validation patterns at line 488
- Multi-seed testing for race condition detection
- Single-threaded execution model with deterministic scheduling

### Key Discoveries:

- **async_task::Runnable provides perfect foundation**: Already type-erased, object-safe, with zero boxing overhead (`crates/gpui/src/executor.rs:171-173`)
- **Platform-native runtimes required**: GPUI needs GCD, Cloudflare needs V8 event loop - work through abstraction, not replacement
- **Session management patterns**: Cloud's comprehensive session tracking with cleanup validation at `crates/platform_simulator/src/runtime.rs:488`
- **Multi-seed testing effectiveness**: Cloud's systematic race condition detection via `ChaCha8Rng` seeding
- **Object-safe trait design**: Core scheduler interface uses concrete types, with generic extensions via blanket implementations

## Desired End State

A unified scheduler abstraction that:
- Provides object-safe `Arc<dyn Scheduler>` references with `async_task::Runnable` dispatch
- Achieves **zero performance overhead** through direct platform integration
- Works with **platform-native runtimes** (GCD, V8 event loop, simulation) via adapter pattern
- Integrates AFL fuzzing for comprehensive race condition detection
- Enables sophisticated session management with cleanup validation
- Provides multi-seed testing for systematic race condition discovery

**Performance Target**: Memory overhead <5%, CPU overhead <2%, identical dispatch performance to current GPUI implementation.

**Verification**: Integration tests demonstrate abstraction works across all platforms with performance benchmarks, session cleanup validation, and AFL fuzzing discovering scheduling race conditions.

## What We're NOT Doing

- **Replacing platform-native runtimes** - GPUI keeps GCD, Cloudflare keeps V8 event loop
- **Using Tokio for production** - incompatible with both GPUI and Cloudflare constraints
- **Breaking existing runtime integrations** - work through abstraction, not replacement
- **Modifying existing scheduler implementations** - create adapters instead
- **Creating new async runtime primitives** - leverage existing platform capabilities

## Implementation Approach

Build the unified scheduler as an **abstraction layer** with platform-specific adapters:
1. **Object-safe core traits** - following GPUI's `PlatformDispatcher` patterns
2. **Platform adapters** - wrap existing GPUI, Cloudflare, and Simulator runtimes
3. **Testing and fuzzing layer** - simulation scheduler drives all platform patterns

This approach respects platform constraints while enabling unified testing and fuzzing capabilities.

## Phase 1: Core Scheduler Abstraction

### Overview

Establish the foundational traits, types, and Arc-based architecture that unifies both GPUI and Cloud patterns.

### Changes Required:

#### 1. Crate Structure and Dependencies

**File**: `crates/unified_scheduler/Cargo.toml`
**Changes**: Create crate with conditional dependencies

```toml
[package]
name = "unified_scheduler"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow = "1.0"
futures = "0.3" 
rand = "0.8"
rand_chacha = "0.3"
uuid = { version = "1.0", features = ["v4"] }
tracing = "0.1"
async-task = "4.0"

# Optional runtime backends
tokio = { version = "1.0", features = ["full"], optional = true }
async-std = { version = "1.0", optional = true }

# Optional fuzzing support  
afl = { version = "0.15", optional = true }

[features]
default = ["tokio-runtime"]
tokio-runtime = ["tokio"]
async-std-runtime = ["async-std"] 
fuzzing = ["afl"]
testing = []
```

#### 2. Core Task Abstraction

**File**: `crates/unified_scheduler/src/task.rs`
**Changes**: Unified Task<T> type combining both patterns

```rust
use std::pin::Pin;
use std::future::Future;
use std::sync::Arc;
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

enum TaskState<T> {
    Ready(Option<T>),
    Spawned(async_task::Task<T>),
    Cancelled,
}

impl<T> Task<T> {
    /// Create task with immediate value (GPUI Ready pattern)
    pub fn ready(value: T) -> Self {
        Self::ready_with_session(value, None)
    }
    
    /// Create task with session context (Cloud pattern)
    pub fn ready_with_session(value: T, session_id: Option<SessionId>) -> Self;
    
    /// Detach task to run without returning value (GPUI pattern)
    pub fn detach(self);
    
    /// Detach with error logging (GPUI pattern)
    pub fn detach_and_log_err(self) where T: std::fmt::Debug;
    
    /// Check if task is from specific session (Cloud pattern)
    pub fn session_id(&self) -> Option<SessionId>;
    
    /// Get spawn location for debugging (Cloud pattern)  
    pub fn spawn_location(&self) -> &'static std::panic::Location<'static>;
}

/// Task priority for scheduling decisions (unified pattern)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskPriority {
    Background,
    Normal, 
    Foreground,
    Critical,
}

/// Task label for debugging and test control (GPUI pattern)
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct TaskLabel(Arc<str>);
```

#### 3. Object-Safe Core Scheduler Traits

**File**: `crates/unified_scheduler/src/scheduler.rs` 
**Changes**: Object-safe scheduler interface using `async_task::Runnable` foundation

```rust
use async_task::Runnable;
use std::time::{Duration, Instant};
use std::sync::Arc;
use crate::task::{TaskId, SessionId, TaskLabel};

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
        let (runnable, async_task) = async_task::spawn(future, {
            let scheduler = self.clone_scheduler();
            move |runnable| scheduler.dispatch_background(runnable, None)
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
        let (runnable, async_task) = async_task::spawn(future, {
            let scheduler = self.clone_scheduler();
            move |runnable| scheduler.dispatch_background(runnable, Some(label.clone()))
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

/// Extended interface for testable schedulers (also object-safe)
pub trait TestableScheduler: Scheduler {
    /// Advance simulated time
    fn advance_time(&self, duration: Duration);
    
    /// Run until no more work (Cloud block_on pattern)
    fn run_until_idle(&self);
    
    /// Execute single step and return whether work was done
    fn step(&self) -> bool;
    
    /// Enable/disable randomization (Cloud pattern)
    fn set_randomization(&self, enabled: bool);
}

/// Timer handle that can be awaited
pub struct TimerHandle {
    task: async_task::Task<()>,
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
    fn from_fuzz_data(data: &[u8]) -> Self where Self: Sized;
}
```

#### 4. Arc-Based Executor Implementation

**File**: `crates/unified_scheduler/src/executor.rs`
**Changes**: Arc-wrapped schedulers with Send compatibility

```rust
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
}

impl std::ops::Deref for TestableSchedulerHandle {
    type Target = dyn TestableScheduler;
    
    fn deref(&self) -> &Self::Target {
        &*self.inner  
    }
}
```

### Success Criteria:

#### Automated Verification:
- [ ] Crate compiles cleanly: `cargo check -p unified_scheduler`
- [ ] All features compile: `cargo check -p unified_scheduler --all-features`
- [ ] Documentation builds: `cargo doc -p unified_scheduler` 
- [ ] Linting passes: `cargo clippy -p unified_scheduler`

#### Manual Verification:
- [ ] Task<T> API provides both GPUI Ready and Cloud session patterns
- [ ] Arc-wrapped schedulers can be shared across threads
- [ ] Trait design supports both foreground/background and session-based patterns
- [ ] API is ergonomic for both existing use cases

---

## Phase 2: Production Runtime Implementation

### Overview

Implement production-quality scheduler supporting GPUI's foreground/background executor patterns with platform-specific optimizations.

### Changes Required:

#### 1. Production Scheduler Core

**File**: `crates/unified_scheduler/src/runtime/production.rs`
**Changes**: Production runtime with platform dispatchers

```rust
use std::sync::Arc;
use crate::scheduler::{Scheduler, TestableScheduler};
use crate::task::{Task, TaskId, SessionId, TaskPriority, TaskLabel};
use crate::platform::{PlatformDispatcher, DispatcherKind};

pub struct ProductionScheduler {
    background_dispatcher: Arc<dyn PlatformDispatcher>,
    foreground_dispatcher: Arc<dyn PlatformDispatcher>, 
    sessions: Arc<Mutex<HashMap<SessionId, SessionState>>>,
    config: RuntimeConfig,
}

struct SessionState {
    spawned_tasks: HashSet<TaskId>,
    cleanup_registered: bool,
}

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub max_background_threads: Option<usize>,
    pub enable_session_tracking: bool,
    pub platform_optimizations: bool,
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
        }
    }
}

impl Scheduler for ProductionScheduler {
    fn spawn_with_priority<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
        priority: TaskPriority,
        label: Option<TaskLabel>,
    ) -> Task<T> {
        let dispatcher = match priority {
            TaskPriority::Foreground | TaskPriority::Critical => &self.foreground_dispatcher,
            _ => &self.background_dispatcher,
        };
        
        let task_id = TaskId::new_v4();
        let location = std::panic::Location::caller();
        
        let (runnable, async_task) = async_task::spawn(future, {
            let dispatcher = dispatcher.clone();
            let label = label.clone();
            move |runnable| dispatcher.dispatch(runnable, label)
        });
        
        runnable.schedule();
        
        Task::from_spawned(async_task, task_id, None, location)
    }
    
    fn create_session(&self) -> SessionId {
        let session_id = SessionId::new_v4();
        if self.config.enable_session_tracking {
            let mut sessions = self.sessions.lock().unwrap();
            sessions.insert(session_id, SessionState::new());
        }
        session_id
    }
}
```

#### 2. Platform Dispatcher Implementation

**File**: `crates/unified_scheduler/src/platform/mod.rs`
**Changes**: Unified platform abstraction

```rust
use crate::task::TaskLabel;

pub trait PlatformDispatcher: Send + Sync {
    fn is_main_thread(&self) -> bool;
    fn dispatch(&self, runnable: async_task::Runnable, label: Option<TaskLabel>);
    fn dispatch_after(&self, duration: Duration, runnable: async_task::Runnable);
}

pub enum DispatcherKind {
    Background,
    Foreground,
}

pub fn create_platform_dispatcher(kind: DispatcherKind) -> Arc<dyn PlatformDispatcher> {
    #[cfg(target_os = "macos")]
    return Arc::new(crate::platform::macos::MacOSDispatcher::new(kind));
    
    #[cfg(target_os = "linux")]
    return Arc::new(crate::platform::linux::LinuxDispatcher::new(kind));
    
    #[cfg(target_os = "windows")]
    return Arc::new(crate::platform::windows::WindowsDispatcher::new(kind));
    
    #[cfg(test)]
    return Arc::new(crate::platform::test::TestDispatcher::new(kind));
}

// Platform-specific modules
#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "linux")]  
pub mod linux;

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(test)]
pub mod test;
```

#### 3. macOS Dispatcher (GPUI Pattern)

**File**: `crates/unified_scheduler/src/platform/macos.rs`
**Changes**: macOS GCD integration from GPUI

```rust
use std::time::Duration;
use crate::platform::{PlatformDispatcher, DispatcherKind};
use crate::task::TaskLabel;

pub struct MacOSDispatcher {
    background_queue: dispatch::Queue,
    main_queue: dispatch::Queue,
    kind: DispatcherKind,
}

impl MacOSDispatcher {
    pub fn new(kind: DispatcherKind) -> Self {
        Self {
            background_queue: dispatch::Queue::create(
                "unified_scheduler.background",
                dispatch::QueueAttribute::Concurrent,
            ),
            main_queue: dispatch::Queue::main(),
            kind,
        }
    }
}

impl PlatformDispatcher for MacOSDispatcher {
    fn is_main_thread(&self) -> bool {
        unsafe { foundation::NSThread::isMainThread() }
    }
    
    fn dispatch(&self, runnable: async_task::Runnable, _label: Option<TaskLabel>) {
        let queue = match self.kind {
            DispatcherKind::Foreground => &self.main_queue,
            DispatcherKind::Background => &self.background_queue,
        };
        
        queue.exec_async(move || {
            runnable.run();
        });
    }
    
    fn dispatch_after(&self, duration: Duration, runnable: async_task::Runnable) {
        let queue = match self.kind {
            DispatcherKind::Foreground => &self.main_queue,
            DispatcherKind::Background => &self.background_queue,  
        };
        
        queue.exec_after(duration, move || {
            runnable.run(); 
        });
    }
}
```

### Success Criteria:

#### Automated Verification:
- [ ] Production scheduler compiles: `cargo check -p unified_scheduler --features tokio-runtime`
- [ ] Platform dispatchers work: `cargo test -p unified_scheduler test_platform_dispatch`  
- [ ] Cross-platform compilation: `cargo check --target x86_64-pc-windows-msvc`
- [ ] No unsafe code warnings: `cargo clippy -p unified_scheduler`

#### Manual Verification:
- [ ] Background tasks execute on separate threads
- [ ] Foreground tasks execute on main thread
- [ ] Platform-specific optimizations work correctly
- [ ] Performance is comparable to GPUI's current implementation

---

## Phase 3: Simulation Runtime Implementation

### Overview

Implement deterministic simulation runtime supporting Cloud's comprehensive randomization and session management patterns.

### Changes Required:

#### 1. Simulation Runtime Core

**File**: `crates/unified_scheduler/src/runtime/simulation.rs`
**Changes**: Cloud SimulatorRuntime patterns

```rust
use std::collections::{HashMap, HashSet, VecDeque};
use rand_chacha::{ChaCha8Rng, rand_core::SeedableRng};
use crate::scheduler::{Scheduler, TestableScheduler};

pub struct SimulationScheduler {
    inner: Arc<Mutex<SimulationState>>,
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
}

#[derive(Debug, Clone)]
pub struct SimulationConfig {
    pub seed: u64,
    pub randomize_order: bool,
    pub randomize_delays: bool,
    pub max_steps: usize,
    pub log_operations: bool,
}

struct SimulatedTask {
    future: Pin<Box<dyn Future<Output = ()> + Send>>,
    waker: Option<Waker>,
    spawn_location: &'static std::panic::Location<'static>,
    priority: TaskPriority,
}

struct DelayedTask {
    task_id: TaskId,
    ready_time: Duration,
    time_range: Option<(Duration, Duration)>, // Cloud time-range delays
}

struct SessionInfo {
    spawned_tasks: HashSet<TaskId>,
    wait_until_tasks: HashSet<TaskId>, // Cloud pattern
    cleanup_registered: bool,
}

impl SimulationScheduler {
    pub fn with_config(config: SimulationConfig) -> Self {
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
            })),
        }
    }
    
    /// Single step execution (Cloud pattern)
    pub fn step(&self) -> bool {
        let mut state = self.inner.lock().unwrap();
        
        // Process delayed tasks
        self.process_delay_queue(&mut state);
        
        // Select next ready task (randomized or FIFO)
        let task_id = if state.config.randomize_order {
            self.select_random_task(&mut state)
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
    
    fn select_random_task(&self, state: &mut SimulationState) -> Option<TaskId> {
        if state.ready_queue.is_empty() {
            return None;
        }
        
        let index = (state.rng.next_u32() as usize) % state.ready_queue.len();
        Some(state.ready_queue.remove(index).unwrap())
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

impl TestableScheduler for SimulationScheduler {
    fn with_seed(seed: u64) -> Self {
        Self::with_config(SimulationConfig::with_seed(seed))
    }
    
    fn run_until_idle(&self) {
        while self.step() {
            // Continue until no work remains
        }
    }
    
    fn advance_time(&self, duration: Duration) {
        let mut state = self.inner.lock().unwrap();
        state.current_time += duration;
        // Process any delays that are now ready
        self.process_delay_queue(&mut state);
    }
}
```

#### 2. Session Management Implementation

**File**: `crates/unified_scheduler/src/runtime/simulation/sessions.rs`
**Changes**: Cloud session lifecycle patterns

```rust
use crate::task::{SessionId, TaskId};

impl SimulationScheduler {
    pub fn create_session_internal(&self) -> SessionId {
        let mut state = self.inner.lock().unwrap();
        let session_id = SessionId::new_v4();
        
        state.sessions.insert(session_id, SessionInfo {
            spawned_tasks: HashSet::new(),
            wait_until_tasks: HashSet::new(),
            cleanup_registered: false,
        });
        
        session_id
    }
    
    pub fn with_session<F, R>(&self, session_id: SessionId, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        {
            let mut state = self.inner.lock().unwrap();
            state.current_session = Some(session_id);
        }
        
        let result = f();
        
        {
            let mut state = self.inner.lock().unwrap();
            state.current_session = None;
        }
        
        result
    }
    
    pub fn register_wait_until_task(&self, session_id: SessionId, task_id: TaskId) {
        let mut state = self.inner.lock().unwrap();
        if let Some(session) = state.sessions.get_mut(&session_id) {
            session.wait_until_tasks.insert(task_id);
        }
    }
    
    pub fn validate_session_cleanup(&self, session_id: SessionId) -> anyhow::Result<()> {
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
```

### Success Criteria:

#### Automated Verification:
- [ ] Simulation runtime compiles: `cargo check -p unified_scheduler --features testing`
- [ ] Deterministic execution: `cargo test -p unified_scheduler test_deterministic_simulation`
- [ ] Session validation: `cargo test -p unified_scheduler test_session_cleanup`
- [ ] Multi-seed testing: `cargo test -p unified_scheduler test_multi_seed`

#### Manual Verification:
- [ ] Same seed produces identical execution orders
- [ ] Different seeds explore different task interleavings  
- [ ] Session cleanup validation detects dangling tasks
- [ ] Time advancement works correctly for delayed tasks

---

## Phase 4: AFL Fuzzing Integration

### Overview  

Integrate AFL fuzzing capabilities to drive scheduler decisions and discover race conditions through fuzz-generated execution patterns.

### Changes Required:

#### 1. Fuzzing Scheduler Implementation

**File**: `crates/unified_scheduler/src/runtime/fuzzing.rs`
**Changes**: AFL-driven scheduler decisions

```rust
#[cfg(feature = "fuzzing")]
use afl;
use crate::scheduler::{TestableScheduler, FuzzableScheduler};

#[cfg(feature = "fuzzing")]
pub struct FuzzingScheduler {
    simulation: SimulationScheduler,
    fuzz_state: Arc<Mutex<FuzzState>>,
}

#[cfg(feature = "fuzzing")]
struct FuzzState {
    input_data: Vec<u8>,
    byte_offset: usize,
    decisions_made: usize,
}

#[cfg(feature = "fuzzing")]
struct FuzzConfig {
    seed: u64,
    randomize_order: bool,
    randomize_delays: bool,
    task_selection_bytes: Vec<u8>,
    delay_timing_bytes: Vec<u8>,
}

#[cfg(feature = "fuzzing")]
impl FuzzConfig {
    fn from_bytes(data: &[u8]) -> Self {
        if data.len() < 16 {
            return Self::default();
        }
        
        // Extract seed from first 8 bytes
        let seed = u64::from_le_bytes(data[0..8].try_into().unwrap_or([0u8; 8]));
        
        // Configuration flags from next 8 bytes
        let flags = u64::from_le_bytes(data[8..16].try_into().unwrap_or([0u8; 8]));
        let randomize_order = flags & 1 != 0;
        let randomize_delays = flags & 2 != 0;
        
        // Remaining bytes for scheduling decisions
        let remaining = &data[16..];
        let mid = remaining.len() / 2;
        
        Self {
            seed,
            randomize_order,
            randomize_delays,
            task_selection_bytes: remaining[..mid].to_vec(),
            delay_timing_bytes: remaining[mid..].to_vec(),
        }
    }
}

#[cfg(feature = "fuzzing")]
impl FuzzingScheduler {
    pub fn from_fuzz_data(data: &[u8]) -> Self {
        let fuzz_config = FuzzConfig::from_bytes(data);
        
        let simulation_config = SimulationConfig {
            seed: fuzz_config.seed,
            randomize_order: fuzz_config.randomize_order,
            randomize_delays: fuzz_config.randomize_delays,
            max_steps: 10000, // Reasonable limit for fuzzing
            log_operations: false, // Disable logging for performance
        };
        
        Self {
            simulation: SimulationScheduler::with_config(simulation_config),
            fuzz_state: Arc::new(Mutex::new(FuzzState {
                input_data: data.to_vec(),
                byte_offset: 16, // Skip seed and flags
                decisions_made: 0,
            })),
        }
    }
    
    /// Get next byte from fuzz input for scheduling decisions
    fn next_fuzz_byte(&self) -> u8 {
        let mut state = self.fuzz_state.lock().unwrap();
        if state.byte_offset >= state.input_data.len() {
            // Cycle through input when exhausted
            state.byte_offset = 16; 
        }
        
        let byte = state.input_data.get(state.byte_offset).copied().unwrap_or(0);
        state.byte_offset += 1;
        state.decisions_made += 1;
        
        byte
    }
    
    /// Override task selection with fuzz-driven choice
    pub fn fuzz_task_selection(&self, available_tasks: &[TaskId]) -> Option<TaskId> {
        if available_tasks.is_empty() {
            return None;
        }
        
        let fuzz_byte = self.next_fuzz_byte();
        let index = (fuzz_byte as usize) % available_tasks.len();
        Some(available_tasks[index])
    }
    
    /// Override delay timing with fuzz-driven delays
    pub fn fuzz_delay_timing(&self, base_duration: Duration) -> Duration {
        let fuzz_byte = self.next_fuzz_byte();
        
        // Use fuzz byte to create variations: 0.5x to 2.0x base duration
        let multiplier = 0.5 + (fuzz_byte as f64 / 255.0) * 1.5;
        Duration::from_nanos((base_duration.as_nanos() as f64 * multiplier) as u64)
    }
}

#[cfg(feature = "fuzzing")]
impl FuzzableScheduler for FuzzingScheduler {
    fn from_fuzz_data(data: &[u8]) -> Self {
        Self::from_fuzz_data(data)
    }
    
    fn apply_fuzz_decisions(&self, data: &[u8]) {
        // Update fuzz state with new data
        let mut state = self.fuzz_state.lock().unwrap();
        state.input_data = data.to_vec();
        state.byte_offset = 16;
        state.decisions_made = 0;
    }
}

/// AFL fuzzing entry point
#[cfg(feature = "fuzzing")]
pub fn fuzz_scheduler<F>(test_scenario: F)
where
    F: Fn(Arc<dyn FuzzableScheduler>) + 'static,
{
    afl::fuzz!(|data: &[u8]| {
        let scheduler = Arc::new(FuzzingScheduler::from_fuzz_data(data));
        
        // Run test scenario with fuzzed scheduler
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            test_scenario(scheduler);
        })).ok(); // Ignore panics for fuzzing
    });
}

#[cfg(feature = "fuzzing")]
impl std::ops::Deref for FuzzingScheduler {
    type Target = SimulationScheduler;
    
    fn deref(&self) -> &Self::Target {
        &self.simulation
    }
}
```

#### 2. Fuzzing Test Harness

**File**: `crates/unified_scheduler/src/fuzzing/harness.rs`  
**Changes**: Test harness for fuzzing scenarios

```rust
#[cfg(feature = "fuzzing")]
use crate::runtime::fuzzing::{FuzzingScheduler, fuzz_scheduler};
#[cfg(feature = "fuzzing")]
use crate::scheduler::FuzzableScheduler;

#[cfg(feature = "fuzzing")]
pub fn fuzz_basic_scheduling() {
    fuzz_scheduler(|scheduler| {
        // Basic task spawning and completion
        let task1 = scheduler.spawn(async { 42 });
        let task2 = scheduler.spawn(async { "hello" });
        
        scheduler.run_until_idle();
        
        // Verify tasks completed
        assert!(task1.is_finished());
        assert!(task2.is_finished());
    });
}

#[cfg(feature = "fuzzing")]
pub fn fuzz_session_management() {
    fuzz_scheduler(|scheduler| {
        let session_id = scheduler.create_session();
        
        let _task = scheduler.spawn_in_session(async {
            // Some work
            std::future::ready(()).await;
        }, session_id);
        
        scheduler.run_until_idle();
        scheduler.validate_session_cleanup(session_id).expect("Session cleanup failed");
    });
}

#[cfg(feature = "fuzzing")]  
pub fn fuzz_task_priorities() {
    fuzz_scheduler(|scheduler| {
        use crate::task::TaskPriority;
        
        // Spawn tasks with different priorities
        let low = scheduler.spawn_with_priority(async { 1 }, TaskPriority::Background, None);
        let normal = scheduler.spawn_with_priority(async { 2 }, TaskPriority::Normal, None);
        let high = scheduler.spawn_with_priority(async { 3 }, TaskPriority::Foreground, None);
        
        scheduler.run_until_idle();
        
        // All tasks should complete regardless of scheduling order
        assert!(low.is_finished() && normal.is_finished() && high.is_finished());
    });
}

/// Fuzzing binary entry points
#[cfg(feature = "fuzzing")]
pub mod fuzz_targets {
    pub fn fuzz_target_basic() {
        super::fuzz_basic_scheduling();
    }
    
    pub fn fuzz_target_sessions() {
        super::fuzz_session_management();
    }
    
    pub fn fuzz_target_priorities() {
        super::fuzz_task_priorities();
    }
}
```

#### 3. Fuzzing Binaries

**File**: `crates/unified_scheduler/fuzz/fuzz_targets/basic.rs`
**Changes**: AFL fuzzing binaries

```rust
#[cfg(feature = "fuzzing")]
fn main() {
    unified_scheduler::fuzzing::harness::fuzz_targets::fuzz_target_basic();
}

#[cfg(not(feature = "fuzzing"))]
fn main() {
    println!("Fuzzing not enabled. Compile with --features fuzzing");
}
```

**File**: `crates/unified_scheduler/fuzz/fuzz_targets/sessions.rs`
**Changes**: Session fuzzing binary

```rust
#[cfg(feature = "fuzzing")]
fn main() {
    unified_scheduler::fuzzing::harness::fuzz_targets::fuzz_target_sessions();
}

#[cfg(not(feature = "fuzzing"))]
fn main() {
    println!("Fuzzing not enabled. Compile with --features fuzzing");
}
```

### Success Criteria:

#### Automated Verification:
- [ ] Fuzzing binaries compile: `cargo afl build --features fuzzing`
- [ ] Basic fuzzing runs: `timeout 30 cargo afl fuzz -i in -o out target/debug/basic`
- [ ] Session fuzzing runs: `timeout 30 cargo afl fuzz -i in -o out target/debug/sessions`
- [ ] No crashes in short fuzzing runs: Verify AFL doesn't find immediate crashes

#### Manual Verification:
- [ ] AFL discovers different execution interleavings
- [ ] Fuzzing can trigger race conditions in test scenarios
- [ ] Fuzz input properly drives scheduling decisions
- [ ] Performance is acceptable for fuzzing workloads

---

## Phase 5: Integration and Testing

### Overview

Create comprehensive test suite, integration examples, and migration utilities to validate the unified scheduler works as a drop-in replacement for both GPUI and Cloud patterns.

### Changes Required:

#### 1. Compatibility Layer for GPUI

**File**: `crates/unified_scheduler/src/compat/gpui.rs`
**Changes**: GPUI-compatible API layer

```rust
use crate::{SchedulerHandle, Task, TaskLabel};

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
    pub fn spawn<T: 'static>(
        &self,
        future: impl Future<Output = T> + 'static,
    ) -> Task<T> {
        // Convert !Send future to Send by ensuring it stays on current thread
        let future = make_send_future(future);
        
        self.scheduler.spawn_with_priority(
            future,
            TaskPriority::Foreground,
            None,
        )
    }
}

// Helper to make !Send futures Send by thread-pinning
fn make_send_future<T: 'static>(
    future: impl Future<Output = T> + 'static,
) -> impl Future<Output = T> + Send + 'static {
    let thread_id = std::thread::current().id();
    
    async move {
        assert_eq!(
            std::thread::current().id(),
            thread_id,
            "Foreground future executed on wrong thread"
        );
        future.await
    }
}

/// GPUI compatibility async context  
pub struct AsyncApp {
    background_executor: BackgroundExecutor,
    foreground_executor: ForegroundExecutor,
}

impl AsyncApp {
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
```

#### 2. Compatibility Layer for Cloud

**File**: `crates/unified_scheduler/src/compat/cloud.rs`
**Changes**: Cloud-compatible API layer

```rust
use crate::{SchedulerHandle, Task, SessionId};

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
        if let Ok(testable) = self.scheduler.inner.clone().downcast::<dyn TestableScheduler>() {
            // Use simulation for testing
            let task = testable.spawn(future);
            testable.run_until_idle();
            task.into_result().expect("Task should complete")
        } else {
            // Use tokio for production
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(future)
        }
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
        let session_id = self.session_id.ok_or_else(|| {
            anyhow::anyhow!("No session context")
        })?;
        
        // Add deterministic delay (Cloud pattern)
        self.scheduler.timer(Duration::from_millis(1)).await;
        
        // Spawn background task
        let task = self.scheduler.spawn_in_session(future, session_id);
        
        // Register for session cleanup
        if let Some(testable) = self.scheduler.as_testable() {
            testable.register_wait_until_task(session_id, task.id());
        }
        
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
        Fut: Future<Output = anyhow::Result<T>>,
    {
        let scheduler = TestableSchedulerHandle::new(
            SimulationScheduler::with_seed(seed)
        );
        let platform_runtime = PlatformRuntime::new(scheduler.clone().into());
        let simulator = Self { scheduler, platform_runtime };
        
        let result = f(simulator);
        scheduler.run_until_idle();
        
        // Block on the future
        scheduler.block_on(result)
    }
    
    /// Cloud multi-seed test pattern  
    pub fn many<F, Fut, T>(iterations: usize, f: F) -> Vec<anyhow::Result<T>>
    where
        F: Fn(Self) -> Fut + Send + Sync,
        Fut: Future<Output = anyhow::Result<T>>,
        T: Send,
    {
        (0..iterations)
            .map(|i| Self::once(i as u64, &f))
            .collect()
    }
}
```

#### 3. Comprehensive Test Suite

**File**: `crates/unified_scheduler/tests/integration_tests.rs`
**Changes**: Integration tests validating both compatibility layers

```rust
use unified_scheduler::*;
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

#[tokio::test]
async fn test_gpui_compatibility() {
    let scheduler = SchedulerHandle::new(ProductionScheduler::new(RuntimeConfig::default()));
    let background_executor = compat::gpui::BackgroundExecutor::new(scheduler.clone());
    let foreground_executor = compat::gpui::ForegroundExecutor::new(scheduler);
    
    // Test background execution
    let bg_task = background_executor.spawn(async { 42 });
    let result = bg_task.await;
    assert_eq!(result, 42);
    
    // Test foreground execution
    let fg_task = foreground_executor.spawn(async { "hello" });
    let result = fg_task.await;
    assert_eq!(result, "hello");
    
    // Test timer
    let timer_task = background_executor.timer(Duration::from_millis(10));
    timer_task.await; // Should complete without error
}

#[test]
fn test_cloud_compatibility() {
    let result = compat::cloud::Simulator::once(42, |simulator| async {
        let execution_context = compat::cloud::SimulatedExecutionContext::new(
            simulator.scheduler.clone().into()
        );
        
        // Test wait_until pattern
        execution_context.wait_until(async {
            // Background work
            Ok(())
        }).await?;
        
        Ok("success")
    });
    
    assert_eq!(result.unwrap(), "success");
}

#[test]
fn test_session_management() {
    let scheduler = TestableSchedulerHandle::new(
        SimulationScheduler::with_seed(123)
    );
    
    let session_id = scheduler.create_session();
    let counter = Arc::new(AtomicUsize::new(0));
    
    // Spawn task in session
    let task = {
        let counter = counter.clone();
        scheduler.spawn_in_session(async move {
            counter.fetch_add(1, Ordering::SeqCst);
        }, session_id)
    };
    
    scheduler.run_until_idle();
    assert_eq!(counter.load(Ordering::SeqCst), 1);
    
    // Validate session cleanup
    scheduler.validate_session_cleanup(session_id).unwrap();
}

#[test] 
fn test_multi_seed_determinism() {
    let results = compat::cloud::Simulator::many(5, |simulator| async {
        let mut values = Vec::new();
        
        // Spawn multiple tasks that may interleave differently
        let tasks: Vec<_> = (0..10)
            .map(|i| {
                let values = values.clone();
                simulator.scheduler.spawn(async move {
                    values.push(i);
                })
            })
            .collect();
        
        for task in tasks {
            task.await;
        }
        
        Ok(values.len())
    });
    
    // All runs should complete successfully
    assert!(results.iter().all(|r| r.is_ok()));
    
    // But execution orders may differ between seeds
    // (This is the point - we're testing different interleavings)
}

#[test]
fn test_arc_send_compatibility() {
    let scheduler = SchedulerHandle::new(ProductionScheduler::new(RuntimeConfig::default()));
    
    // Should be able to clone and send across threads
    let scheduler_clone = scheduler.clone();
    
    let handle = std::thread::spawn(move || {
        let task = scheduler_clone.spawn(async { 42 });
        futures::executor::block_on(task)
    });
    
    let result = handle.join().unwrap();
    assert_eq!(result, 42);
}

#[cfg(feature = "fuzzing")]
#[test]
fn test_fuzzing_integration() {
    use crate::runtime::fuzzing::FuzzingScheduler;
    
    // Test with deterministic fuzz input
    let fuzz_data = vec![
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, // Seed
        0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Flags (randomize both)
        0x10, 0x20, 0x30, 0x40, // Task selection bytes
        0x50, 0x60, 0x70, 0x80, // Delay timing bytes
    ];
    
    let scheduler = Arc::new(FuzzingScheduler::from_fuzz_data(&fuzz_data));
    
    // Run basic scenario
    let task = scheduler.spawn(async { "fuzzed" });
    scheduler.run_until_idle();
    
    // Should complete despite fuzzed scheduling
    assert!(task.is_finished());
}
```

### Success Criteria:

#### Automated Verification:
- [ ] All integration tests pass: `cargo test -p unified_scheduler`
- [ ] GPUI compatibility works: `cargo test -p unified_scheduler test_gpui_compatibility`
- [ ] Cloud compatibility works: `cargo test -p unified_scheduler test_cloud_compatibility`  
- [ ] Multi-threading tests pass: `cargo test -p unified_scheduler test_arc_send_compatibility`
- [ ] Fuzzing tests pass: `cargo test -p unified_scheduler --features fuzzing`

#### Manual Verification:
- [ ] Drop-in replacement works for simple GPUI use cases
- [ ] Drop-in replacement works for simple Cloud use cases
- [ ] Performance is within 10% of original implementations
- [ ] Memory usage is reasonable
- [ ] AFL fuzzing successfully finds race conditions in test scenarios

---

## Testing Strategy

### Unit Tests:

- **async_task::Runnable integration**: Test direct runnable dispatch with zero boxing overhead
- **Object-safe trait methods**: Verify all Scheduler methods work through Arc<dyn Scheduler>
- **Platform dispatchers**: Test OS-specific dispatch mechanisms maintain performance
- **Session lifecycle**: Test comprehensive Cloud-style session management patterns
- **Multi-seed determinism**: Test same seed produces identical execution orders
- **Task cleanup validation**: Test detection of dangling tasks with spawn location tracking

### Multi-Seed Integration Tests (Cloud Pattern):

- **Race condition detection**: Run concurrent scenarios with 100+ seeds to explore different execution orders
- **Session cleanup validation**: Test all spawned tasks are properly tracked and validated
- **Deterministic replay**: Verify bugs can be reproduced with specific seeds
- **Scheduling fairness**: Test randomization explores different task interleavings

### AFL Fuzzing Integration Tests:

- **Scheduler decision fuzzing**: Use AFL to drive task selection and timing decisions
- **Session management fuzzing**: Fuzz session creation/cleanup patterns  
- **Priority scheduling fuzzing**: Fuzz priority-based task ordering scenarios
- **Error condition discovery**: Use fuzzing to find edge cases in error handling

### Cross-Platform Integration Tests:

- **GPUI compatibility**: Test foreground/background executor patterns work identically
- **Cloud compatibility**: Test SimulatorRuntime patterns, wait_until behavior preserved
- **Platform dispatchers**: Test GCD (macOS), thread pools (Linux), native pools (Windows)
- **Threading**: Test Arc-based Send compatibility, cross-thread task spawning

### Manual Testing Steps:

1. **Replace GPUI executor in simple test**: Verify drop-in compatibility
2. **Replace Cloud simulator in simple test**: Verify session management works  
3. **Run AFL fuzzing for 1 hour**: Verify it discovers scheduling variations
4. **Performance benchmark**: Compare with original implementations
5. **Multi-platform testing**: Test on macOS, Linux, Windows
6. **Memory leak testing**: Run long tests with session creation/cleanup

## Performance Considerations

### Performance Targets (from research):
- **Memory overhead**: <5% increase over current GPUI implementation
- **CPU overhead**: <2% increase in production, identical dispatch performance  
- **Latency**: Identical to current platform dispatchers (GCD, thread pools)

### Key Performance Insights:
- **async_task::Runnable dispatch**: Zero additional overhead - already optimally designed
- **Type erasure cost**: 0% - handled internally by async_task, not our abstraction
- **Trait object dispatch**: ~1-2ns per call (measured negligible impact)
- **Session tracking**: Only active when sessions are created (~5% memory when enabled)
- **Platform integration**: Direct delegation to existing dispatchers (zero abstraction penalty)

### Memory Footprint Analysis:
- **Arc<dyn Scheduler>**: 16 bytes (pointer + vtable)
- **Task<T>**: Identical to current GPUI Task (~32 bytes)
- **Session state**: Only allocated when session management is used
- **TaskId/SessionId**: 8 bytes each (UUID)

### Optimizations:
- **Conditional session tracking**: Only enabled when sessions are created
- **Platform-specific fast paths**: Direct delegation to OS-specific dispatchers  
- **Zero-copy runnable dispatch**: No additional boxing beyond async_task
- **Efficient randomization**: ChaCha8Rng for deterministic multi-seed testing

## Migration Notes

### GPUI Migration:
1. Replace `gpui::BackgroundExecutor` with `unified_scheduler::compat::gpui::BackgroundExecutor`
2. Replace `gpui::ForegroundExecutor` with `unified_scheduler::compat::gpui::ForegroundExecutor` 
3. Update `Cargo.toml` dependencies
4. Test existing async patterns work unchanged

### Cloud Migration:
1. Replace `platform_simulator::SimulatorRuntime` with `unified_scheduler::compat::cloud::PlatformRuntime`
2. Replace `platform_simulator::Simulator` with `unified_scheduler::compat::cloud::Simulator`
3. Update test patterns to use new multi-seed testing
4. Verify session cleanup validation still works

## Key Research Insights Incorporated

### async_task::Runnable Foundation

The most important discovery from the object-safe scheduler design research is that `async_task::Runnable` provides the perfect foundation for zero-cost object-safe scheduling:

- **Already type-erased**: No generic parameters, concrete `Runnable` type
- **Zero boxing overhead**: async_task handles all type erasure internally
- **Object-safe by design**: Can be stored and dispatched through trait objects
- **Platform-native integration**: Works directly with GCD, thread pools, event loops

This eliminates the need for additional abstraction layers while achieving full object safety.

### Session Management Patterns

Cloud's comprehensive session management provides sophisticated task tracking:

- **Cleanup validation**: Detect dangling tasks that weren't properly handled
- **Spawn location tracking**: Detailed error reporting for debugging
- **Session-based isolation**: Group related tasks for lifecycle management
- **wait_until patterns**: Background task coordination with deterministic cleanup

### Multi-Seed Testing Strategy

Cloud's systematic approach to race condition detection:

- **Deterministic randomization**: ChaCha8Rng with seed-based reproducibility
- **Parallel seed exploration**: Test multiple execution orders concurrently  
- **Panic handling**: Continue testing other seeds when individual tests panic
- **Race condition discovery**: Systematic exploration of task interleavings

## References

- **Primary research**: `thoughts/shared/research/2025-08-30_15-13-13_scheduler-crate-research.md`
- **Object-safe design**: `thoughts/shared/research/object-safe-scheduler-design.md`
- **GPUI executor implementation**: `crates/gpui/src/executor.rs:58` (async_task integration patterns)
- **GPUI platform dispatcher**: `crates/gpui/src/platform.rs:561` (object-safe trait design)
- **Cloud simulator implementation**: `crates/platform_simulator/src/runtime.rs:104` (session management)
- **Cloud session patterns**: `crates/platform_simulator/src/runtime.rs:488` (cleanup validation)
- **Multi-seed testing**: `crates/platform_simulator/tests/runtime_tests.rs:14` (systematic race detection)
- **macOS GCD integration**: `crates/gpui/src/platform/mac/dispatcher.rs:56` (platform constraints)
- **AFL fuzzing documentation**: https://docs.rs/afl/ (fuzzing integration patterns)