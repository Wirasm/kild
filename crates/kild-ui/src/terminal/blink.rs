use std::time::Duration;

use gpui::{Context, Task};

use super::terminal_view::TerminalView;

const BLINK_INTERVAL: Duration = Duration::from_millis(500);

/// Manages cursor blink state with epoch-based timer cancellation.
///
/// Constructed in an inert state. Call `enable()` to start blinking and
/// `disable()` to stop. `TerminalView` drives the lifecycle from `render()`
/// based on focus state — blinking only runs while the terminal is focused.
///
/// Each `enable()` or `reset()` call increments the epoch and spawns a new
/// async blink cycle via `cx.spawn()`. Old cycles detect the stale epoch
/// and exit, so at most one timer drives repaints.
pub(super) struct BlinkManager {
    visible: bool,
    enabled: bool,
    epoch: usize,
    /// Dropping a GPUI `Task` detaches it (the future keeps running until it
    /// exits). Stored here so the current cycle is accessible for replacement.
    _task: Option<Task<()>>,
}

impl BlinkManager {
    pub fn new() -> Self {
        Self {
            visible: true,
            enabled: false,
            epoch: 0,
            _task: None,
        }
    }

    /// Whether the cursor should be painted this frame.
    ///
    /// Returns `true` when blinking is disabled (cursor always on) or when
    /// the blink cycle is in its visible phase.
    pub fn visible(&self) -> bool {
        !self.enabled || self.visible
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Start blinking. Resets visibility and spawns a new blink cycle.
    pub fn enable(&mut self, cx: &mut Context<TerminalView>) {
        self.enabled = true;
        self.visible = true;
        self.start_cycle(cx);
    }

    /// Stop blinking and keep the cursor permanently visible.
    pub fn disable(&mut self) {
        self.enabled = false;
        self.visible = true;
        self.epoch = self.epoch.wrapping_add(1);
        self._task = None;
    }

    /// Reset cursor to visible and restart the blink timer from zero.
    ///
    /// Call on every keystroke so the cursor stays solid during typing and
    /// only resumes blinking after a full 500ms of inactivity.
    pub fn reset(&mut self, cx: &mut Context<TerminalView>) {
        if !self.enabled {
            return;
        }
        self.visible = true;
        self.start_cycle(cx);
    }

    /// Toggle visibility if this cycle is still current. Returns `false` when
    /// the epoch is stale, signaling the caller to exit the blink loop.
    fn toggle_if_current(&mut self, epoch: usize, cx: &mut Context<TerminalView>) -> bool {
        if self.epoch != epoch {
            return false;
        }
        self.visible = !self.visible;
        cx.notify();
        true
    }

    fn start_cycle(&mut self, cx: &mut Context<TerminalView>) {
        self.epoch = self.epoch.wrapping_add(1);
        let epoch = self.epoch;

        self._task = Some(cx.spawn(async move |this, cx: &mut gpui::AsyncApp| {
            loop {
                cx.background_executor().timer(BLINK_INTERVAL).await;

                let should_continue =
                    match this.update(cx, |view, cx| view.blink.toggle_if_current(epoch, cx)) {
                        Ok(cont) => cont,
                        Err(_) => break, // view released — normal teardown
                    };

                if !should_continue {
                    break;
                }
            }
        }));
    }
}
