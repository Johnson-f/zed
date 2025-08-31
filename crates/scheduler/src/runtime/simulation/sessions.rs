use std::collections::HashSet;
use crate::task::{SessionId, TaskId};
use crate::runtime::simulation::SimulationScheduler;

impl SimulationScheduler {
    pub fn create_session_internal(&self) -> SessionId {
        let session_id = SessionId::new_v4();
        let mut state = self.inner.lock().unwrap();
        
        state.sessions.insert(session_id, super::SessionInfo {
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
    
    pub fn get_session_tasks(&self, session_id: SessionId) -> Option<(HashSet<TaskId>, HashSet<TaskId>)> {
        let state = self.inner.lock().unwrap();
        if let Some(session) = state.sessions.get(&session_id) {
            Some((session.spawned_tasks.clone(), session.wait_until_tasks.clone()))
        } else {
            None
        }
    }
    
    pub fn cleanup_session(&self, session_id: SessionId) {
        let mut state = self.inner.lock().unwrap();
        
        // Remove session and associated task mappings
        if let Some(session) = state.sessions.remove(&session_id) {
            for task_id in session.spawned_tasks {
                state.task_to_session.remove(&task_id);
            }
        }
    }
    
    /// Get statistics about the current simulation state
    pub fn get_stats(&self) -> SimulationStats {
        let state = self.inner.lock().unwrap();
        SimulationStats {
            steps_executed: state.step_count,
            tasks_ready: state.ready_queue.len(),
            tasks_delayed: state.delay_queue.len(),
            active_sessions: state.sessions.len(),
            current_time: state.current_time,
            randomization_enabled: state.config.randomize_order,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SimulationStats {
    pub steps_executed: usize,
    pub tasks_ready: usize,
    pub tasks_delayed: usize,
    pub active_sessions: usize,
    pub current_time: std::time::Duration,
    pub randomization_enabled: bool,
}