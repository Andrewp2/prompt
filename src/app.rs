use crate::{
    command_runner::{run_command, Terminal},
    file_item::{FileItem, MAX_FILES},
    file_tree::{build_file_tree, generate_file_tree_string, show_file_tree, sort_file_tree},
    prompt_builder::{compute_prompt, extract_text},
    remote::{Remote, RemoteUpdate, RemoteUrl},
};
use core::f32;
use eframe::egui;
use globset::GlobSet;
use num_format::{Locale, ToFormattedString};
use std::{
    env,
    path::PathBuf,
    time::{Duration, Instant},
};

pub struct MyApp {
    pub files: Vec<FileItem>,
    pub extra_text: String,
    pub ignore_set: GlobSet,
    pub generated_prompt: String,
    pub token_count: usize,
    pub current_folder: Option<PathBuf>,
    pub include_file_tree: bool,
    pub notification: Option<(String, Instant)>,

    pub remote: Remote,
    pub terminal: Terminal,
}

impl MyApp {
    pub fn refresh_files(&mut self) {
        if let Some(ref folder) = self.current_folder {
            self.ignore_set = crate::file_item::load_ignore_set_from(folder);
            let file_paths =
                crate::file_item::get_all_files_limited(folder, MAX_FILES, &self.ignore_set);
            self.files.clear();
            for path in file_paths {
                let rel_path = match path.strip_prefix(folder) {
                    Ok(rel) => rel.to_string_lossy().to_string(),
                    Err(_) => path.to_string_lossy().to_string(),
                };
                if self.ignore_set.is_match(&rel_path) {
                    continue;
                }
                self.files.push(FileItem {
                    path,
                    rel_path,
                    selected: false,
                    content: None,
                });
            }
        }
    }

    fn remote_url_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("remote_url_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Remote URL:");
                ui.text_edit_singleline(&mut self.remote.new_url);
                if ui.button("Add URL").clicked() && !self.remote.new_url.is_empty() {
                    self.remote.remote_urls.push(RemoteUrl {
                        url: self.remote.new_url.clone(),
                        content: None,
                        include: false,
                    });
                    let index = self.remote.remote_urls.len() - 1;
                    let url = self.remote.remote_urls[index].url.clone();
                    let tx = self.remote.remote_update_tx.clone();
                    std::thread::spawn(move || {
                        match reqwest::blocking::get(&url).and_then(|resp| resp.text()) {
                            Ok(text) => {
                                let _ = tx.send(RemoteUpdate::Fetched {
                                    index,
                                    content: extract_text(&text),
                                });
                            }
                            Err(err) => {
                                eprintln!("Error fetching {}: {:?}", url, err);
                            }
                        }
                    });
                    self.remote.new_url.clear();
                }
            });
            for i in (0..self.remote.remote_urls.len()).rev() {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.remote.remote_urls[i].include, "Include");
                    ui.label(&self.remote.remote_urls[i].url);
                    if ui.button("Re-fetch").clicked() {
                        let url = self.remote.remote_urls[i].url.clone();
                        let tx = self.remote.remote_update_tx.clone();
                        let index = i;
                        std::thread::spawn(move || {
                            match reqwest::blocking::get(&url).and_then(|resp| resp.text()) {
                                Ok(text) => {
                                    let _ = tx.send(RemoteUpdate::Fetched {
                                        index,
                                        content: extract_text(&text),
                                    });
                                }
                                Err(err) => {
                                    eprintln!("Error re-fetching {}: {:?}", url, err);
                                }
                            }
                        });
                    }
                    if ui.button("Remove").clicked() {
                        self.remote.remote_urls.remove(i);
                    }
                });
            }
        });
    }

    fn file_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("left_panel")
            .resizable(true)
            .default_width(300.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Select Folder").clicked() {
                        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                            self.current_folder = Some(folder.clone());
                            self.refresh_files();
                        }
                    }
                    if ui.button("Refresh").clicked() {
                        self.refresh_files();
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("file_tree_scroll_area")
                    .show(ui, |ui| {
                        let mut tree = build_file_tree(&self.files);
                        sort_file_tree(&mut tree, &self.files);
                        show_file_tree(ui, &tree, &mut self.files);
                    });
            });
    }

    fn bottom_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("bottom_panel")
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.set_height(30.0);
                    ui.checkbox(&mut self.include_file_tree, "Include file tree in prompt");
                    ui.separator();
                    let prompt =
                        compute_prompt(&self.files, &self.extra_text, &self.remote.remote_urls);
                    self.token_count = ((prompt.chars().count() as f32) / 4.0).ceil() as usize;
                    let formatted_token_count = self.token_count.to_formatted_string(&Locale::en);
                    ui.label(format!(
                        "Token count: {} / 200,000 ({:.2}%)",
                        formatted_token_count,
                        (self.token_count as f32 / 200_000.0) * 100.0
                    ));
                    ui.separator();
                    if ui.button("Copy Prompt").clicked() {
                        compute_and_copy_prompt(self, ctx);
                    }
                    const NOTIFICATION_DURATION: f32 = 3.0;
                    if let Some((message, start)) = &self.notification {
                        let elapsed = start.elapsed().as_secs_f32();
                        if elapsed < NOTIFICATION_DURATION {
                            let alpha = 1.0 - (elapsed / NOTIFICATION_DURATION);
                            let text = egui::RichText::new(message).color(
                                egui::Color32::from_rgba_unmultiplied(
                                    255,
                                    255,
                                    255,
                                    (alpha * 255.0) as u8,
                                ),
                            );
                            ui.label(text);
                            ctx.request_repaint();
                        } else {
                            self.notification = None;
                        }
                    }
                });
            });
    }

    fn central_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.label("User Prompt:");

                egui::ScrollArea::vertical()
                    .max_height(350.0)
                    .id_salt("user_prompt_scroll_area")
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.extra_text)
                                .lock_focus(true)
                                .desired_width(f32::INFINITY)
                                .desired_rows(8)
                                .frame(true),
                        );
                    });

                // ui.add(
                //     egui::TextEdit::multiline(&mut self.extra_text)
                //         .lock_focus(true)
                //         .desired_width(f32::INFINITY)
                //         .desired_rows(8)
                //         .frame(true),
                // );
                ui.separator();
                ui.heading("Terminal Command");
                ui.add(
                    egui::TextEdit::singleline(&mut self.terminal.terminal_command)
                        .desired_width(f32::INFINITY)
                        .frame(true),
                );
                ui.horizontal(|ui| {
                    ui.label("Head lines:");
                    ui.add(egui::DragValue::new(&mut self.terminal.head_lines));
                    ui.label("Tail lines:");
                    ui.add(egui::DragValue::new(&mut self.terminal.tail_lines));
                    ui.label("Timeout (sec, 0 = no timeout):");
                    ui.add(egui::DragValue::new(&mut self.terminal.timeout_secs));
                });
                if ui.button("Run Command").clicked() {
                    let command = self.terminal.terminal_command.clone();
                    let tokens: Vec<String> =
                        command.split_whitespace().map(String::from).collect();
                    if tokens.is_empty() {
                        return;
                    }
                    let cmd = tokens[0].clone();
                    let args: Vec<String> = tokens[1..].to_vec();
                    let head = self.terminal.head_lines;
                    let tail = self.terminal.tail_lines;
                    let timeout = self.terminal.timeout_secs;
                    let tx = self.terminal.terminal_update_tx.clone();
                    let working_dir = self
                        .current_folder
                        .clone()
                        .unwrap_or_else(|| std::env::current_dir().unwrap());
                    std::thread::spawn(move || {
                        let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();
                        let do_timeout = timeout > 0;
                        let output = run_command(
                            &working_dir,
                            &cmd,
                            &args_ref,
                            head,
                            tail,
                            do_timeout,
                            Duration::from_secs(timeout),
                        );
                        let _ = tx.send(output);
                    });
                }
                ui.separator();
                ui.label("Terminal Output:");
                // ui.add(
                //     egui::TextEdit::multiline(&mut self.terminal.terminal_output)
                //         .lock_focus(true)
                //         .desired_width(f32::INFINITY)
                //         .frame(true)
                //         .desired_rows(8),
                // );
                egui::ScrollArea::vertical()
                    .max_height(350.0)
                    .id_salt("terminal_output_scroll_area")
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.terminal.terminal_output)
                                .lock_focus(true)
                                .desired_width(f32::INFINITY)
                                .desired_rows(8)
                                .frame(true),
                        );
                    });
                if ui.button("Copy Output to Prompt").clicked() {
                    self.extra_text.push('\n');
                    self.extra_text.push_str(&self.terminal.terminal_command);
                    self.extra_text.push('\n');
                    self.extra_text.push_str(&self.terminal.terminal_output);
                }
            });
        });
        ctx.request_repaint();
    }
}

fn compute_and_copy_prompt(app: &mut MyApp, ctx: &egui::Context) {
    for file_item in app.files.iter_mut().filter(|f| f.selected) {
        file_item.content = std::fs::read_to_string(&file_item.path).ok();
    }
    let base_prompt = compute_prompt(&app.files, &app.extra_text, &app.remote.remote_urls);
    let final_prompt = if app.include_file_tree {
        let base = app
            .current_folder
            .as_deref()
            .unwrap_or(std::path::Path::new("."));
        let tree_text = generate_file_tree_string(&app.files, base);
        format!("{}\n\n{}", tree_text, base_prompt)
    } else {
        base_prompt
    };
    app.token_count = (final_prompt.chars().count() as f32 / 4.0).ceil() as usize;
    app.generated_prompt = final_prompt.clone();
    ctx.copy_text(final_prompt);
    app.notification = Some(("Prompt copied to clipboard!".to_owned(), Instant::now()));
}

impl Default for MyApp {
    fn default() -> Self {
        let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let ignore_set = crate::file_item::load_ignore_set_from(&cwd);

        let mut app = Self {
            files: Vec::new(),
            extra_text: String::new(),
            ignore_set,
            generated_prompt: String::new(),
            token_count: 0,
            current_folder: Some(cwd.clone()),
            include_file_tree: true,
            notification: None,
            remote: Remote::default(),
            terminal: Terminal::default(),
        };

        let file_paths = crate::file_item::get_all_files_limited(&cwd, MAX_FILES, &app.ignore_set);
        for path in file_paths {
            let rel_path = match path.strip_prefix(&cwd) {
                Ok(rel) => rel.to_string_lossy().to_string(),
                Err(_) => path.to_string_lossy().to_string(),
            };
            if app.ignore_set.is_match(&rel_path) {
                continue;
            }
            app.files.push(FileItem {
                path,
                rel_path,
                selected: false,
                content: None,
            });
        }
        app
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(update) = self.remote.remote_update_rx.try_recv() {
            let RemoteUpdate::Fetched { index, content } = update;
            if let Some(remote) = self.remote.remote_urls.get_mut(index) {
                remote.content = Some(content);
            }
        }
        while let Ok(output) = self.terminal.terminal_update_rx.try_recv() {
            self.terminal.terminal_output = output;
        }

        self.remote_url_panel(ctx);

        self.file_panel(ctx);

        self.bottom_panel(ctx);

        self.central_panel(ctx);
    }
}

pub fn run() {
    let mut app = MyApp::default();
    if let Some(arg) = env::args().nth(1) {
        let folder = PathBuf::from(arg);
        if folder.is_dir() {
            app.current_folder = Some(folder);
            app.refresh_files();
        } else {
            eprintln!("Warning: Provided argument is not a valid directory.");
        }
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1920.0, 1080.0])
            .with_transparent(true),
        ..Default::default()
    };
    let _ = eframe::run_native(
        "Prompt Generator",
        options,
        Box::new(|_cc| Ok(Box::new(app))),
    );
}
