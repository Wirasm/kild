use serde::{Deserialize, Serialize};

/// Information about a window
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    /// Unique window identifier
    pub id: u32,
    /// Window title
    pub title: String,
    /// Application name that owns this window
    pub app_name: String,
    /// Window x position
    pub x: i32,
    /// Window y position
    pub y: i32,
    /// Window width in pixels
    pub width: u32,
    /// Window height in pixels
    pub height: u32,
    /// Whether the window is minimized
    pub is_minimized: bool,
}

/// Information about a monitor/display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorInfo {
    /// Unique monitor identifier
    pub id: u32,
    /// Monitor name
    pub name: String,
    /// Monitor x position
    pub x: i32,
    /// Monitor y position
    pub y: i32,
    /// Monitor width in pixels
    pub width: u32,
    /// Monitor height in pixels
    pub height: u32,
    /// Whether this is the primary monitor
    pub is_primary: bool,
}
