//! The UWP window: a view of the one `CoreWindow`. Its state lives in the event loop's shared
//! state, so every method works from any thread; what needs the window itself (the cursor) is
//! queued for the UI thread's next pass.

use std::collections::VecDeque;
use std::sync::Arc;

use crate::cursor::Cursor;
use crate::dpi::{PhysicalPosition, PhysicalSize, Position, Size};
use crate::error;
use crate::event::WindowEvent;
use crate::platform_impl::Fullscreen;
use crate::window::{self, ImePurpose};

use super::event_loop::{shared, Shared};
use super::{ActiveEventLoop, MonitorHandle, OsError, WindowId};

pub struct Window {
    shared: Arc<Shared>,
}

impl Window {
    pub(crate) fn new(
        _event_loop: &ActiveEventLoop,
        attributes: window::WindowAttributes,
    ) -> Result<Self, error::OsError> {
        let shared = shared()
            .filter(|shared| !shared.core_window().is_null())
            .cloned()
            .ok_or_else(|| os_error!(OsError("the CoreWindow is not ready".to_owned())))?;
        {
            let mut state = shared.lock();
            state.title = attributes.title.clone();
            // The window already has its size: tell the app, as a new desktop window would.
            let size = state.size;
            state.push_window(WindowEvent::Resized(size));
            if state.focused {
                state.push_window(WindowEvent::Focused(true));
            }
            state.redraw = true;
        }
        Ok(Self { shared })
    }

    pub(crate) fn maybe_queue_on_main(&self, f: impl FnOnce(&Self) + Send + 'static) {
        f(self)
    }

    pub(crate) fn maybe_wait_on_main<R: Send>(&self, f: impl FnOnce(&Self) -> R + Send) -> R {
        f(self)
    }

    #[inline]
    pub fn id(&self) -> WindowId {
        WindowId::core()
    }

    #[inline]
    pub fn primary_monitor(&self) -> Option<MonitorHandle> {
        Some(MonitorHandle)
    }

    #[inline]
    pub fn available_monitors(&self) -> VecDeque<MonitorHandle> {
        VecDeque::from([MonitorHandle])
    }

    #[inline]
    pub fn current_monitor(&self) -> Option<MonitorHandle> {
        Some(MonitorHandle)
    }

    #[inline]
    pub fn scale_factor(&self) -> f64 {
        self.shared.scale()
    }

    #[inline]
    pub fn request_redraw(&self) {
        self.shared.lock().redraw = true;
        self.shared.wake();
    }

    #[inline]
    pub fn pre_present_notify(&self) {}

    #[inline]
    pub fn reset_dead_keys(&self) {}

    #[inline]
    pub fn inner_position(&self) -> Result<PhysicalPosition<i32>, error::NotSupportedError> {
        Ok((0, 0).into())
    }

    #[inline]
    pub fn outer_position(&self) -> Result<PhysicalPosition<i32>, error::NotSupportedError> {
        Ok((0, 0).into())
    }

    #[inline]
    pub fn set_outer_position(&self, _position: Position) {}

    #[inline]
    pub fn inner_size(&self) -> PhysicalSize<u32> {
        self.shared.size()
    }

    #[inline]
    pub fn request_inner_size(&self, _size: Size) -> Option<PhysicalSize<u32>> {
        Some(self.inner_size())
    }

    #[inline]
    pub fn outer_size(&self) -> PhysicalSize<u32> {
        self.inner_size()
    }

    #[inline]
    pub fn set_min_inner_size(&self, _: Option<Size>) {}

    #[inline]
    pub fn set_max_inner_size(&self, _: Option<Size>) {}

    #[inline]
    pub fn title(&self) -> String {
        self.shared.lock().title.clone()
    }

    #[inline]
    pub fn set_title(&self, title: &str) {
        self.shared.lock().title = title.to_owned();
    }

    #[inline]
    pub fn set_transparent(&self, _transparent: bool) {}

    #[inline]
    pub fn set_blur(&self, _blur: bool) {}

    #[inline]
    pub fn set_visible(&self, _visible: bool) {}

    #[inline]
    pub fn is_visible(&self) -> Option<bool> {
        Some(self.shared.lock().visible)
    }

    #[inline]
    pub fn resize_increments(&self) -> Option<PhysicalSize<u32>> {
        None
    }

    #[inline]
    pub fn set_resize_increments(&self, _increments: Option<Size>) {}

    #[inline]
    pub fn set_resizable(&self, _resizable: bool) {}

    #[inline]
    pub fn is_resizable(&self) -> bool {
        false
    }

    #[inline]
    pub fn set_minimized(&self, _minimized: bool) {}

    #[inline]
    pub fn is_minimized(&self) -> Option<bool> {
        Some(false)
    }

    #[inline]
    pub fn set_maximized(&self, _maximized: bool) {}

    #[inline]
    pub fn is_maximized(&self) -> bool {
        true
    }

    #[inline]
    pub(crate) fn set_fullscreen(&self, _monitor: Option<Fullscreen>) {}

    #[inline]
    pub(crate) fn fullscreen(&self) -> Option<Fullscreen> {
        // The window always fills its display; saying so keeps apps from resizing it.
        Some(Fullscreen::Borderless(Some(MonitorHandle)))
    }

    #[inline]
    pub fn set_decorations(&self, _decorations: bool) {}

    #[inline]
    pub fn is_decorated(&self) -> bool {
        false
    }

    #[inline]
    pub fn set_window_level(&self, _level: window::WindowLevel) {}

    #[inline]
    pub fn set_window_icon(&self, _window_icon: Option<crate::icon::Icon>) {}

    #[inline]
    pub fn set_ime_cursor_area(&self, _position: Position, _size: Size) {}

    #[inline]
    pub fn set_ime_allowed(&self, _allowed: bool) {}

    #[inline]
    pub fn set_ime_purpose(&self, _purpose: ImePurpose) {}

    #[inline]
    pub fn focus_window(&self) {}

    #[inline]
    pub fn request_user_attention(&self, _request_type: Option<window::UserAttentionType>) {}

    #[inline]
    pub fn set_cursor(&self, _: Cursor) {}

    #[inline]
    pub fn set_cursor_position(&self, position: Position) -> Result<(), error::ExternalError> {
        let mut state = self.shared.lock();
        state.warp = Some(position.to_physical(state.scale));
        drop(state);
        self.shared.wake();
        Ok(())
    }

    #[inline]
    pub fn set_cursor_grab(
        &self,
        mode: window::CursorGrabMode,
    ) -> Result<(), error::ExternalError> {
        // A hidden cursor already gives raw mouse motion; there is no confining a UWP pointer.
        self.shared.lock().cursor_grab = mode;
        Ok(())
    }

    #[inline]
    pub fn set_cursor_visible(&self, visible: bool) {
        let mut state = self.shared.lock();
        if state.cursor_visible != visible {
            state.cursor_visible = visible;
            state.cursor_dirty = true;
        }
        drop(state);
        self.shared.wake();
    }

    #[inline]
    pub fn drag_window(&self) -> Result<(), error::ExternalError> {
        Err(error::ExternalError::NotSupported(
            error::NotSupportedError::new(),
        ))
    }

    #[inline]
    pub fn drag_resize_window(
        &self,
        _direction: window::ResizeDirection,
    ) -> Result<(), error::ExternalError> {
        Err(error::ExternalError::NotSupported(
            error::NotSupportedError::new(),
        ))
    }

    #[inline]
    pub fn show_window_menu(&self, _position: Position) {}

    #[inline]
    pub fn set_cursor_hittest(&self, _hittest: bool) -> Result<(), error::ExternalError> {
        Err(error::ExternalError::NotSupported(
            error::NotSupportedError::new(),
        ))
    }

    #[cfg(feature = "rwh_04")]
    #[inline]
    pub fn raw_window_handle_rwh_04(&self) -> rwh_04::RawWindowHandle {
        let mut handle = rwh_04::WinRtHandle::empty();
        handle.core_window = self.shared.core_window();
        rwh_04::RawWindowHandle::WinRt(handle)
    }

    #[cfg(feature = "rwh_05")]
    #[inline]
    pub fn raw_window_handle_rwh_05(&self) -> rwh_05::RawWindowHandle {
        let mut handle = rwh_05::WinRtWindowHandle::empty();
        handle.core_window = self.shared.core_window();
        rwh_05::RawWindowHandle::WinRt(handle)
    }

    #[cfg(feature = "rwh_05")]
    #[inline]
    pub fn raw_display_handle_rwh_05(&self) -> rwh_05::RawDisplayHandle {
        rwh_05::RawDisplayHandle::Windows(rwh_05::WindowsDisplayHandle::empty())
    }

    #[cfg(feature = "rwh_06")]
    #[inline]
    pub fn raw_window_handle_rwh_06(&self) -> Result<rwh_06::RawWindowHandle, rwh_06::HandleError> {
        let core_window = std::ptr::NonNull::new(self.shared.core_window())
            .ok_or(rwh_06::HandleError::Unavailable)?;
        Ok(rwh_06::RawWindowHandle::WinRt(
            rwh_06::WinRtWindowHandle::new(core_window),
        ))
    }

    #[cfg(feature = "rwh_06")]
    #[inline]
    pub fn raw_display_handle_rwh_06(
        &self,
    ) -> Result<rwh_06::RawDisplayHandle, rwh_06::HandleError> {
        Ok(rwh_06::RawDisplayHandle::Windows(
            rwh_06::WindowsDisplayHandle::new(),
        ))
    }

    #[inline]
    pub fn set_enabled_buttons(&self, _buttons: window::WindowButtons) {}

    #[inline]
    pub fn enabled_buttons(&self) -> window::WindowButtons {
        window::WindowButtons::empty()
    }

    #[inline]
    pub fn theme(&self) -> Option<window::Theme> {
        None
    }

    #[inline]
    pub fn has_focus(&self) -> bool {
        self.shared.lock().focused
    }

    #[inline]
    pub fn set_theme(&self, _theme: Option<window::Theme>) {}

    pub fn set_content_protected(&self, _protected: bool) {}
}
