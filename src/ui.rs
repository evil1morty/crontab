use crate::autostart;
use crate::config::{self, Job};
use crate::cron_parse;
use crate::scheduler::{self, SharedState};
use crate::theme;
use chrono::Local;
use eframe::egui::{self, Color32, RichText, Rounding, Stroke};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(PartialEq, Eq)]
enum Tab {
    Jobs,
    Logs,
    Settings,
}

pub struct App {
    state: Arc<SharedState>,
    show_window: Arc<AtomicBool>,
    last_visibility_sent: Option<bool>,
    quit_flag: Arc<AtomicBool>,
    // Kept alive so the TrayIcon's Drop doesn't fire and remove the icon.
    _tray: Option<crate::tray::TrayHandles>,
    theme_applied: bool,

    tab: Tab,

    // Edit form
    edit_index: Option<usize>, // None = adding new
    form_open: bool,
    form_name: String,
    form_schedule: String,
    form_command: String,
    form_error: Option<String>,

    autostart_on: bool,
    status: String,
}

impl App {
    pub fn new(
        state: Arc<SharedState>,
        show_window: Arc<AtomicBool>,
        quit_flag: Arc<AtomicBool>,
        tray: Option<crate::tray::TrayHandles>,
        _start_hidden: bool,
    ) -> Self {
        Self {
            state,
            show_window,
            last_visibility_sent: None,
            quit_flag,
            _tray: tray,
            theme_applied: false,
            tab: Tab::Jobs,
            edit_index: None,
            form_open: false,
            form_name: String::new(),
            form_schedule: "*/5 * * * *".into(),
            form_command: String::new(),
            form_error: None,
            autostart_on: autostart::is_enabled(),
            status: String::new(),
        }
    }

    fn save_form(&mut self) {
        if self.form_name.trim().is_empty() {
            self.form_error = Some("name is required".into());
            return;
        }
        if let Err(e) = cron_parse::validate(&self.form_schedule) {
            self.form_error = Some(format!("invalid schedule — {e}"));
            return;
        }
        if self.form_command.trim().is_empty() {
            self.form_error = Some("command is required".into());
            return;
        }
        let job = Job {
            name: self.form_name.trim().to_string(),
            schedule: self.form_schedule.trim().to_string(),
            command: self.form_command.trim().to_string(),
            enabled: true,
        };
        let mut cfg = self.state.config.lock().unwrap();
        match self.edit_index {
            Some(i) if i < cfg.jobs.len() => {
                let prev_enabled = cfg.jobs[i].enabled;
                cfg.jobs[i] = Job { enabled: prev_enabled, ..job };
            }
            _ => cfg.jobs.push(job),
        }
        let cfg_clone = cfg.clone();
        drop(cfg);
        if let Err(e) = config::save(&cfg_clone) {
            self.status = format!("save failed — {e}");
        } else {
            self.status = "saved".into();
            *self.state.config_mtime.lock().unwrap() = config::mtime(&config::config_path());
            self.reset_form();
        }
    }

    fn reset_form(&mut self) {
        self.edit_index = None;
        self.form_open = false;
        self.form_name.clear();
        self.form_schedule = "*/5 * * * *".into();
        self.form_command.clear();
        self.form_error = None;
    }

    fn open_form_new(&mut self) {
        self.reset_form();
        self.form_open = true;
    }

    fn open_form_edit(&mut self, i: usize, j: &Job) {
        self.edit_index = Some(i);
        self.form_open = true;
        self.form_name = j.name.clone();
        self.form_schedule = j.schedule.clone();
        self.form_command = j.command.clone();
        self.form_error = None;
    }

    fn persist(&mut self) {
        let cfg = self.state.config.lock().unwrap().clone();
        if let Err(e) = config::save(&cfg) {
            self.status = format!("save failed — {e}");
        } else {
            *self.state.config_mtime.lock().unwrap() = config::mtime(&config::config_path());
        }
    }

    fn open_logs_folder(&self) {
        let dir = {
            let cfg = self.state.config.lock().unwrap();
            if cfg.log_dir.is_empty() {
                config::default_log_dir()
            } else {
                std::path::PathBuf::from(&cfg.log_dir)
            }
        };
        std::fs::create_dir_all(&dir).ok();
        #[cfg(windows)]
        {
            let _ = std::process::Command::new("explorer").arg(&dir).spawn();
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.theme_applied {
            theme::apply(ctx);
            self.theme_applied = true;
        }

        if ctx.input(|i| i.viewport().close_requested()) {
            self.show_window.store(false, Ordering::SeqCst);
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            self.last_visibility_sent = Some(false);
        }

        let want_visible = self.show_window.load(Ordering::SeqCst);
        if self.last_visibility_sent != Some(want_visible) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(want_visible));
            if want_visible {
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            self.last_visibility_sent = Some(want_visible);
        }

        if self.quit_flag.load(Ordering::SeqCst) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            std::process::exit(0);
        }

        // While hidden, draw nothing but keep a slow 1Hz pulse so tray
        // commands (Show / Quit) get picked up. ctx.request_repaint() from
        // the MenuEvent handler is unreliable once winit has marked the
        // viewport invisible, so we poll the flag instead. 1Hz is effectively
        // free CPU-wise compared to the old 4Hz heartbeat.
        if !want_visible {
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
            return;
        }

        // While visible, refresh the live "next run in Xm" labels every 30s.
        // User interaction wakes egui naturally, so this is the only timer.
        ctx.request_repaint_after(std::time::Duration::from_secs(30));

        // Top header
        egui::TopBottomPanel::top("header")
            .exact_height(56.0)
            .frame(egui::Frame::none().fill(Color32::from_rgb(28, 31, 38)).inner_margin(egui::Margin::symmetric(16.0, 10.0)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new("⏱  Crontab").size(18.0).strong());
                    ui.add_space(20.0);

                    tab_button(ui, &mut self.tab, Tab::Jobs, "Jobs");
                    tab_button(ui, &mut self.tab, Tab::Logs, "Logs");
                    tab_button(ui, &mut self.tab, Tab::Settings, "Settings");

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Master toggle pill
                        let mut master = self.state.config.lock().unwrap().master_enabled;
                        let label = if master { "● running" } else { "⏸ paused" };
                        let color = if master { theme::SUCCESS } else { theme::MUTED };
                        if ui
                            .add(
                                egui::Button::new(RichText::new(label).color(color).size(12.0))
                                    .fill(Color32::from_rgb(34, 38, 46))
                                    .rounding(Rounding::same(14.0)),
                            )
                            .clicked()
                        {
                            master = !master;
                            self.state.config.lock().unwrap().master_enabled = master;
                            self.persist();
                        }
                    });
                });
            });

        // Status footer
        if !self.status.is_empty() {
            egui::TopBottomPanel::bottom("status")
                .exact_height(28.0)
                .frame(egui::Frame::none().fill(Color32::from_rgb(28, 31, 38)).inner_margin(egui::Margin::symmetric(16.0, 6.0)))
                .show(ctx, |ui| {
                    ui.label(RichText::new(&self.status).color(theme::MUTED).size(12.0));
                });
        }

        // Body
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::from_rgb(20, 22, 28)).inner_margin(egui::Margin::same(16.0)))
            .show(ctx, |ui| match self.tab {
                Tab::Jobs => self.render_jobs(ui),
                Tab::Logs => self.render_logs(ui),
                Tab::Settings => self.render_settings(ui),
            });
    }
}

fn tab_button(ui: &mut egui::Ui, current: &mut Tab, tab: Tab, label: &str) {
    let active = *current == tab;
    let color = if active { theme::ACCENT } else { theme::MUTED };
    let rt = RichText::new(label).color(color).size(13.5);
    let btn = egui::Button::new(rt)
        .fill(Color32::TRANSPARENT)
        .stroke(if active {
            Stroke::new(1.0, theme::ACCENT)
        } else {
            Stroke::NONE
        })
        .rounding(Rounding::same(6.0));
    if ui.add(btn).clicked() {
        *current = tab;
    }
}

impl App {
    fn render_jobs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Jobs").heading());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(
                        egui::Button::new(RichText::new("+ New job").color(Color32::WHITE))
                            .fill(theme::ACCENT)
                            .rounding(Rounding::same(8.0)),
                    )
                    .clicked()
                {
                    self.open_form_new();
                }
            });
        });

        ui.add_space(4.0);

        let now = Local::now();
        let jobs_snapshot = self.state.config.lock().unwrap().jobs.clone();

        let mut to_delete: Option<usize> = None;
        let mut to_edit: Option<usize> = None;
        let mut to_test: Option<usize> = None;
        let mut changed = false;

        if jobs_snapshot.is_empty() && !self.form_open {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("No jobs yet").size(15.0).color(theme::MUTED));
                ui.add_space(6.0);
                ui.label(RichText::new("Click \"+ New job\" to add one.").size(12.0).color(theme::MUTED));
            });
        }

        egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            for (i, job) in jobs_snapshot.iter().enumerate() {
                job_card(ui, i, job, now, &mut |action| match action {
                    JobAction::ToggleEnabled(v) => {
                        self.state.config.lock().unwrap().jobs[i].enabled = v;
                        changed = true;
                    }
                    JobAction::Edit => to_edit = Some(i),
                    JobAction::Delete => to_delete = Some(i),
                    JobAction::RunNow => to_test = Some(i),
                });
                ui.add_space(8.0);
            }

            // Inline form (after the list, if open)
            if self.form_open {
                ui.add_space(4.0);
                self.render_form(ui, now);
            }
        });

        if let Some(i) = to_delete {
            self.state.config.lock().unwrap().jobs.remove(i);
            changed = true;
            self.status = "job deleted".into();
        }
        if let Some(i) = to_edit {
            let j = jobs_snapshot[i].clone();
            self.open_form_edit(i, &j);
        }
        if let Some(i) = to_test {
            let j = jobs_snapshot[i].clone();
            let log_dir = {
                let cfg = self.state.config.lock().unwrap();
                if cfg.log_dir.is_empty() {
                    config::default_log_dir()
                } else {
                    std::path::PathBuf::from(&cfg.log_dir)
                }
            };
            std::fs::create_dir_all(&log_dir).ok();
            scheduler::fire(&j, &log_dir, self.state.clone());
            self.status = format!("ran '{}' now", j.name);
        }
        if changed {
            self.persist();
        }
    }

    fn render_form(&mut self, ui: &mut egui::Ui, now: chrono::DateTime<Local>) {
        egui::Frame::none()
            .fill(theme::CARD_BG)
            .stroke(Stroke::new(1.0, theme::ACCENT))
            .rounding(Rounding::same(10.0))
            .inner_margin(egui::Margin::same(14.0))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(if self.edit_index.is_some() { "Edit job" } else { "New job" })
                        .strong()
                        .size(14.0),
                );
                ui.add_space(6.0);

                ui.label(RichText::new("Name").size(12.0).color(theme::MUTED));
                ui.add(egui::TextEdit::singleline(&mut self.form_name).desired_width(f32::INFINITY));

                ui.add_space(6.0);
                ui.label(RichText::new("Schedule").size(12.0).color(theme::MUTED));
                ui.add(
                    egui::TextEdit::singleline(&mut self.form_schedule)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace),
                );

                // Live next-run preview
                match cron_parse::validate(&self.form_schedule) {
                    Ok(()) => match cron_parse::next_run(&self.form_schedule, now) {
                        Some(n) => {
                            ui.label(
                                RichText::new(format!(
                                    "next: {} ({})",
                                    n.format("%a %Y-%m-%d %H:%M"),
                                    cron_parse::human_until(n, now)
                                ))
                                .size(12.0)
                                .color(theme::SUCCESS),
                            );
                        }
                        None => {
                            ui.label(RichText::new("no upcoming runs").size(12.0).color(theme::MUTED));
                        }
                    },
                    Err(e) => {
                        ui.label(RichText::new(format!("invalid — {e}")).size(12.0).color(theme::DANGER));
                    }
                }

                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Presets:").size(11.5).color(theme::MUTED));
                    for (label, expr) in cron_parse::PRESETS {
                        let btn = egui::Button::new(RichText::new(*label).size(11.5))
                            .fill(Color32::from_rgb(42, 46, 56))
                            .rounding(Rounding::same(12.0))
                            .stroke(Stroke::NONE);
                        if ui.add(btn).clicked() {
                            self.form_schedule = (*expr).into();
                        }
                    }
                });

                ui.add_space(6.0);
                ui.label(RichText::new("Command").size(12.0).color(theme::MUTED));
                ui.add(
                    egui::TextEdit::multiline(&mut self.form_command)
                        .desired_width(f32::INFINITY)
                        .desired_rows(2)
                        .font(egui::TextStyle::Monospace),
                );

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let primary = egui::Button::new(
                        RichText::new(if self.edit_index.is_some() { "Update" } else { "Add" })
                            .color(Color32::WHITE),
                    )
                    .fill(theme::ACCENT)
                    .rounding(Rounding::same(8.0));
                    if ui.add(primary).clicked() {
                        self.save_form();
                    }
                    if ui.button("Cancel").clicked() {
                        self.reset_form();
                    }
                    if let Some(err) = &self.form_error {
                        ui.label(RichText::new(err).color(theme::DANGER).size(12.0));
                    }
                });
            });
    }

    fn render_logs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Recent runs").heading());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Open logs folder").clicked() {
                    self.open_logs_folder();
                }
            });
        });
        ui.add_space(4.0);

        let runs = self.state.last_runs.lock().unwrap().clone();
        if runs.is_empty() {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("No runs yet").size(14.0).color(theme::MUTED));
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Jobs will appear here after they fire.")
                        .size(12.0)
                        .color(theme::MUTED),
                );
            });
            return;
        }
        egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            for (name, when, code) in runs.iter().rev() {
                let ok = *code == 0;
                let dot_color = if ok { theme::SUCCESS } else { theme::DANGER };
                egui::Frame::none()
                    .fill(theme::CARD_BG)
                    .rounding(Rounding::same(8.0))
                    .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("●").color(dot_color));
                            ui.label(RichText::new(name).strong());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(
                                    RichText::new(format!("exit {code}"))
                                        .color(if ok { theme::MUTED } else { theme::DANGER })
                                        .size(11.5),
                                );
                                ui.label(
                                    RichText::new(when.format("%Y-%m-%d %H:%M:%S").to_string())
                                        .color(theme::MUTED)
                                        .size(11.5),
                                );
                            });
                        });
                    });
                ui.add_space(6.0);
            }
        });
    }

    fn render_settings(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Settings").heading());
        ui.add_space(8.0);

        // Autostart card
        egui::Frame::none()
            .fill(theme::CARD_BG)
            .rounding(Rounding::same(10.0))
            .inner_margin(egui::Margin::same(14.0))
            .show(ui, |ui| {
                ui.label(RichText::new("Startup").strong());
                ui.add_space(4.0);
                let mut a = self.autostart_on;
                if ui
                    .checkbox(&mut a, "Start with Windows (hidden in tray)")
                    .changed()
                {
                    let exe = std::env::current_exe()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let res = if a { autostart::enable(&exe) } else { autostart::disable() };
                    match res {
                        Ok(()) => {
                            self.autostart_on = a;
                            self.status = if a { "autostart enabled".into() } else { "autostart disabled".into() };
                        }
                        Err(e) => self.status = format!("autostart error — {e}"),
                    }
                }
                ui.label(
                    RichText::new("Adds an entry to HKCU\\…\\Run so the app launches with --hidden on login.")
                        .size(11.5)
                        .color(theme::MUTED),
                );
            });

        ui.add_space(10.0);

        // Files card
        egui::Frame::none()
            .fill(theme::CARD_BG)
            .rounding(Rounding::same(10.0))
            .inner_margin(egui::Margin::same(14.0))
            .show(ui, |ui| {
                ui.label(RichText::new("Files").strong());
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.button("Open config file").clicked() {
                        let path = config::config_path();
                        #[cfg(windows)]
                        {
                            let _ = std::process::Command::new("notepad").arg(&path).spawn();
                        }
                    }
                    if ui.button("Open logs folder").clicked() {
                        self.open_logs_folder();
                    }
                });
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!("config: {}", config::config_path().display()))
                        .size(11.0)
                        .color(theme::MUTED)
                        .monospace(),
                );
            });

        ui.add_space(10.0);

        // Danger card
        egui::Frame::none()
            .fill(theme::CARD_BG)
            .rounding(Rounding::same(10.0))
            .inner_margin(egui::Margin::same(14.0))
            .show(ui, |ui| {
                ui.label(RichText::new("App").strong());
                ui.add_space(4.0);
                if ui
                    .add(
                        egui::Button::new(RichText::new("Quit Crontab").color(Color32::WHITE))
                            .fill(theme::DANGER)
                            .rounding(Rounding::same(8.0)),
                    )
                    .clicked()
                {
                    self.quit_flag.store(true, Ordering::SeqCst);
                }
            });
    }
}

enum JobAction {
    ToggleEnabled(bool),
    Edit,
    Delete,
    RunNow,
}

fn job_card(
    ui: &mut egui::Ui,
    _i: usize,
    job: &Job,
    now: chrono::DateTime<Local>,
    on_action: &mut dyn FnMut(JobAction),
) {
    egui::Frame::none()
        .fill(theme::CARD_BG)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .rounding(Rounding::same(10.0))
        .inner_margin(egui::Margin::same(14.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let mut enabled = job.enabled;
                if ui.add(egui::Checkbox::without_text(&mut enabled)).changed() {
                    on_action(JobAction::ToggleEnabled(enabled));
                }
                ui.label(RichText::new(&job.name).strong().size(14.5));

                ui.add_space(8.0);
                let pill = egui::Frame::none()
                    .fill(Color32::from_rgb(42, 46, 56))
                    .rounding(Rounding::same(12.0))
                    .inner_margin(egui::Margin::symmetric(8.0, 2.0));
                pill.show(ui, |ui| {
                    ui.label(
                        RichText::new(&job.schedule)
                            .size(11.5)
                            .color(theme::ACCENT)
                            .monospace(),
                    );
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("delete").clicked() {
                        on_action(JobAction::Delete);
                    }
                    if ui.small_button("edit").clicked() {
                        on_action(JobAction::Edit);
                    }
                    if ui.small_button("run now").clicked() {
                        on_action(JobAction::RunNow);
                    }
                });
            });

            ui.add_space(4.0);
            ui.label(
                RichText::new(&job.command)
                    .monospace()
                    .size(12.0)
                    .color(if job.enabled { Color32::from_rgb(200, 205, 215) } else { theme::MUTED }),
            );

            ui.add_space(4.0);
            match cron_parse::next_run(&job.schedule, now) {
                Some(n) => {
                    let txt = format!(
                        "next: {}  ·  {}",
                        n.format("%a %H:%M"),
                        cron_parse::human_until(n, now)
                    );
                    ui.label(
                        RichText::new(txt)
                            .size(11.5)
                            .color(if job.enabled { theme::MUTED } else { Color32::from_rgb(100, 105, 115) }),
                    );
                }
                None => {
                    ui.label(RichText::new("invalid schedule").size(11.5).color(theme::DANGER));
                }
            }
        });
}
