use crate::autostart;
use crate::config::{self, Job};
use crate::cron_parse;
use crate::scheduler::{self, SharedState};
use chrono::Local;
use eframe::egui;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct App {
    state: Arc<SharedState>,
    show_window: Arc<AtomicBool>,
    last_visibility_sent: Option<bool>,
    quit_flag: Arc<AtomicBool>,
    tray: Option<crate::tray::TrayHandles>,
    // Edit form
    edit_index: Option<usize>, // None = new
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
            tray,
            edit_index: None,
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
            self.form_error = Some("name required".into());
            return;
        }
        if let Err(e) = cron_parse::validate(&self.form_schedule) {
            self.form_error = Some(format!("invalid schedule: {e}"));
            return;
        }
        if self.form_command.trim().is_empty() {
            self.form_error = Some("command required".into());
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
            self.status = format!("save failed: {e}");
        } else {
            self.status = "saved".into();
            *self.state.config_mtime.lock().unwrap() = config::mtime(&config::config_path());
            self.reset_form();
        }
    }

    fn reset_form(&mut self) {
        self.edit_index = None;
        self.form_name.clear();
        self.form_schedule = "*/5 * * * *".into();
        self.form_command.clear();
        self.form_error = None;
    }

    fn persist(&mut self) {
        let cfg = self.state.config.lock().unwrap().clone();
        if let Err(e) = config::save(&cfg) {
            self.status = format!("save failed: {e}");
        } else {
            *self.state.config_mtime.lock().unwrap() = config::mtime(&config::config_path());
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Pump tray events
        if let Some(tray) = &self.tray {
            crate::tray::poll(tray, &self.show_window, &self.quit_flag, &self.state);
        }

        // Hide-on-close: catch close request, hide instead
        if ctx.input(|i| i.viewport().close_requested()) {
            self.show_window.store(false, Ordering::SeqCst);
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        // Sync visibility from shared flag (tray can change it). Only send the
        // command when the desired state changes — otherwise we'd spam Focus
        // on every frame, stealing focus from other apps.
        let want_visible = self.show_window.load(Ordering::SeqCst);
        if self.last_visibility_sent != Some(want_visible) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(want_visible));
            if want_visible {
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            self.last_visibility_sent = Some(want_visible);
        }

        // Quit if tray asked
        if self.quit_flag.load(Ordering::SeqCst) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            std::process::exit(0);
        }

        ctx.request_repaint_after(std::time::Duration::from_secs(1));

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Claude Cron");

            ui.horizontal(|ui| {
                let mut master = self.state.config.lock().unwrap().master_enabled;
                if ui.checkbox(&mut master, "Master enabled").changed() {
                    self.state.config.lock().unwrap().master_enabled = master;
                    self.persist();
                }
                if ui.button("Open logs folder").clicked() {
                    let dir = {
                        let cfg = self.state.config.lock().unwrap();
                        if cfg.log_dir.is_empty() {
                            config::default_log_dir()
                        } else {
                            std::path::PathBuf::from(&cfg.log_dir)
                        }
                    };
                    let _ = std::fs::create_dir_all(&dir);
                    #[cfg(windows)]
                    {
                        let _ = std::process::Command::new("explorer")
                            .arg(&dir)
                            .spawn();
                    }
                }
                if ui.button("Open config file").clicked() {
                    let path = config::config_path();
                    #[cfg(windows)]
                    {
                        let _ = std::process::Command::new("notepad")
                            .arg(&path)
                            .spawn();
                    }
                }
            });

            ui.separator();

            // Jobs list
            ui.heading("Jobs");
            let mut to_delete: Option<usize> = None;
            let mut to_edit: Option<usize> = None;
            let mut to_test: Option<usize> = None;
            let mut changed = false;

            let now = Local::now();
            let jobs_snapshot = self.state.config.lock().unwrap().jobs.clone();

            if jobs_snapshot.is_empty() {
                ui.label(egui::RichText::new("No jobs yet — add one below.").italics());
            }

            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                for (i, job) in jobs_snapshot.iter().enumerate() {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            let mut enabled = job.enabled;
                            if ui.checkbox(&mut enabled, "").changed() {
                                self.state.config.lock().unwrap().jobs[i].enabled = enabled;
                                changed = true;
                            }
                            ui.label(egui::RichText::new(&job.name).strong());
                            ui.label(egui::RichText::new(&job.schedule).monospace());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("delete").clicked() {
                                    to_delete = Some(i);
                                }
                                if ui.small_button("edit").clicked() {
                                    to_edit = Some(i);
                                }
                                if ui.small_button("run now").clicked() {
                                    to_test = Some(i);
                                }
                            });
                        });
                        ui.label(egui::RichText::new(&job.command).monospace().weak());
                        match cron_parse::next_run(&job.schedule, now) {
                            Some(n) => {
                                ui.label(format!(
                                    "next: {}  ({})",
                                    n.format("%Y-%m-%d %H:%M"),
                                    cron_parse::human_until(n, now)
                                ));
                            }
                            None => {
                                ui.colored_label(egui::Color32::RED, "invalid schedule");
                            }
                        }
                    });
                }
            });

            if let Some(i) = to_delete {
                self.state.config.lock().unwrap().jobs.remove(i);
                changed = true;
            }
            if let Some(i) = to_edit {
                let j = &jobs_snapshot[i];
                self.edit_index = Some(i);
                self.form_name = j.name.clone();
                self.form_schedule = j.schedule.clone();
                self.form_command = j.command.clone();
                self.form_error = None;
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
                self.status = format!("fired '{}' now", j.name);
            }
            if changed {
                self.persist();
            }

            ui.separator();

            // Add / edit form
            ui.heading(if self.edit_index.is_some() { "Edit job" } else { "Add job" });
            egui::Grid::new("form").num_columns(2).show(ui, |ui| {
                ui.label("Name");
                ui.text_edit_singleline(&mut self.form_name);
                ui.end_row();

                ui.label("Schedule");
                ui.text_edit_singleline(&mut self.form_schedule);
                ui.end_row();

                ui.label("");
                match cron_parse::validate(&self.form_schedule) {
                    Ok(()) => {
                        let next = cron_parse::next_run(&self.form_schedule, now);
                        match next {
                            Some(n) => ui.label(format!(
                                "next: {}  ({})",
                                n.format("%Y-%m-%d %H:%M"),
                                cron_parse::human_until(n, now)
                            )),
                            None => ui.label("no upcoming runs"),
                        };
                    }
                    Err(e) => {
                        ui.colored_label(egui::Color32::RED, format!("invalid: {e}"));
                    }
                }
                ui.end_row();

                ui.label("Command");
                ui.text_edit_multiline(&mut self.form_command);
                ui.end_row();
            });

            ui.label("Presets:");
            ui.horizontal_wrapped(|ui| {
                for (label, expr) in cron_parse::PRESETS {
                    if ui.small_button(*label).clicked() {
                        self.form_schedule = (*expr).into();
                    }
                }
            });

            ui.horizontal(|ui| {
                if ui.button(if self.edit_index.is_some() { "Update" } else { "Add" }).clicked() {
                    self.save_form();
                }
                if self.edit_index.is_some() && ui.button("Cancel").clicked() {
                    self.reset_form();
                }
            });

            if let Some(err) = &self.form_error {
                ui.colored_label(egui::Color32::RED, err);
            }

            ui.separator();

            ui.horizontal(|ui| {
                let mut a = self.autostart_on;
                if ui.checkbox(&mut a, "Start with Windows (hidden)").changed() {
                    let exe = std::env::current_exe()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let res = if a { autostart::enable(&exe) } else { autostart::disable() };
                    match res {
                        Ok(()) => {
                            self.autostart_on = a;
                            self.status = if a { "autostart enabled".into() } else { "autostart disabled".into() };
                        }
                        Err(e) => self.status = format!("autostart error: {e}"),
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Quit app").clicked() {
                        self.quit_flag.store(true, Ordering::SeqCst);
                    }
                });
            });

            // Recent runs
            ui.separator();
            ui.collapsing("Recent runs", |ui| {
                let runs = self.state.last_runs.lock().unwrap().clone();
                if runs.is_empty() {
                    ui.label("(none yet)");
                } else {
                    for (name, when, code) in runs.iter().rev().take(15) {
                        let color = if *code == 0 {
                            egui::Color32::from_rgb(80, 200, 120)
                        } else {
                            egui::Color32::from_rgb(220, 80, 80)
                        };
                        ui.colored_label(
                            color,
                            format!("{}  {}  exit={}", when.format("%H:%M:%S"), name, code),
                        );
                    }
                }
            });

            if !self.status.is_empty() {
                ui.separator();
                ui.label(egui::RichText::new(&self.status).italics().weak());
            }
        });
    }
}
