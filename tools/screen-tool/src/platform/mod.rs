//! Platform-specific window management and display enumeration.

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "windows")]
pub mod windows;

// Re-export the active platform (used when adding Windows support)
#[cfg(target_os = "linux")]
#[allow(unused_imports)]
pub use linux as native;

#[cfg(target_os = "windows")]
#[allow(unused_imports)]
pub use windows as native;
