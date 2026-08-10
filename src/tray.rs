//! The tray icon and its right-click menu - the only visible presence of the
//! background daemon. No window is created; this is a bare winit event loop
//! (not eframe/egui) purely to pump the native messages tray-icon needs.

use crate::daemon::DaemonHandle;
use crate::log;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIconBuilder, TrayIconEvent};
use winit::application::ApplicationHandler;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};

pub fn run(handle: DaemonHandle) {
    let Ok(event_loop) = EventLoop::new() else {
        log::line("tray: couldn't create event loop - background sync will run without a tray icon");
        // Still worth keeping the process alive for the watcher/poll threads.
        loop {
            std::thread::sleep(std::time::Duration::from_secs(3600));
        }
    };
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App::new(handle);
    let _ = event_loop.run_app(&mut app);
}

struct App {
    handle: DaemonHandle,
    tray: Option<tray_icon::TrayIcon>,
    sync_now_id: String,
    pause_id: String,
    open_id: String,
    quit_id: String,
    pause_item: Option<MenuItem>,
}

impl App {
    fn new(handle: DaemonHandle) -> Self {
        Self {
            handle,
            tray: None,
            sync_now_id: String::new(),
            pause_id: String::new(),
            open_id: String::new(),
            quit_id: String::new(),
            pause_item: None,
        }
    }

    fn build_tray(&mut self) {
        let menu = Menu::new();
        let sync_now = MenuItem::new("Sync now", true, None);
        let pause = MenuItem::new(pause_label(self.handle.is_paused()), true, None);
        let open = MenuItem::new("Open Nimbus", true, None);
        let quit = MenuItem::new("Quit", true, None);

        let _ = menu.append(&sync_now);
        let _ = menu.append(&pause);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&open);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&quit);

        self.sync_now_id = sync_now.id().0.clone();
        self.pause_id = pause.id().0.clone();
        self.open_id = open.id().0.clone();
        self.quit_id = quit.id().0.clone();
        self.pause_item = Some(pause);

        let icon = build_icon();

        match TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Nimbus - background save sync")
            .with_icon(icon)
            .build()
        {
            Ok(tray) => self.tray = Some(tray),
            Err(e) => log::line(&format!("tray: couldn't create tray icon: {e}")),
        }
    }

    fn handle_menu_event(&self, id: &str) -> bool {
        if id == self.sync_now_id {
            log::line("tray: sync now clicked");
            let handle = self.handle.clone();
            std::thread::spawn(move || handle.sync_now());
        } else if id == self.pause_id {
            let now_paused = self.handle.toggle_pause();
            if let Some(item) = &self.pause_item {
                item.set_text(pause_label(now_paused));
            }
        } else if id == self.open_id {
            open_main_window();
        } else if id == self.quit_id {
            log::line("tray: quit clicked");
            return true;
        }
        false
    }
}

fn pause_label(paused: bool) -> &'static str {
    if paused { "Resume syncing" } else { "Pause syncing" }
}

fn open_main_window() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).spawn();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        if self.tray.is_none() {
            self.build_tray();
        }
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        _event: winit::event::WindowEvent,
    ) {
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Ok(event) = MenuEvent::receiver().try_recv() {
            if self.handle_menu_event(&event.id.0) {
                event_loop.exit();
                return;
            }
        }
        // Left/double clicks on the icon itself also open the main window.
        if let Ok(TrayIconEvent::Click { .. } | TrayIconEvent::DoubleClick { .. }) = TrayIconEvent::receiver().try_recv() {
            open_main_window();
        }
    }
}

/// A small solid cloud-blue circle - procedurally generated so there's no
/// external image asset to ship or embed.
fn build_icon() -> Icon {
    const SIZE: u32 = 32;
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    let center = SIZE as f32 / 2.0;
    let radius = center - 2.0;

    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 + 0.5 - center;
            let dy = y as f32 + 0.5 - center;
            let inside = (dx * dx + dy * dy).sqrt() <= radius;
            let idx = ((y * SIZE + x) * 4) as usize;
            if inside {
                rgba[idx] = 70;
                rgba[idx + 1] = 140;
                rgba[idx + 2] = 220;
                rgba[idx + 3] = 255;
            }
        }
    }

    Icon::from_rgba(rgba, SIZE, SIZE).expect("valid fixed-size RGBA buffer")
}
