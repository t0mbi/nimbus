use crate::config::{self, Config};
use crate::ludusavi_ctl::{self, GameStatus};
use crate::pathset;
use crate::ui;
use eframe::egui;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};

pub fn run() {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([560.0, 480.0]),
        ..Default::default()
    };
    let _ = eframe::run_native(
        "Nimbus",
        options,
        Box::new(|_cc| Ok(Box::new(NimbusApp::new()))),
    );
}

#[derive(PartialEq)]
enum Screen {
    Onboarding,
    Main,
}

#[derive(PartialEq)]
enum Tab {
    Settings,
    Games,
}

enum GamesState {
    Idle,
    Loading(Receiver<Result<Vec<GameStatus>, String>>),
    Loaded(Vec<GameStatus>),
    Failed(String),
}

enum ActionState {
    Idle,
    Running { game: String, kind: &'static str, rx: Receiver<Result<(), String>> },
    Done { message: String, ok: bool },
}

struct NimbusApp {
    config: Config,
    screen: Screen,
    tab: Tab,
    ludusavi_status: LudusaviStatus,

    sync_path_text: String,
    format_zip: bool,
    full_limit_text: String,
    settings_saved_at: Option<std::time::Instant>,

    games: GamesState,
    action: ActionState,
    game_filter: String,

    path_button_result: Option<Result<(), String>>,
}

enum LudusaviStatus {
    Missing(PathBuf),
    Found(String),
}

impl NimbusApp {
    fn new() -> Self {
        let mut config = Config::load().unwrap_or_default();
        config.inherit_from_ludusavi_if_unset();
        config.skip_onboarding_if_already_set_up();

        let bin = config.ludusavi_bin();
        let ludusavi_status = match config::probe_ludusavi(&bin) {
            Some(v) => LudusaviStatus::Found(v),
            None => LudusaviStatus::Missing(bin),
        };

        let sync_path_text = config
            .sync_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let format_zip = config.format() == "zip";
        let full_limit_text = config.full_limit().to_string();
        let screen = if config.onboarded { Screen::Main } else { Screen::Onboarding };

        Self {
            config,
            screen,
            tab: Tab::Settings,
            ludusavi_status,
            sync_path_text,
            format_zip,
            full_limit_text,
            settings_saved_at: None,
            games: GamesState::Idle,
            action: ActionState::Idle,
            game_filter: String::new(),
            path_button_result: None,
        }
    }

    fn save_settings(&mut self) {
        self.config.sync_path =
            (!self.sync_path_text.trim().is_empty()).then(|| PathBuf::from(self.sync_path_text.trim()));
        self.config.format = Some(if self.format_zip { "zip" } else { "simple" }.to_string());
        self.config.full_limit = self.full_limit_text.trim().parse().ok();
        let _ = self.config.save();
        self.settings_saved_at = Some(std::time::Instant::now());
    }

    fn start_loading_games(&mut self) {
        let bin = self.config.ludusavi_bin();
        let (tx, rx): (Sender<Result<Vec<GameStatus>, String>>, _) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(ludusavi_ctl::list_games(&bin));
        });
        self.games = GamesState::Loading(rx);
    }

    fn start_action(&mut self, game: String, kind: &'static str) {
        let bin = self.config.ludusavi_bin();
        let sync_path = match &self.config.sync_path {
            Some(p) => p.clone(),
            None => {
                self.action = ActionState::Done {
                    message: "Set a sync folder first.".into(),
                    ok: false,
                };
                return;
            }
        };
        let format = self.config.format().to_string();
        let limit = self.config.full_limit();

        let (tx, rx): (Sender<Result<(), String>>, _) = std::sync::mpsc::channel();
        let game_for_thread = game.clone();
        std::thread::spawn(move || {
            let result = if kind == "Push" {
                ludusavi_ctl::backup(&bin, &sync_path, &format, limit, &game_for_thread)
            } else {
                ludusavi_ctl::restore(&bin, &sync_path, &game_for_thread)
            };
            let _ = tx.send(result);
        });

        self.action = ActionState::Running { game, kind, rx };
    }

    fn poll_background_work(&mut self, ctx: &egui::Context) {
        if let GamesState::Loading(rx) = &self.games {
            if let Ok(result) = rx.try_recv() {
                self.games = match result {
                    Ok(games) => GamesState::Loaded(games),
                    Err(e) => GamesState::Failed(e),
                };
            } else {
                ctx.request_repaint();
            }
        }

        if let ActionState::Running { game, kind, rx } = &self.action {
            if let Ok(result) = rx.try_recv() {
                let message = match &result {
                    Ok(()) => format!("{kind} succeeded for {game}."),
                    Err(e) => format!("{kind} failed for {game}: {e}"),
                };
                self.action = ActionState::Done { message, ok: result.is_ok() };
            } else {
                ctx.request_repaint();
            }
        }
    }
}

impl eframe::App for NimbusApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_background_work(ctx);

        match self.screen {
            Screen::Onboarding => {
                egui::CentralPanel::default().show(ctx, |ui| self.onboarding_screen(ui));
            }
            Screen::Main => {
                egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.tab, Tab::Settings, "Settings");
                        ui.selectable_value(&mut self.tab, Tab::Games, "Games");
                    });
                });

                egui::CentralPanel::default().show(ctx, |ui| match self.tab {
                    Tab::Settings => self.settings_tab(ui),
                    Tab::Games => self.games_tab(ui),
                });
            }
        }
    }
}

impl NimbusApp {
    fn onboarding_screen(&mut self, ui: &mut egui::Ui) {
        ui.heading("Welcome to Nimbus");
        ui.add_space(4.0);
        ui.label(
            "Nimbus keeps your game saves in sync across your PCs, using a folder you \
             control - a NAS share, an external drive - instead of Steam Cloud or a \
             subscription.",
        );
        ui.label(
            "When a game launches through Nimbus, it pulls your latest save first. When \
             you quit, it pushes anything that changed back. That's the whole thing - no \
             background service, nothing running between sessions.",
        );
        ui.add_space(12.0);
        ui.separator();

        ui.add_space(8.0);
        ui.strong("1. Where should saves sync to?");
        ui.label("A shared network location, mounted as a normal path (e.g. a NAS share).");
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.sync_path_text);
            if ui.button("Browse…").clicked() {
                if let Some(picked) = rfd::FileDialog::new().pick_folder() {
                    self.sync_path_text = picked.display().to_string();
                }
            }
        });
        if !self.sync_path_text.trim().is_empty() && ui::looks_local(&self.sync_path_text) {
            ui.colored_label(
                egui::Color32::from_rgb(200, 150, 50),
                "That looks like a folder on this PC, not a network share - saves won't \
                 reach your other machines until this points at a shared folder.",
            );
        }

        ui.add_space(12.0);
        ui.strong("2. Add Nimbus to PATH (optional)");
        ui.label("Lets Launch Options just say \"nimbus %command%\" instead of a full path.");
        if let Some(dir) = config::install_dir() {
            if ui.button("Add Nimbus to PATH").clicked() {
                self.path_button_result = Some(pathset::add_to_user_path(&dir));
            }
            match &self.path_button_result {
                Some(Ok(())) => {
                    ui.colored_label(
                        egui::Color32::from_rgb(70, 160, 70),
                        "Added. Restart Steam (and any open terminals) for it to take effect.",
                    );
                }
                Some(Err(e)) => {
                    ui.colored_label(egui::Color32::from_rgb(200, 80, 80), format!("Couldn't add to PATH: {e}"));
                }
                None => {}
            }
        }

        ui.add_space(12.0);
        ui.strong("3. Set the Launch Options for each game");
        ui.label("In Steam: right-click a game → Properties → General → Launch Options, and paste:");
        let launch_options = config::launch_options_string();
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut launch_options.clone()).desired_width(380.0));
            if ui.button("Copy").clicked() {
                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    let _ = clipboard.set_text(launch_options.clone());
                }
            }
        });
        ui.label(
            "Repeat this on each PC, all pointed at the same shared folder. You can always \
             come back to these settings, and manage individual games, from the Settings \
             and Games tabs.",
        );

        ui.add_space(16.0);
        if ui.button("Get started").clicked() {
            self.save_settings();
            self.config.onboarded = true;
            let _ = self.config.save();
            self.screen = Screen::Main;
        }
    }

    fn settings_tab(&mut self, ui: &mut egui::Ui) {
        match &self.ludusavi_status {
            LudusaviStatus::Found(version) => {
                ui.colored_label(egui::Color32::from_rgb(70, 160, 70), format!("Ludusavi {version} found"));
            }
            LudusaviStatus::Missing(looked_at) => {
                ui.colored_label(
                    egui::Color32::from_rgb(200, 80, 80),
                    format!(
                        "Ludusavi not found (looked for {}). Nimbus can't sync anything until \
                         it's installed, or ludusavi.exe is placed next to nimbus.exe.",
                        looked_at.display()
                    ),
                );
                if ui.button("Open download page").clicked() {
                    let _ = ui::open(ui::LUDUSAVI_URL);
                }
            }
        }

        ui.add_space(12.0);
        ui.heading("Sync folder");
        ui.label("A shared network location, mounted as a normal path (e.g. a NAS share).");
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.sync_path_text);
            if ui.button("Browse…").clicked() {
                if let Some(picked) = rfd::FileDialog::new().pick_folder() {
                    self.sync_path_text = picked.display().to_string();
                }
            }
        });
        if !self.sync_path_text.trim().is_empty() && ui::looks_local(&self.sync_path_text) {
            ui.colored_label(
                egui::Color32::from_rgb(200, 150, 50),
                "That looks like a folder on this PC, not a network share - saves won't reach \
                 your other machines until this points at a shared folder.",
            );
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label("Format:");
            ui.selectable_value(&mut self.format_zip, true, "zip (recommended)");
            ui.selectable_value(&mut self.format_zip, false, "simple");
        });
        if !self.format_zip {
            ui.colored_label(
                egui::Color32::from_rgb(200, 150, 50),
                "\"simple\" format overwrites saves in place with no history. zip keeps \
                 versions, so a bad sync is recoverable.",
            );
        }

        ui.horizontal(|ui| {
            ui.label("Versions to keep per game:");
            ui.add(egui::TextEdit::singleline(&mut self.full_limit_text).desired_width(40.0));
        });

        ui.add_space(8.0);
        if ui.button("Save settings").clicked() {
            self.save_settings();
        }
        if let Some(at) = self.settings_saved_at {
            if at.elapsed().as_secs() < 3 {
                ui.colored_label(egui::Color32::from_rgb(70, 160, 70), "Saved.");
            }
        }

        ui.separator();
        ui.heading("Steam Launch Options");
        let launch_options = config::launch_options_string();
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut launch_options.clone()).desired_width(400.0));
            if ui.button("Copy").clicked() {
                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    let _ = clipboard.set_text(launch_options.clone());
                }
            }
        });
        ui.label("Paste into a game's Steam Properties → Launch Options.");

        ui.add_space(4.0);
        if let Some(dir) = config::install_dir() {
            if ui.button("Add Nimbus to PATH").clicked() {
                self.path_button_result = Some(pathset::add_to_user_path(&dir));
            }
            match &self.path_button_result {
                Some(Ok(())) => {
                    ui.colored_label(
                        egui::Color32::from_rgb(70, 160, 70),
                        "Added. Restart Steam (and any open terminals) for it to take effect, \
                         then Launch Options can just say: nimbus %command%",
                    );
                }
                Some(Err(e)) => {
                    ui.colored_label(egui::Color32::from_rgb(200, 80, 80), format!("Couldn't add to PATH: {e}"));
                }
                None => {}
            }
        }

        ui.add_space(12.0);
        ui.separator();
        if ui.button("Show welcome screen again").clicked() {
            self.screen = Screen::Onboarding;
        }
    }

    fn games_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("Refresh").clicked() {
                self.start_loading_games();
            }
            ui.add(egui::TextEdit::singleline(&mut self.game_filter).hint_text("Filter…"));
        });

        match &self.action {
            ActionState::Running { game, kind, .. } => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(format!("{kind} in progress for {game}…"));
                });
            }
            ActionState::Done { message, ok } => {
                let color = if *ok {
                    egui::Color32::from_rgb(70, 160, 70)
                } else {
                    egui::Color32::from_rgb(200, 80, 80)
                };
                ui.colored_label(color, message);
            }
            ActionState::Idle => {}
        }

        ui.separator();

        match &self.games {
            GamesState::Idle => {
                ui.label("Press Refresh to scan for games with local save data.");
            }
            GamesState::Loading(_) => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Scanning your library - this can take a while for a large collection…");
                });
            }
            GamesState::Failed(e) => {
                ui.colored_label(egui::Color32::from_rgb(200, 80, 80), format!("Couldn't list games: {e}"));
            }
            GamesState::Loaded(games) => {
                let filter = self.game_filter.to_lowercase();
                let busy = matches!(self.action, ActionState::Running { .. });
                let visible: Vec<(String, u64)> = games
                    .iter()
                    .filter(|g| filter.is_empty() || g.name.to_lowercase().contains(&filter))
                    .map(|g| (g.name.clone(), g.bytes))
                    .collect();

                let mut requested: Option<(String, &'static str)> = None;

                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (name, bytes) in &visible {
                        ui.horizontal(|ui| {
                            ui.label(name);
                            ui.label(format_bytes(*bytes));
                            ui.add_enabled_ui(!busy, |ui| {
                                if ui.button("Push ↑").on_hover_text("Back up to sync folder").clicked() {
                                    requested = Some((name.clone(), "Push"));
                                }
                                if ui.button("Pull ↓").on_hover_text("Restore from sync folder").clicked() {
                                    requested = Some((name.clone(), "Pull"));
                                }
                            });
                        });
                    }
                });

                if let Some((name, kind)) = requested {
                    self.start_action(name, kind);
                }
            }
        }
    }
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}
