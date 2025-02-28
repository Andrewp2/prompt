use core::f32;
use eframe::egui::{self};
use std::{env, path::PathBuf, time::Instant};

use crate::{
    file_item::{FileItem, MAX_FILES},
    file_tree::{build_file_tree, generate_file_tree_string, show_file_tree, sort_file_tree},
};

pub struct MyApp {
    pub files: Vec<FileItem>,
    pub extra_text: String,
    pub ignore_set: globset::GlobSet,
    pub generated_prompt: String,
    pub token_count: usize,
    pub current_folder: Option<PathBuf>,
    pub include_file_tree: bool, // Toggle inclusion of file tree.
    pub notification: Option<(String, Instant)>,
}

impl MyApp {
    /// Refresh the file list based on the current folder.
    pub fn refresh_files(&mut self) {
        if let Some(ref folder) = self.current_folder {
            // Reload the ignore set from the selected folder.
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
                self.files.push(crate::file_item::FileItem {
                    path,
                    rel_path,
                    selected: false,
                    content: None,
                });
            }
        }
    }
}

impl Default for MyApp {
    fn default() -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
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
        };
        let file_paths = crate::file_item::get_all_files_limited(
            &cwd,
            crate::file_item::MAX_FILES,
            &app.ignore_set,
        );
        for path in file_paths {
            let rel_path = match path.strip_prefix(&cwd) {
                Ok(rel) => rel.to_string_lossy().to_string(),
                Err(_) => path.to_string_lossy().to_string(),
            };
            if app.ignore_set.is_match(&rel_path) {
                continue;
            }
            app.files.push(crate::file_item::FileItem {
                path,
                rel_path,
                selected: false,
                content: None,
            });
        }
        app
    }
}

/// Helper function to compute the prompt from the selected files and extra text.
fn compute_prompt(files: &[FileItem], extra_text: &str) -> String {
    let mut prompt = String::new();
    for file_item in files.iter().filter(|f| f.selected) {
        let content = if let Some(ref cached) = file_item.content {
            cached.clone()
        } else {
            match std::fs::read_to_string(&file_item.path) {
                Ok(contents) => contents,
                Err(err) => format!("Error reading {}: {}", file_item.rel_path, err),
            }
        };
        prompt.push_str(&format!("```{}\n", file_item.rel_path));
        prompt.push_str(&content);
        prompt.push_str("\n```\n\n");
    }
    prompt.push_str(extra_text);
    prompt
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Example: set a semi-transparent background if you want
        let mut visuals = ctx.style().visuals.clone();
        visuals.window_fill = egui::Color32::from_rgba_unmultiplied(0, 0, 0, 128);
        ctx.set_visuals(visuals);

        // Update file contents if selected
        for file_item in &mut self.files {
            if file_item.selected && file_item.content.is_none() {
                file_item.content = std::fs::read_to_string(&file_item.path).ok();
            }
        }

        // Build final prompt text
        let base_prompt = compute_prompt(&self.files, &self.extra_text);
        let final_prompt = if self.include_file_tree {
            let base = self
                .current_folder
                .as_deref()
                .unwrap_or(std::path::Path::new("."));
            let tree_text = generate_file_tree_string(&self.files, base);
            format!("{}\n\n{}", tree_text, base_prompt)
        } else {
            base_prompt
        };
        self.token_count = (final_prompt.chars().count() as f32 / 4.0).ceil() as usize;

        // Left panel: File tree
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

                // Show the file tree
                let mut tree = build_file_tree(&self.files);
                sort_file_tree(&mut tree, &self.files);
                show_file_tree(ui, &tree, &mut self.files);
            });

        egui::TopBottomPanel::bottom("bottom_panel")
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.set_height(30.0);
                    ui.checkbox(&mut self.include_file_tree, "Include file tree in prompt");
                    ui.separator();
                    ui.label(format!(
                        "Token count: {} / 200,000 ({:.2}%)",
                        self.token_count,
                        (self.token_count as f32 / 200000.0) * 100.0
                    ));
                    ui.separator();
                    if ui.button("Copy Prompt").clicked() {
                        for file_item in self.files.iter_mut().filter(|f| f.selected) {
                            file_item.content = std::fs::read_to_string(&file_item.path).ok();
                        }
                        let prompt = compute_prompt(&self.files, &self.extra_text);
                        let final_prompt = if self.include_file_tree {
                            let base = self
                                .current_folder
                                .as_deref()
                                .unwrap_or(std::path::Path::new("."));
                            let tree_text = generate_file_tree_string(&self.files, base);
                            format!("{}\n\n{}", tree_text, prompt)
                        } else {
                            prompt
                        };
                        self.generated_prompt = final_prompt.clone();
                        ctx.copy_text(final_prompt);
                        self.notification =
                            Some(("Prompt copied to clipboard!".to_owned(), Instant::now()));
                    }
                    const NOTIFICATION_DURATION: f32 = 3.0; // seconds
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
                            // Request a repaint so that the fade-out animation updates.
                            ctx.request_repaint();
                        } else {
                            // Clear the notification after the duration expires.
                            self.notification = None;
                        }
                    }
                });
            });

        // Central panel: multiline text editor
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .max_height(f32::INFINITY)
                .max_width(f32::INFINITY)
                .show(ui, |ui| {
                    //ui.put(max_rect, widget)
                    ui.add_sized(
                        [ui.available_width(), ui.available_height()],
                        egui::TextEdit::multiline(&mut self.extra_text)
                            .lock_focus(true)
                            .desired_width(f32::INFINITY)
                            .desired_rows(4)
                            .frame(true),
                    );
                });
        });
    }
}

/// Runs the application.
pub fn run() {
    let mut app = MyApp::default();
    // If a folder is provided as a command-line argument, use it.
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
