use std::time::Duration;
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

pub fn create_platform_dispatcher(kind: DispatcherKind) -> std::sync::Arc<dyn PlatformDispatcher> {
    #[cfg(target_os = "macos")]
    return std::sync::Arc::new(crate::platform::macos::MacOSDispatcher::new(kind));
    
    #[cfg(target_os = "linux")]
    return std::sync::Arc::new(crate::platform::linux::LinuxDispatcher::new(kind));
    
    #[cfg(target_os = "windows")]
    return std::sync::Arc::new(crate::platform::windows::WindowsDispatcher::new(kind));
    
    #[cfg(test)]
    return std::sync::Arc::new(crate::platform::test::TestDispatcher::new(kind));
    
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows", test)))]
    return std::sync::Arc::new(crate::platform::generic::GenericDispatcher::new(kind));
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

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows", test)))]
pub mod generic;