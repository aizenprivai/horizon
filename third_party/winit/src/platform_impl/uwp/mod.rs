//! Universal Windows Platform backend (Horizon's addition to winit 0.30.13).
//!
//! A UWP app has exactly one window, the `CoreWindow` the system hands to the app's
//! `IFrameworkView`, and its event loop is the window's `CoreDispatcher`. This backend runs
//! `CoreApplication::Run` from [`EventLoop::run`]; the application's handler then runs on the
//! view's UI thread while the calling thread waits for the view to end. Every winit window maps
//! onto that one `CoreWindow`.
//!
//! What it covers: size and scale, focus and visibility, keyboard (with text), mouse buttons,
//! position and wheel, raw mouse motion, cursor hiding and warping, closing, user events and the
//! three control flows. What it leaves out: monitors other than the window's own, window
//! decorations and placement (a UWP window has none the app controls), IME composition, custom
//! cursors, icons, and the app lifecycle's suspend and resume.
//!
//! The Xbox needs two things a desktop UWP app doesn't: its back navigation (the controller's B
//! button) is marked handled, or the console would leave the app for the Home screen; and its
//! layout scaling is turned off, so the window measures real pixels (1920 × 1080) rather than
//! the 960 × 540 view pixels UWP apps get by default there.

use std::fmt::{self, Display, Formatter};

use smol_str::SmolStr;

use crate::dpi::{PhysicalPosition, PhysicalSize};
use crate::keyboard::Key;

pub(crate) use self::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy, OwnedDisplayHandle};
pub use self::window::Window;

mod event_loop;
mod keyboard;
mod window;

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) struct PlatformSpecificEventLoopAttributes {}

/// The id of the one window: every [`Window`] is the same `CoreWindow`.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WindowId(u64);

impl WindowId {
    pub const fn dummy() -> Self {
        WindowId(u64::MAX)
    }

    /// The `CoreWindow`'s id.
    pub(crate) const fn core() -> Self {
        WindowId(1)
    }
}

impl From<WindowId> for u64 {
    fn from(id: WindowId) -> Self {
        id.0
    }
}

impl From<u64> for WindowId {
    fn from(raw: u64) -> Self {
        Self(raw)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DeviceId;

impl DeviceId {
    pub const fn dummy() -> Self {
        DeviceId
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PlatformSpecificWindowAttributes;

/// A failed WinRT call, kept as its message (`windows::core::Error` is not `Clone`).
#[derive(Clone, Debug)]
pub struct OsError(String);

impl OsError {
    pub(crate) fn new(error: windows::core::Error) -> Self {
        Self(error.to_string())
    }
}

impl Display for OsError {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        fmt.write_str(&self.0)
    }
}

pub(crate) use crate::cursor::{
    NoCustomCursor as PlatformCustomCursor, NoCustomCursor as PlatformCustomCursorSource,
};
pub(crate) use crate::icon::NoIcon as PlatformIcon;

/// The display the window fills. UWP gives an app no other monitor, so its size is the window's.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MonitorHandle;

impl MonitorHandle {
    pub fn name(&self) -> Option<String> {
        Some("UWP display".to_owned())
    }

    pub fn size(&self) -> PhysicalSize<u32> {
        event_loop::shared().map_or(PhysicalSize::new(0, 0), |shared| shared.size())
    }

    pub fn position(&self) -> PhysicalPosition<i32> {
        (0, 0).into()
    }

    pub fn scale_factor(&self) -> f64 {
        event_loop::shared().map_or(1.0, |shared| shared.scale())
    }

    pub fn refresh_rate_millihertz(&self) -> Option<u32> {
        // `HdmiDisplayInformation` knows it on the console, but it needs a capability; the game
        // only reads this for statistics.
        None
    }

    pub fn video_modes(&self) -> impl Iterator<Item = VideoModeHandle> {
        let size = self.size().into();
        std::iter::once(VideoModeHandle {
            size,
            bit_depth: 32,
            refresh_rate_millihertz: 60000,
            monitor: self.clone(),
        })
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct VideoModeHandle {
    size: (u32, u32),
    bit_depth: u16,
    refresh_rate_millihertz: u32,
    monitor: MonitorHandle,
}

impl VideoModeHandle {
    pub fn size(&self) -> PhysicalSize<u32> {
        self.size.into()
    }

    pub fn bit_depth(&self) -> u16 {
        self.bit_depth
    }

    pub fn refresh_rate_millihertz(&self) -> u32 {
        self.refresh_rate_millihertz
    }

    pub fn monitor(&self) -> MonitorHandle {
        self.monitor.clone()
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct KeyEventExtra {
    pub key_without_modifiers: Key,
    pub text_with_all_modifiers: Option<SmolStr>,
}
