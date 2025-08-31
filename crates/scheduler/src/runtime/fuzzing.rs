#[cfg(feature = "fuzzing")]
use afl;

use std::sync::{Arc, Mutex};
use std::time::Duration;
use crate::scheduler::{Scheduler, TestableScheduler, FuzzableScheduler, TimerHandle};
use crate::runtime::simulation::{SimulationScheduler, SimulationConfig};
use crate::task::{TaskId, TaskLabel, SessionId};

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
impl Default for FuzzConfig {
    fn default() -> Self {
        Self {
            seed: 42,
            randomize_order: true,
            randomize_delays: true,
            task_selection_bytes: Vec::new(),
            delay_timing_bytes: Vec::new(),
        }
    }
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
    pub fn fuzz_delay_timing(&self, base_duration: std::time::Duration) -> std::time::Duration {
        let fuzz_byte = self.next_fuzz_byte();
        
        // Use fuzz byte to create variations: 0.5x to 2.0x base duration
        let multiplier = 0.5 + (fuzz_byte as f64 / 255.0) * 1.5;
        std::time::Duration::from_nanos((base_duration.as_nanos() as f64 * multiplier) as u64)
    }
}

#[cfg(feature = "fuzzing")]
impl std::ops::Deref for FuzzingScheduler {
    type Target = SimulationScheduler;
    
    fn deref(&self) -> &Self::Target {
        &self.simulation
    }
}

#[cfg(feature = "fuzzing")]
impl Scheduler for FuzzingScheduler {
    fn dispatch_background(&self, runnable: async_task::Runnable, label: Option<TaskLabel>) {
        self.simulation.dispatch_background(runnable, label)
    }

    fn dispatch_foreground(&self, runnable: async_task::Runnable) {
        self.simulation.dispatch_foreground(runnable)
    }

    fn dispatch_after(&self, duration: Duration, runnable: async_task::Runnable) {
        self.simulation.dispatch_after(duration, runnable)
    }

    fn create_timer(&self, duration: Duration) -> TimerHandle {
        self.simulation.create_timer(duration)
    }

    fn is_main_thread(&self) -> bool {
        self.simulation.is_main_thread()
    }

    fn now(&self) -> std::time::Instant {
        self.simulation.now()
    }

    fn clone_scheduler(&self) -> Box<dyn Scheduler> {
        // Create a new fuzzing scheduler with the same fuzz data
        let fuzz_data = self.fuzz_state.lock().unwrap().input_data.clone();
        Box::new(FuzzingScheduler::from_fuzz_data(&fuzz_data))
    }

    fn create_session(&self) -> SessionId {
        self.simulation.create_session()
    }

    fn validate_session_cleanup(&self, session_id: SessionId) -> anyhow::Result<()> {
        self.simulation.validate_session_cleanup(session_id)
    }
}

#[cfg(feature = "fuzzing")]
impl TestableScheduler for FuzzingScheduler {
    fn advance_time(&self, duration: Duration) {
        self.simulation.advance_time(duration)
    }

    fn run_until_idle(&self) {
        self.simulation.run_until_idle()
    }

    fn step(&self) -> bool {
        self.simulation.step()
    }

    fn set_randomization(&self, enabled: bool) {
        self.simulation.set_randomization(enabled)
    }

    fn with_seed(seed: u64) -> Self {
        Self::from_fuzz_data(&seed.to_le_bytes())
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
    F: Fn(Arc<dyn FuzzableScheduler>) + 'static + std::panic::RefUnwindSafe,
{
    afl::fuzz!(|data: &[u8]| {
        let scheduler = Arc::new(FuzzingScheduler::from_fuzz_data(data));
        
        // Run test scenario with fuzzed scheduler
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            test_scenario(scheduler);
        }));
        // Ignore panics and errors for fuzzing - we want to explore all paths
    });
}

// Stub implementations for when fuzzing is not enabled
#[cfg(not(feature = "fuzzing"))]
pub struct FuzzingScheduler;

#[cfg(not(feature = "fuzzing"))]
impl FuzzingScheduler {
    pub fn from_fuzz_data(_data: &[u8]) -> Self {
        Self
    }
}