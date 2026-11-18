//! A game started without its prepared files (the game build run by hand, not
//! through a launcher): a window that says where to get them
//! (`horizon_client::no_resources`) and quits when it is closed. No loader, no
//! crash report: nothing went wrong in the game.
use super::*;
use horizon_client::no_resources::{Action, NoResources};
use winit::event::MouseButton;

struct Missing {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    gui: UiResources,
    screen: NoResources,
    /// The pointer in physical pixels.
    pointer: (f64, f64),
    api: horizon_client::graphics::GraphicsApi,
    error: Option<Box<dyn std::error::Error>>,
    /// UWP: `resources.zip` being unpacked on a thread of its own.
    #[cfg(target_vendor = "uwp")]
    unpack: Option<Unpack>,
}

/// The UWP build's unpacking of `resources.zip` (`horizon_client::resources_zip`).
#[cfg(target_vendor = "uwp")]
struct Unpack {
    progress: Arc<horizon_client::resources_zip::Progress>,
    result: Arc<std::sync::Mutex<Option<std::result::Result<(), String>>>>,
}

/// Shows the screen until the player closes it.
pub(super) fn show(looked_in: &std::path::Path, settings: &Settings) -> Result<()> {
    // The player's language, from the strings compiled in: there are no prepared ones.
    horizon_client::lang::set(horizon_client::lang::Language::load(
        &[],
        &settings.language,
    ));
    let mut app = Missing {
        window: None,
        renderer: None,
        gui: UiResources {
            atlas: HudAtlas::build(looked_in)?,
            panorama: Arc::new(horizon_client::gfx::panorama::Faces::colour([0; 4])),
        },
        screen: NoResources::new(looked_in, horizon_hooks::engine::get_the_game()),
        pointer: (0.0, 0.0),
        api: settings.horizon.api,
        error: None,
        #[cfg(target_vendor = "uwp")]
        unpack: None,
    };
    #[cfg(target_vendor = "uwp")]
    {
        app.unpack = start_unpack(&mut app.screen);
    }
    horizon_client::hud::GUI_SCALE.store(settings.gui_scale, Ordering::Relaxed);
    EventLoop::new()?.run_app(&mut app)?;
    app.error.map_or(Ok(()), Err)
}

/// UWP (the Xbox): the prepared files come as `resources.zip` in the app's LocalState
/// (docs/xbox-uwp.md). With one there, unpack it now that a window shows how far it is;
/// without, say how to upload it.
#[cfg(target_vendor = "uwp")]
fn start_unpack(screen: &mut NoResources) -> Option<Unpack> {
    use horizon_client::resources_zip;
    let local = horizon_threads::uwp::local_folder()?;
    let zip = local.join(resources_zip::ZIP_NAME);
    if !zip.is_file() {
        screen.say(
            horizon_client::lang::get("horizon.no_resources.title"),
            vec![horizon_client::lang::get("horizon.no_resources.upload")],
        );
        return None;
    }
    let unpack = Unpack {
        progress: Arc::default(),
        result: Arc::default(),
    };
    let (progress, result) = (unpack.progress.clone(), unpack.result.clone());
    std::thread::Builder::new()
        .name("Resources unpacker".into())
        .spawn(move || {
            let unpacked =
                resources_zip::unpack(&zip, &local.join(resources_zip::FOLDER_NAME), &progress);
            match &unpacked {
                // The files are out: the zip only takes room now.
                Ok(()) => {
                    let _ = std::fs::remove_file(&zip);
                    log::info!("Horizon: unpacked {}", zip.display());
                }
                Err(e) => log::error!("Horizon: {e}"),
            }
            *result.lock().unwrap_or_else(|p| p.into_inner()) = Some(unpacked);
        })
        .ok()?;
    show_unpacking(screen, &unpack);
    Some(unpack)
}

/// The screen's text for where the unpacking is.
#[cfg(target_vendor = "uwp")]
fn show_unpacking(screen: &mut NoResources, unpack: &Unpack) {
    let (done, total) = unpack.progress.get();
    screen.say(
        horizon_client::lang::get("horizon.no_resources.unpacking"),
        vec![horizon_client::lang::tr(
            "horizon.no_resources.unpacking.progress",
            &[&done, &total],
        )],
    );
}

impl Missing {
    /// UWP: the unpacking's progress on screen, then its outcome.
    #[cfg(target_vendor = "uwp")]
    fn follow_unpack(&mut self) {
        let Some(unpack) = &self.unpack else {
            return;
        };
        let finished = unpack
            .result
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take();
        match finished {
            None => show_unpacking(&mut self.screen, unpack),
            Some(Ok(())) => {
                self.screen.say(
                    horizon_client::lang::get("horizon.no_resources.unpacked"),
                    vec![horizon_client::lang::get(
                        "horizon.no_resources.unpacked.restart",
                    )],
                );
                self.unpack = None;
            }
            Some(Err(e)) => {
                self.screen.say(
                    horizon_client::lang::get("horizon.no_resources.title"),
                    vec![horizon_client::lang::tr(
                        "horizon.no_resources.unpack_failed",
                        &[&e],
                    )],
                );
                self.unpack = None;
            }
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn size(&self) -> [u32; 2] {
        self.renderer
            .as_ref()
            .map_or([1280, 720], |r| [r.config.width, r.config.height])
    }

    /// `f` over a Ui of the window's size, the cursor where the pointer is.
    fn with_ui<R>(
        &self,
        batch: &mut HudBatch,
        f: impl FnOnce(&mut horizon_client::ui_theme::Ui) -> R,
    ) -> R {
        let cursor = (
            self.pointer.0 as f32 / batch.scale,
            self.pointer.1 as f32 / batch.scale,
        );
        let mut ui = horizon_client::ui_theme::Ui::new(&self.gui.atlas, batch, cursor);
        let result = f(&mut ui);
        ui.finish();
        result
    }

    fn draw(&mut self) {
        let mut batch = HudBatch::new(self.size());
        self.with_ui(&mut batch, |ui| self.screen.draw(ui));
        let Some(renderer) = &mut self.renderer else {
            return;
        };
        match renderer.render(&loading_frame(&batch)) {
            Ok(true) => return,
            Ok(false) => {}
            Err(e) => log::warn!("Horizon: frame failed: {e}"),
        }
        // The frame was not shown (the first one often isn't): the glyphs it
        // rasterized never reached the atlas, so draw them again next frame.
        horizon_client::ui_theme::reset_atlas();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn click(&mut self, event_loop: &ActiveEventLoop) {
        let mut batch = HudBatch::new(self.size());
        match self.with_ui(&mut batch, |ui| self.screen.click(ui)) {
            Some(Action::Open(address)) => packs::open_uri(address),
            Some(Action::Quit) => event_loop.exit(),
            None => {}
        }
    }
}

impl ApplicationHandler for Missing {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes =
            horizon_client::window::with_icon(Window::default_attributes().with_title("Horizon"))
                .with_inner_size(winit::dpi::LogicalSize::new(1040.0, 700.0));
        let made = event_loop
            .create_window(attributes)
            .map_err(Into::into)
            .and_then(|window| {
                let window = Arc::new(window);
                let renderer = loading_renderer(
                    window.clone(),
                    false,
                    Some(&self.gui.atlas),
                    Some(&self.gui.panorama),
                    None,
                    self.api,
                    false,
                )?;
                Ok((window, renderer))
            });
        match made {
            Ok((window, renderer)) => {
                window.request_redraw();
                self.window = Some(window);
                self.renderer = Some(renderer);
            }
            Err(e) => {
                self.error = Some(e);
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(window) = self.window.clone().filter(|w| w.id() == id) else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(r) = &mut self.renderer {
                    r.resize(size.width, size.height);
                }
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => self.draw(),
            WindowEvent::CursorMoved { position, .. } => {
                self.pointer = (position.x, position.y);
                window.request_redraw();
            }
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            } => self.click(event_loop),
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && event.physical_key == PhysicalKey::Code(KeyCode::Escape) =>
            {
                event_loop.exit()
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if horizon_threads::stop_requests() > 0 {
            event_loop.exit();
            return;
        }
        #[cfg(target_vendor = "uwp")]
        self.follow_unpack();
        // Wake now and then for Ctrl+C in the console; redraws come with events.
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(250),
        ));
    }
}
