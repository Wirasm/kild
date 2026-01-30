use serde::{Deserialize, Serialize};

/// Information about a window
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    id: u32,
    title: String,
    app_name: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    is_minimized: bool,
    pid: Option<i32>,
}

impl WindowInfo {
    /// Create a new WindowInfo. Internal use only.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        id: u32,
        title: String,
        app_name: String,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        is_minimized: bool,
        pid: Option<i32>,
    ) -> Self {
        debug_assert!(width > 0, "Window width must be positive");
        debug_assert!(height > 0, "Window height must be positive");

        Self {
            id,
            title,
            app_name,
            x,
            y,
            width,
            height,
            is_minimized,
            pid,
        }
    }

    /// Unique window identifier
    pub fn id(&self) -> u32 {
        self.id
    }

    /// Window title
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Application name that owns this window
    pub fn app_name(&self) -> &str {
        &self.app_name
    }

    /// Window x position
    pub fn x(&self) -> i32 {
        self.x
    }

    /// Window y position
    pub fn y(&self) -> i32 {
        self.y
    }

    /// Window width in pixels
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Window height in pixels
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Whether the window is minimized
    pub fn is_minimized(&self) -> bool {
        self.is_minimized
    }

    /// Process ID of the owning application, if available
    pub fn pid(&self) -> Option<i32> {
        self.pid
    }
}

/// Information about a monitor/display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorInfo {
    id: u32,
    name: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    is_primary: bool,
}

impl MonitorInfo {
    /// Create a new MonitorInfo. Internal use only.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        id: u32,
        name: String,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        is_primary: bool,
    ) -> Self {
        debug_assert!(width > 0, "Monitor width must be positive");
        debug_assert!(height > 0, "Monitor height must be positive");

        Self {
            id,
            name,
            x,
            y,
            width,
            height,
            is_primary,
        }
    }

    /// Unique monitor identifier
    pub fn id(&self) -> u32 {
        self.id
    }

    /// Monitor name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Monitor x position
    pub fn x(&self) -> i32 {
        self.x
    }

    /// Monitor y position
    pub fn y(&self) -> i32 {
        self.y
    }

    /// Monitor width in pixels
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Monitor height in pixels
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Whether this is the primary monitor
    pub fn is_primary(&self) -> bool {
        self.is_primary
    }
}
