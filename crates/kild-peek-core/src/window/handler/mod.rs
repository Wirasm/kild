mod builders;
mod find;
mod list;
mod monitors;

pub use find::{
    find_window_by_app, find_window_by_app_and_title, find_window_by_app_and_title_with_wait,
    find_window_by_app_with_wait, find_window_by_id, find_window_by_title,
    find_window_by_title_with_wait,
};
pub use list::{list_monitors, list_windows};
pub use monitors::{get_monitor, get_primary_monitor};

#[cfg(test)]
mod tests;
