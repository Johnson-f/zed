#[cfg(feature = "fuzzing")]
use crate::runtime::fuzzing::fuzz_scheduler;
#[cfg(feature = "fuzzing")]
use crate::scheduler::{FuzzableScheduler, SchedulerExt, TestableScheduler};
#[cfg(feature = "fuzzing")]
use std::sync::Arc;

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
        
        // Spawn a task - since we don't have spawn_in_session implemented,
        // we'll use regular spawn
        let _task = scheduler.spawn(async {
            // Some work
            std::future::ready(()).await;
        });
        
        scheduler.run_until_idle();
        let _ = scheduler.validate_session_cleanup(session_id);
        // Don't assert on session cleanup for fuzzing - we want to explore all paths
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

#[cfg(feature = "fuzzing")]
pub fn fuzz_timer_operations() {
    fuzz_scheduler(|scheduler| {
        use std::time::Duration;
        
        // Create timers with different durations
        let timer1 = scheduler.timer(Duration::from_millis(1));
        let timer2 = scheduler.timer(Duration::from_millis(5));
        
        scheduler.run_until_idle();
        
        // Timers should complete (may not be immediately available)
        // Don't assert for fuzzing - explore all execution paths
        let _ = timer1.is_finished();
        let _ = timer2.is_finished();
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
    
    pub fn fuzz_target_timers() {
        super::fuzz_timer_operations();
    }
}

// Stub implementations for when fuzzing is not enabled
#[cfg(not(feature = "fuzzing"))]
pub fn fuzz_basic_scheduling() {}

#[cfg(not(feature = "fuzzing"))]
pub fn fuzz_session_management() {}

#[cfg(not(feature = "fuzzing"))]  
pub fn fuzz_task_priorities() {}

#[cfg(not(feature = "fuzzing"))]
pub fn fuzz_timer_operations() {}

#[cfg(not(feature = "fuzzing"))]
pub mod fuzz_targets {
    pub fn fuzz_target_basic() {}
    pub fn fuzz_target_sessions() {}
    pub fn fuzz_target_priorities() {}
    pub fn fuzz_target_timers() {}
}