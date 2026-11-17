//! The UWP event loop: `CoreApplication::Run` with a framework view whose `Run` drives the
//! application's handler on the view's UI thread.
//!
//! The `CoreWindow`'s event handlers only queue winit events in [`Shared`]; the loop pumps the
//! dispatcher, then hands the queue, user events, the redraw and `AboutToWait` to the handler,
//! the same order winit's other backends use. Window calls from other threads (a redraw
//! request, a user event) wake a waiting loop through the dispatcher's `RunAsync`.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::ffi::c_void;
use std::marker::PhantomData;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::{mpsc, Arc, Mutex, MutexGuard, OnceLock};
use std::thread::{self, ThreadId};
use std::time::{Duration, Instant};

use smol_str::SmolStr;
use windows::core::{implement, Interface, Ref, Result as WinResult, HSTRING};
use windows::ApplicationModel::Core::{
    CoreApplication, CoreApplicationView, IFrameworkView, IFrameworkViewSource,
    IFrameworkViewSource_Impl, IFrameworkView_Impl,
};
use windows::Devices::Input::{MouseDevice, MouseEventArgs};
use windows::Foundation::{EventHandler, Point, TypedEventHandler};
use windows::Graphics::Display::DisplayInformation;
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};
use windows::UI::Core::{
    AcceleratorKeyEventArgs, BackRequestedEventArgs, CharacterReceivedEventArgs,
    CoreAcceleratorKeyEventType, CoreCursor, CoreCursorType, CoreDispatcher,
    CoreDispatcherPriority, CorePhysicalKeyStatus, CoreProcessEventsOption, CoreWindow,
    CoreWindowActivationState, CoreWindowEventArgs, DispatchedHandler, KeyEventArgs,
    PointerEventArgs, SystemNavigationManager, VisibilityChangedEventArgs,
    WindowActivatedEventArgs, WindowSizeChangedEventArgs,
};
use windows::UI::Input::PointerPoint;
use windows::UI::ViewManagement::ApplicationViewScaling;

use crate::dpi::{PhysicalPosition, PhysicalSize};
use crate::error::EventLoopError;
use crate::event::{
    self, DeviceEvent, ElementState, KeyEvent, Modifiers, MouseButton, MouseScrollDelta,
    StartCause, TouchPhase, WindowEvent,
};
use crate::event_loop::{self, ControlFlow, DeviceEvents};
use crate::keyboard::{
    Key, KeyCode, ModifiersKeys, ModifiersState, NamedKey, NativeKey, PhysicalKey,
};
use crate::window::{
    CursorGrabMode, CustomCursor as RootCustomCursor, CustomCursorSource, Theme,
    WindowId as RootWindowId,
};

use super::{
    keyboard, DeviceId, KeyEventExtra, MonitorHandle, OsError, PlatformSpecificEventLoopAttributes,
    WindowId,
};

/// The state the `CoreWindow`'s handlers, the loop and [`super::Window`] share. One per process:
/// UWP has one main view, and winit allows one event loop.
pub(crate) struct Shared {
    state: Mutex<State>,
    /// The window's dispatcher, for waking the loop from other threads.
    dispatcher: Mutex<Option<CoreDispatcher>>,
    /// The `CoreWindow`'s interface pointer, for raw window handles. The UI thread's
    /// [`CORE_WINDOW`] holds the reference that keeps it alive.
    core_window: AtomicPtr<c_void>,
    ui_thread: OnceLock<ThreadId>,
}

static SHARED: OnceLock<Arc<Shared>> = OnceLock::new();

thread_local! {
    /// The `CoreWindow`, on the UI thread only: WinRT window objects belong to their thread.
    static CORE_WINDOW: RefCell<Option<CoreWindow>> = const { RefCell::new(None) };
}

/// The process's shared state, once an event loop exists.
pub(super) fn shared() -> Option<&'static Arc<Shared>> {
    SHARED.get()
}

impl Shared {
    pub(super) fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(super) fn size(&self) -> PhysicalSize<u32> {
        self.lock().size
    }

    pub(super) fn scale(&self) -> f64 {
        self.lock().scale
    }

    pub(super) fn core_window(&self) -> *mut c_void {
        self.core_window.load(Ordering::Acquire)
    }

    pub(super) fn on_ui_thread(&self) -> bool {
        self.ui_thread.get() == Some(&thread::current().id())
    }

    /// Wakes a loop waiting in the dispatcher. From the UI thread the loop checks its flags before
    /// it waits, so only other threads need the dispatcher.
    pub(super) fn wake(&self) {
        self.lock().woken = true;
        if self.on_ui_thread() {
            return;
        }
        let dispatcher = self.dispatcher.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(dispatcher) = dispatcher.as_ref() {
            let _ = dispatcher.RunAsync(
                CoreDispatcherPriority::Normal,
                &DispatchedHandler::new(|| Ok(())),
            );
        }
    }

    /// Whether the loop has something to deliver, so it must not wait.
    fn has_work(&self) -> bool {
        let state = self.lock();
        !state.queue.is_empty() || state.pending_key.is_some() || state.redraw || state.woken
    }
}

/// A queued event, delivered on the loop's next pass.
pub(super) enum Pending {
    Window(WindowEvent),
    Device(DeviceEvent),
}

pub(super) struct State {
    queue: VecDeque<Pending>,
    /// A key press waiting for the `CharacterReceived` event that carries its text.
    pending_key: Option<KeyEvent>,
    /// The first half of a UTF-16 surrogate pair from `CharacterReceived`.
    high_surrogate: Option<u16>,
    pub(super) size: PhysicalSize<u32>,
    pub(super) scale: f64,
    pub(super) focused: bool,
    pub(super) visible: bool,
    cursor_position: Option<PhysicalPosition<f64>>,
    /// Left, right, middle, back, forward.
    buttons: [bool; 5],
    modifiers: ModifiersKeys,
    pub(super) redraw: bool,
    woken: bool,
    // Window requests, applied on the UI thread by the loop.
    pub(super) cursor_visible: bool,
    pub(super) cursor_grab: CursorGrabMode,
    pub(super) cursor_dirty: bool,
    pub(super) warp: Option<PhysicalPosition<f64>>,
    pub(super) title: String,
}

impl State {
    fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            pending_key: None,
            high_surrogate: None,
            size: PhysicalSize::new(0, 0),
            scale: 1.0,
            focused: false,
            visible: true,
            cursor_position: None,
            buttons: [false; 5],
            modifiers: ModifiersKeys::empty(),
            redraw: false,
            woken: false,
            cursor_visible: true,
            cursor_grab: CursorGrabMode::None,
            cursor_dirty: false,
            warp: None,
            title: String::new(),
        }
    }

    pub(super) fn push_window(&mut self, event: WindowEvent) {
        self.queue.push_back(Pending::Window(event));
    }

    fn push_key(&mut self, event: KeyEvent) {
        self.push_window(WindowEvent::KeyboardInput {
            device_id: event::DeviceId(DeviceId),
            event,
            is_synthetic: false,
        });
    }

    /// Queues the key press still waiting for its text.
    fn flush_key(&mut self) {
        if let Some(event) = self.pending_key.take() {
            self.push_key(event);
        }
    }

    /// A key went down or up.
    fn key(&mut self, vk: i32, status: CorePhysicalKeyStatus, pressed: bool) {
        if keyboard::is_gamepad(vk) {
            return;
        }
        self.flush_key();
        let physical = keyboard::physical_key(status.ScanCode, status.IsExtendedKey);
        let modifiers_before = self.modifiers;
        if let PhysicalKey::Code(code) = physical {
            if let Some(flag) = modifier_flag(code) {
                self.modifiers.set(flag, pressed);
            }
        }
        let named = keyboard::named_key(vk);
        let logical = match named {
            Some(named) => Key::Named(named),
            None => keyboard::base_char(vk)
                .map_or(Key::Unidentified(NativeKey::Windows(vk as u16)), |c| {
                    Key::Character(SmolStr::new(c.encode_utf8(&mut [0; 4])))
                }),
        };
        let event = KeyEvent {
            physical_key: physical,
            logical_key: logical.clone(),
            text: None,
            location: keyboard::location(physical),
            state: if pressed {
                ElementState::Pressed
            } else {
                ElementState::Released
            },
            repeat: pressed && status.WasKeyDown,
            platform_specific: KeyEventExtra {
                key_without_modifiers: logical,
                text_with_all_modifiers: None,
            },
        };
        // A press that can write waits for its `CharacterReceived`, which the dispatcher raises
        // right after it; winit reports the text on the key event itself.
        let writes = pressed
            && named.map_or(true, |named| {
                matches!(
                    named,
                    NamedKey::Enter
                        | NamedKey::Tab
                        | NamedKey::Backspace
                        | NamedKey::Escape
                        | NamedKey::Space
                )
            });
        if writes {
            self.pending_key = Some(event);
        } else {
            self.push_key(event);
        }
        if self.modifiers != modifiers_before {
            self.push_window(WindowEvent::ModifiersChanged(modifiers(self.modifiers)));
        }
    }

    /// A UTF-16 code unit of typed text.
    fn character(&mut self, unit: u32) {
        let unit = unit as u16;
        let c = match unit {
            0xd800..=0xdbff => {
                self.high_surrogate = Some(unit);
                return;
            }
            0xdc00..=0xdfff => match self.high_surrogate.take() {
                Some(high) => char::decode_utf16([high, unit]).next().and_then(Result::ok),
                None => None,
            },
            _ => char::from_u32(u32::from(unit)),
        };
        let Some(c) = c else {
            return;
        };
        let text = SmolStr::new(c.encode_utf8(&mut [0; 4]));
        // Like winit on desktop Windows: Ctrl's control characters are not text, but Enter, Tab,
        // Backspace and Escape write theirs.
        let control = c.is_control() && !matches!(c, '\r' | '\t' | '\u{8}' | '\u{1b}');
        match self.pending_key.as_mut() {
            Some(event) if event.text.is_none() => {
                event.platform_specific.text_with_all_modifiers = Some(text.clone());
                if !control {
                    if !matches!(event.logical_key, Key::Named(_)) {
                        event.logical_key = Key::Character(text.clone());
                    }
                    event.text = Some(text);
                }
            }
            // Text with no key, such as the on-screen keyboard's: a press of no physical key.
            _ if !control => {
                let logical = Key::Character(text.clone());
                self.push_key(KeyEvent {
                    physical_key: PhysicalKey::Unidentified(
                        crate::keyboard::NativeKeyCode::Unidentified,
                    ),
                    logical_key: logical.clone(),
                    text: Some(text.clone()),
                    location: crate::keyboard::KeyLocation::Standard,
                    state: ElementState::Pressed,
                    repeat: false,
                    platform_specific: KeyEventExtra {
                        key_without_modifiers: logical,
                        text_with_all_modifiers: Some(text),
                    },
                });
            }
            _ => {}
        }
        self.flush_key();
    }

    /// A pointer moved or changed buttons: the cursor position, then each button that changed.
    fn pointer(&mut self, point: &PointerPoint) -> WinResult<()> {
        let position = point.Position()?;
        let position = PhysicalPosition::new(
            position.X as f64 * self.scale,
            position.Y as f64 * self.scale,
        );
        if self.cursor_position != Some(position) {
            self.cursor_position = Some(position);
            self.push_window(WindowEvent::CursorMoved {
                device_id: event::DeviceId(DeviceId),
                position,
            });
        }
        let properties = point.Properties()?;
        let now = [
            properties.IsLeftButtonPressed()?,
            properties.IsRightButtonPressed()?,
            properties.IsMiddleButtonPressed()?,
            properties.IsXButton1Pressed()?,
            properties.IsXButton2Pressed()?,
        ];
        const BUTTONS: [MouseButton; 5] = [
            MouseButton::Left,
            MouseButton::Right,
            MouseButton::Middle,
            MouseButton::Back,
            MouseButton::Forward,
        ];
        for ((was, is), button) in self.buttons.iter().zip(now).zip(BUTTONS) {
            if *was != is {
                self.queue
                    .push_back(Pending::Window(WindowEvent::MouseInput {
                        device_id: event::DeviceId(DeviceId),
                        state: if is {
                            ElementState::Pressed
                        } else {
                            ElementState::Released
                        },
                        button,
                    }));
            }
        }
        self.buttons = now;
        Ok(())
    }

    fn wheel(&mut self, point: &PointerPoint) -> WinResult<()> {
        let properties = point.Properties()?;
        let lines = properties.MouseWheelDelta()? as f32 / 120.0;
        let delta = if properties.IsHorizontalMouseWheel()? {
            MouseScrollDelta::LineDelta(lines, 0.0)
        } else {
            MouseScrollDelta::LineDelta(0.0, lines)
        };
        self.push_window(WindowEvent::MouseWheel {
            device_id: event::DeviceId(DeviceId),
            delta,
            phase: TouchPhase::Moved,
        });
        Ok(())
    }
}

fn modifier_flag(code: KeyCode) -> Option<ModifiersKeys> {
    Some(match code {
        KeyCode::ShiftLeft => ModifiersKeys::LSHIFT,
        KeyCode::ShiftRight => ModifiersKeys::RSHIFT,
        KeyCode::ControlLeft => ModifiersKeys::LCONTROL,
        KeyCode::ControlRight => ModifiersKeys::RCONTROL,
        KeyCode::AltLeft => ModifiersKeys::LALT,
        KeyCode::AltRight => ModifiersKeys::RALT,
        KeyCode::SuperLeft => ModifiersKeys::LSUPER,
        KeyCode::SuperRight => ModifiersKeys::RSUPER,
        _ => return None,
    })
}

fn modifiers(keys: ModifiersKeys) -> Modifiers {
    let mut state = ModifiersState::empty();
    state.set(
        ModifiersState::SHIFT,
        keys.intersects(ModifiersKeys::LSHIFT | ModifiersKeys::RSHIFT),
    );
    state.set(
        ModifiersState::CONTROL,
        keys.intersects(ModifiersKeys::LCONTROL | ModifiersKeys::RCONTROL),
    );
    state.set(
        ModifiersState::ALT,
        keys.intersects(ModifiersKeys::LALT | ModifiersKeys::RALT),
    );
    state.set(
        ModifiersState::SUPER,
        keys.intersects(ModifiersKeys::LSUPER | ModifiersKeys::RSUPER),
    );
    Modifiers {
        state,
        pressed_mods: keys,
    }
}

/// View pixels to physical pixels.
fn physical(width: f32, height: f32, scale: f64) -> PhysicalSize<u32> {
    PhysicalSize::new(
        (width as f64 * scale).round().max(1.0) as u32,
        (height as f64 * scale).round().max(1.0) as u32,
    )
}

/// Hooks the `CoreWindow`'s events to the shared state. Runs on the UI thread, from the view's
/// `SetWindow`.
fn install(window: &CoreWindow) -> WinResult<()> {
    let Some(shared) = shared().cloned() else {
        return Ok(());
    };
    let _ = shared.ui_thread.set(thread::current().id());
    let scale = DisplayInformation::GetForCurrentView()
        .and_then(|display| display.RawPixelsPerViewPixel())
        .unwrap_or(1.0);
    let bounds = window.Bounds()?;
    {
        let mut state = shared.lock();
        state.scale = scale;
        state.size = physical(bounds.Width, bounds.Height, scale);
    }
    let dispatcher = window.Dispatcher()?;
    *shared.dispatcher.lock().unwrap_or_else(|p| p.into_inner()) = Some(dispatcher.clone());
    shared.core_window.store(window.as_raw(), Ordering::Release);
    CORE_WINDOW.with(|slot| *slot.borrow_mut() = Some(window.clone()));

    let s = shared.clone();
    window.SizeChanged(
        &TypedEventHandler::<CoreWindow, WindowSizeChangedEventArgs>::new(move |_, args| {
            let size = args.ok()?.Size()?;
            let mut state = s.lock();
            let size = physical(size.Width, size.Height, state.scale);
            if size != state.size {
                state.size = size;
                state.push_window(WindowEvent::Resized(size));
                state.redraw = true;
            }
            Ok(())
        }),
    )?;
    let s = shared.clone();
    window.Activated(
        &TypedEventHandler::<CoreWindow, WindowActivatedEventArgs>::new(move |_, args| {
            let focused =
                args.ok()?.WindowActivationState()? != CoreWindowActivationState::Deactivated;
            let mut state = s.lock();
            if focused != state.focused {
                state.focused = focused;
                state.push_window(WindowEvent::Focused(focused));
            }
            Ok(())
        }),
    )?;
    let s = shared.clone();
    window.VisibilityChanged(
        &TypedEventHandler::<CoreWindow, VisibilityChangedEventArgs>::new(move |_, args| {
            let visible = args.ok()?.Visible()?;
            let mut state = s.lock();
            state.visible = visible;
            state.push_window(WindowEvent::Occluded(!visible));
            Ok(())
        }),
    )?;
    let s = shared.clone();
    window.Closed(&TypedEventHandler::<CoreWindow, CoreWindowEventArgs>::new(
        move |_, _| {
            s.lock().push_window(WindowEvent::CloseRequested);
            Ok(())
        },
    ))?;
    for pressed in [true, false] {
        let s = shared.clone();
        let handler = TypedEventHandler::<CoreWindow, KeyEventArgs>::new(move |_, args| {
            let args = args.ok()?;
            s.lock()
                .key(args.VirtualKey()?.0, args.KeyStatus()?, pressed);
            Ok(())
        });
        if pressed {
            window.KeyDown(&handler)?;
        } else {
            window.KeyUp(&handler)?;
        }
    }
    let s = shared.clone();
    window.CharacterReceived(
        &TypedEventHandler::<CoreWindow, CharacterReceivedEventArgs>::new(move |_, args| {
            s.lock().character(args.ok()?.KeyCode()?);
            Ok(())
        }),
    )?;
    // Alt and F10 are system keys: the window's KeyDown never sees them.
    let s = shared.clone();
    dispatcher.AcceleratorKeyActivated(&TypedEventHandler::<
        CoreDispatcher,
        AcceleratorKeyEventArgs,
    >::new(move |_, args| {
        let args = args.ok()?;
        let kind = args.EventType()?;
        let pressed = kind == CoreAcceleratorKeyEventType::SystemKeyDown;
        if pressed || kind == CoreAcceleratorKeyEventType::SystemKeyUp {
            s.lock()
                .key(args.VirtualKey()?.0, args.KeyStatus()?, pressed);
        }
        Ok(())
    }))?;
    for which in 0..3 {
        let s = shared.clone();
        let handler = TypedEventHandler::<CoreWindow, PointerEventArgs>::new(move |_, args| {
            s.lock().pointer(&args.ok()?.CurrentPoint()?)
        });
        match which {
            0 => window.PointerMoved(&handler)?,
            1 => window.PointerPressed(&handler)?,
            _ => window.PointerReleased(&handler)?,
        };
    }
    let s = shared.clone();
    window.PointerWheelChanged(&TypedEventHandler::<CoreWindow, PointerEventArgs>::new(
        move |_, args| s.lock().wheel(&args.ok()?.CurrentPoint()?),
    ))?;
    for entered in [true, false] {
        let s = shared.clone();
        let handler = TypedEventHandler::<CoreWindow, PointerEventArgs>::new(move |_, _| {
            let device_id = event::DeviceId(DeviceId);
            s.lock().push_window(if entered {
                WindowEvent::CursorEntered { device_id }
            } else {
                WindowEvent::CursorLeft { device_id }
            });
            Ok(())
        });
        if entered {
            window.PointerEntered(&handler)?;
        } else {
            window.PointerExited(&handler)?;
        }
    }
    // Raw mouse motion, for mouse look while the cursor is hidden.
    if let Ok(mouse) = MouseDevice::GetForCurrentView() {
        let s = shared.clone();
        mouse.MouseMoved(&TypedEventHandler::<MouseDevice, MouseEventArgs>::new(
            move |_, args| {
                let delta = args.ok()?.MouseDelta()?;
                s.lock()
                    .queue
                    .push_back(Pending::Device(DeviceEvent::MouseMotion {
                        delta: (f64::from(delta.X), f64::from(delta.Y)),
                    }));
                Ok(())
            },
        ))?;
    }
    // The Xbox's B button asks to go back; unhandled, the console leaves the app.
    if let Ok(navigation) = SystemNavigationManager::GetForCurrentView() {
        navigation.BackRequested(&EventHandler::<BackRequestedEventArgs>::new(|_, args| {
            if let Some(args) = args.as_ref() {
                args.SetHandled(true)?;
            }
            Ok(())
        }))?;
    }
    Ok(())
}

/// Applies the window requests made since the last pass: the cursor's look and position.
fn apply_requests(shared: &Shared, window: &CoreWindow) {
    let (dirty, visible, warp, scale) = {
        let mut state = shared.lock();
        let dirty = std::mem::take(&mut state.cursor_dirty);
        (dirty, state.cursor_visible, state.warp.take(), state.scale)
    };
    if dirty {
        let result = if visible {
            CoreCursor::CreateCursor(CoreCursorType::Arrow, 0)
                .and_then(|cursor| window.SetPointerCursor(&cursor))
        } else {
            window.SetPointerCursor(None::<&CoreCursor>)
        };
        if let Err(error) = result {
            tracing::warn!("UWP: cursor: {error}");
        }
    }
    if let Some(position) = warp {
        if let Ok(bounds) = window.Bounds() {
            let _ = window.SetPointerPosition(Point {
                X: bounds.X + (position.x / scale) as f32,
                Y: bounds.Y + (position.y / scale) as f32,
            });
        }
    }
}

/// The loop's body, type-erased so the framework view can call it: a pointer to a closure on the
/// stack of the thread that called [`EventLoop::run`], which waits in `CoreApplication::Run`
/// until the view ends.
struct RunSlot(*mut (dyn FnMut() + 'static));

// SAFETY: the closure runs on the view thread while its owner thread is blocked, never on both.
unsafe impl Send for RunSlot {}

static RUN: Mutex<Option<RunSlot>> = Mutex::new(None);
/// A panic from the handler, carried back to the thread that called `run`.
static PANIC: Mutex<Option<Box<dyn Any + Send>>> = Mutex::new(None);

#[implement(IFrameworkViewSource)]
struct ViewSource;

impl IFrameworkViewSource_Impl for ViewSource_Impl {
    fn CreateView(&self) -> WinResult<IFrameworkView> {
        Ok(View.into())
    }
}

#[implement(IFrameworkView)]
struct View;

impl IFrameworkView_Impl for View_Impl {
    fn Initialize(&self, _view: Ref<'_, CoreApplicationView>) -> WinResult<()> {
        // On the Xbox: measure the window in real pixels, not 200 % view pixels. No effect on a
        // desktop.
        let _ = ApplicationViewScaling::TrySetDisableLayoutScaling(true);
        Ok(())
    }

    fn SetWindow(&self, window: Ref<'_, CoreWindow>) -> WinResult<()> {
        install(window.ok()?)
    }

    fn Load(&self, _entry_point: &HSTRING) -> WinResult<()> {
        Ok(())
    }

    fn Run(&self) -> WinResult<()> {
        if let Some(window) = CORE_WINDOW.with(|slot| slot.borrow().clone()) {
            window.Activate()?;
        }
        let body = RUN
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .map(|slot| slot.0);
        if let Some(body) = body {
            // SAFETY: see `RunSlot`; `EventLoop::run` keeps the closure alive until
            // `CoreApplication::Run` returns, which is after this.
            let body = unsafe { &mut *body };
            if let Err(payload) = panic::catch_unwind(AssertUnwindSafe(body)) {
                *PANIC.lock().unwrap_or_else(|p| p.into_inner()) = Some(payload);
            }
        }
        Ok(())
    }

    fn Uninitialize(&self) -> WinResult<()> {
        Ok(())
    }
}

pub struct EventLoop<T: 'static> {
    window_target: event_loop::ActiveEventLoop,
    user_events_sender: mpsc::Sender<T>,
    user_events_receiver: mpsc::Receiver<T>,
}

impl<T: 'static> EventLoop<T> {
    pub(crate) fn new(_: &PlatformSpecificEventLoopAttributes) -> Result<Self, EventLoopError> {
        SHARED.get_or_init(|| {
            Arc::new(Shared {
                state: Mutex::new(State::new()),
                dispatcher: Mutex::new(None),
                core_window: AtomicPtr::new(std::ptr::null_mut()),
                ui_thread: OnceLock::new(),
            })
        });
        let (user_events_sender, user_events_receiver) = mpsc::channel();
        Ok(Self {
            window_target: event_loop::ActiveEventLoop {
                p: ActiveEventLoop {
                    control_flow: Cell::new(ControlFlow::default()),
                    exit: Cell::new(false),
                },
                _marker: PhantomData,
            },
            user_events_sender,
            user_events_receiver,
        })
    }

    pub fn run<F>(mut self, mut event_handler: F) -> Result<(), EventLoopError>
    where
        F: FnMut(event::Event<T>, &event_loop::ActiveEventLoop),
    {
        let mut body = || self.run_view(&mut event_handler);
        let body: &mut (dyn FnMut() + '_) = &mut body;
        // SAFETY: only the lifetime is erased. The view calls the closure while this thread waits
        // in `CoreApplication::Run`, and the slot is emptied before this frame ends.
        let body: *mut (dyn FnMut() + 'static) = unsafe { std::mem::transmute(body) };
        *RUN.lock().unwrap_or_else(|p| p.into_inner()) = Some(RunSlot(body));
        // SAFETY: plain WinRT initialisation of this thread; an already initialised thread only
        // returns an error, which changes nothing.
        let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
        let source: IFrameworkViewSource = ViewSource.into();
        let result = CoreApplication::Run(&source);
        RUN.lock().unwrap_or_else(|p| p.into_inner()).take();
        if let Some(payload) = PANIC.lock().unwrap_or_else(|p| p.into_inner()).take() {
            panic::resume_unwind(payload);
        }
        result.map_err(|error| EventLoopError::Os(os_error!(OsError::new(error))))
    }

    /// The loop itself, on the view's UI thread.
    fn run_view<F>(&mut self, handler: &mut F)
    where
        F: FnMut(event::Event<T>, &event_loop::ActiveEventLoop),
    {
        let Some(shared) = shared().cloned() else {
            return;
        };
        let Some(window) = CORE_WINDOW.with(|slot| slot.borrow().clone()) else {
            return;
        };
        let Ok(dispatcher) = window.Dispatcher() else {
            return;
        };
        let target = &self.window_target;
        let window_id = RootWindowId(WindowId::core());
        let mut cause = StartCause::Init;
        loop {
            handler(event::Event::NewEvents(cause), target);
            if cause == StartCause::Init {
                handler(event::Event::Resumed, target);
            }
            let _ = dispatcher.ProcessEvents(CoreProcessEventsOption::ProcessAllIfPresent);
            {
                let mut state = shared.lock();
                state.flush_key();
                state.woken = false;
            }
            loop {
                let next = shared.lock().queue.pop_front();
                match next {
                    Some(Pending::Window(event)) => {
                        handler(event::Event::WindowEvent { window_id, event }, target)
                    }
                    Some(Pending::Device(event)) => handler(
                        event::Event::DeviceEvent {
                            device_id: event::DeviceId(DeviceId),
                            event,
                        },
                        target,
                    ),
                    None => break,
                }
            }
            while let Ok(event) = self.user_events_receiver.try_recv() {
                handler(event::Event::UserEvent(event), target);
            }
            apply_requests(&shared, &window);
            if std::mem::take(&mut shared.lock().redraw) {
                handler(
                    event::Event::WindowEvent {
                        window_id,
                        event: WindowEvent::RedrawRequested,
                    },
                    target,
                );
            }
            handler(event::Event::AboutToWait, target);
            apply_requests(&shared, &window);
            if target.p.exiting() {
                break;
            }
            cause = match target.p.control_flow() {
                ControlFlow::Poll => StartCause::Poll,
                ControlFlow::Wait => {
                    let start = Instant::now();
                    if !shared.has_work() {
                        // Returns after at least one event, a wake-up's `RunAsync` included.
                        let _ = dispatcher
                            .ProcessEvents(CoreProcessEventsOption::ProcessOneAndAllPending);
                    }
                    StartCause::WaitCancelled {
                        start,
                        requested_resume: None,
                    }
                }
                ControlFlow::WaitUntil(deadline) => {
                    // The dispatcher has no timed wait: poll it every millisecond until the
                    // deadline or an event.
                    let start = Instant::now();
                    loop {
                        if shared.has_work() {
                            break StartCause::WaitCancelled {
                                start,
                                requested_resume: Some(deadline),
                            };
                        }
                        let now = Instant::now();
                        if now >= deadline {
                            break StartCause::ResumeTimeReached {
                                start,
                                requested_resume: deadline,
                            };
                        }
                        thread::sleep((deadline - now).min(Duration::from_millis(1)));
                        let _ =
                            dispatcher.ProcessEvents(CoreProcessEventsOption::ProcessAllIfPresent);
                    }
                }
            };
        }
        handler(event::Event::LoopExiting, target);
    }

    pub fn window_target(&self) -> &event_loop::ActiveEventLoop {
        &self.window_target
    }

    pub fn create_proxy(&self) -> EventLoopProxy<T> {
        EventLoopProxy {
            user_events_sender: self.user_events_sender.clone(),
        }
    }
}

pub struct EventLoopProxy<T: 'static> {
    user_events_sender: mpsc::Sender<T>,
}

impl<T: 'static> EventLoopProxy<T> {
    pub fn send_event(&self, event: T) -> Result<(), event_loop::EventLoopClosed<T>> {
        self.user_events_sender
            .send(event)
            .map_err(|mpsc::SendError(event)| event_loop::EventLoopClosed(event))?;
        if let Some(shared) = shared() {
            shared.wake();
        }
        Ok(())
    }
}

impl<T: 'static> Clone for EventLoopProxy<T> {
    fn clone(&self) -> Self {
        Self {
            user_events_sender: self.user_events_sender.clone(),
        }
    }
}

impl<T> Unpin for EventLoopProxy<T> {}

pub struct ActiveEventLoop {
    control_flow: Cell<ControlFlow>,
    exit: Cell<bool>,
}

impl ActiveEventLoop {
    pub fn create_custom_cursor(&self, source: CustomCursorSource) -> RootCustomCursor {
        let _ = source.inner;
        RootCustomCursor {
            inner: super::PlatformCustomCursor,
        }
    }

    pub fn primary_monitor(&self) -> Option<MonitorHandle> {
        Some(MonitorHandle)
    }

    pub fn available_monitors(&self) -> VecDeque<MonitorHandle> {
        VecDeque::from([MonitorHandle])
    }

    #[inline]
    pub fn listen_device_events(&self, _allowed: DeviceEvents) {}

    #[cfg(feature = "rwh_05")]
    #[inline]
    pub fn raw_display_handle_rwh_05(&self) -> rwh_05::RawDisplayHandle {
        rwh_05::RawDisplayHandle::Windows(rwh_05::WindowsDisplayHandle::empty())
    }

    #[inline]
    pub fn system_theme(&self) -> Option<Theme> {
        None
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

    pub fn set_control_flow(&self, control_flow: ControlFlow) {
        self.control_flow.set(control_flow)
    }

    pub fn control_flow(&self) -> ControlFlow {
        self.control_flow.get()
    }

    pub(crate) fn exit(&self) {
        self.exit.set(true);
    }

    pub(crate) fn exiting(&self) -> bool {
        self.exit.get()
    }

    pub(crate) fn owned_display_handle(&self) -> OwnedDisplayHandle {
        OwnedDisplayHandle
    }
}

#[derive(Clone)]
pub(crate) struct OwnedDisplayHandle;

impl OwnedDisplayHandle {
    #[cfg(feature = "rwh_05")]
    #[inline]
    pub fn raw_display_handle_rwh_05(&self) -> rwh_05::RawDisplayHandle {
        rwh_05::WindowsDisplayHandle::empty().into()
    }

    #[cfg(feature = "rwh_06")]
    #[inline]
    pub fn raw_display_handle_rwh_06(
        &self,
    ) -> Result<rwh_06::RawDisplayHandle, rwh_06::HandleError> {
        Ok(rwh_06::WindowsDisplayHandle::new().into())
    }
}
