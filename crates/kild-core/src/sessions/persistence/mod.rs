//! Session file persistence
//!
//! Handles reading/writing session data to disk with atomic operations.

mod index;
mod patching;
mod session_files;
mod sidecar;
#[cfg(test)]
mod tests;

pub use patching::{patch_session_json_field, patch_session_json_fields};
pub use session_files::{
    ensure_sessions_directory, find_session_by_name, load_session_from_file,
    load_sessions_from_files, remove_session_file, save_session_to_file,
};
pub use sidecar::{
    read_agent_status, read_pr_info, remove_agent_status_file, remove_pr_info_file,
    write_agent_status, write_pr_info,
};
