//! This system's dictation backend, chosen at compile time. See `backend::Backend`.

#[cfg(target_os = "linux")]
pub use super::linux::{Os, Target};
#[cfg(target_os = "macos")]
pub use super::macos::{Os, Target};
#[cfg(windows)]
pub use super::win::{target::Target, Os};
