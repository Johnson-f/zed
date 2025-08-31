pub mod compat;
pub mod executor;
pub mod platform;
pub mod runtime;
pub mod scheduler;
pub mod task;

#[cfg(feature = "fuzzing")]
pub mod fuzzing;

pub use executor::{SchedulerHandle, TestableSchedulerHandle};
pub use platform::{PlatformDispatcher, DispatcherKind};
pub use runtime::{ProductionScheduler, RuntimeConfig, SimulationScheduler, SimulationConfig};
pub use scheduler::{Scheduler, SchedulerExt, SessionScheduler, TestableScheduler, TestableSchedulerExt, TimerHandle};
pub use task::{Task, TaskId, TaskLabel, TaskPriority, SessionId};

#[cfg(feature = "fuzzing")]
pub use scheduler::FuzzableScheduler;

#[cfg(feature = "fuzzing")]
pub use runtime::{FuzzingScheduler, fuzz_scheduler};