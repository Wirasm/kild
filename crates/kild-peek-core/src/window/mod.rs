mod errors;
mod handler;
mod types;

pub use errors::WindowError;
pub use handler::{
    find_window_by_app, find_window_by_app_and_title, find_window_by_id, find_window_by_title,
    get_monitor, get_primary_monitor, list_monitors, list_windows,
};
pub use types::{MonitorInfo, WindowInfo};
