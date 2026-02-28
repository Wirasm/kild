//! Notification backend implementations.

mod linux;
mod macos;

pub use linux::LinuxNotificationBackend;
pub use macos::MacOsNotificationBackend;
