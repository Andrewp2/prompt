use eframe::egui::{self, CentralPanel, Context, Frame, ScrollArea};
use egui::Margin;
use std::env;
use std::path::{Path, PathBuf};

use crate::file_item::{get_all_files_limited, load_ignore_set, FileItem, MAX_FILES};
use crate::file_tree::{
    build_file_tree, generate_file_tree_string, show_file_tree, sort_file_tree,
};

pub struct MyApp {
    pub files: Vec<FileItem>,
    pub extra_text: String,
    pub ignore_set: globset::GlobSet,
    pub generated_prompt: String,
    pub token_count: usize,
    pub current_folder: Option<PathBuf>,
    pub include_file_tree: bool, // Toggle inclusion of file tree.
}

impl MyApp {
    /// Refresh the file list based on the current folder.
    pub fn refresh_files(&mut self) {
        if let Some(ref folder) = self.current_folder {
            let file_paths = get_all_files_limited(folder, MAX_FILES, &self.ignore_set);
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
}

impl Default for MyApp {
    fn default() -> Self {
        let mut app = Self {
            files: Vec::new(),
            extra_text: String::new(),
            ignore_set: load_ignore_set(),
            generated_prompt: String::new(),
            token_count: 0,
            current_folder: None,
            include_file_tree: true, // Checked by default.
        };
        if let Ok(cwd) = std::env::current_dir() {
            app.current_folder = Some(cwd.clone());
            let file_paths = get_all_files_limited(&cwd, MAX_FILES, &app.ignore_set);
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
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // Set a partially transparent background.
        let mut visuals = ctx.style().visuals.clone();
        visuals.window_fill = egui::Color32::from_rgba_unmultiplied(0, 0, 0, 128);
        ctx.set_visuals(visuals);

        // Update file contents if selected.
        for file_item in &mut self.files {
            if file_item.selected && file_item.content.is_none() {
                file_item.content = std::fs::read_to_string(&file_item.path).ok();
            }
        }

        // Build the base prompt from selected files and extra text.
        let base_prompt = compute_prompt(&self.files, &self.extra_text);
        // If the file tree is enabled, prepend its string.
        let final_prompt = if self.include_file_tree {
            let base = self.current_folder.as_deref().unwrap_or(Path::new("."));
            let tree_text = generate_file_tree_string(&self.files, base);
            format!("{}\n\n{}", tree_text, base_prompt)
        } else {
            base_prompt
        };
        // Compute token count (approx. 4 characters per token).
        self.token_count = (final_prompt.chars().count() as f32 / 4.0).ceil() as usize;

        let frame = Frame::none().inner_margin(Margin {
            left: 5.0,
            right: 5.0,
            top: 0.0,
            bottom: 0.0,
        });

        CentralPanel::default()
            .frame(frame.fill(egui::Color32::from_rgba_unmultiplied(25, 25, 25, 220)))
            .show(ctx, |ui| {
                ui.heading("Prompt Generator");

                let available_width = ui.available_width();
                ui.horizontal(|ui| {
                    // Left panel: File selection.
                    ui.vertical(|ui| {
                        ui.set_width(available_width * 0.25);
                        ui.horizontal(|ui| {
                            if ui.button("Select Folder").clicked() {
                                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                                    self.current_folder = Some(folder.clone());
                                    self.refresh_files();
                                }
                            }
                            if self.current_folder.is_some() {
                                if ui.small_button("Refresh").clicked() {
                                    self.refresh_files();
                                }
                            }
                        });
                        ui.separator();
                        let mut tree = build_file_tree(&self.files);
                        sort_file_tree(&mut tree, &self.files);
                        show_file_tree(ui, &tree, &mut self.files);
                    });
                    // Right panel: Prompt creation.
                    ui.vertical(|ui| {
                        ui.set_width(available_width * 0.75);
                        ui.heading("Prompt Text");
                        ui.label("Enter additional prompt text:");
                        ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                            ui.text_edit_multiline(&mut self.extra_text);
                        });
                        ui.separator();
                        ui.checkbox(&mut self.include_file_tree, "Include file tree in prompt");
                        ui.separator();
                        ui.label(format!(
                            "Token count: {} / 200,000 {:.2}%",
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
                                let base = self.current_folder.as_deref().unwrap_or(Path::new("."));
                                let tree_text = generate_file_tree_string(&self.files, base);
                                format!("{}\n\n{}", tree_text, prompt)
                            } else {
                                prompt
                            };
                            self.generated_prompt = final_prompt.clone();
                            ctx.output_mut(|o| {
                                o.copied_text = final_prompt;
                            });
                            ui.label("Prompt copied to clipboard!");
                        }
                    });
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
    let mut native_options = eframe::NativeOptions::default();
    native_options.initial_window_size = Some(egui::vec2(1000.0, 600.0));
    native_options.transparent = true;
    eframe::run_native(
        "Prompt Generator",
        native_options,
        Box::new(|_cc| Box::new(app)),
    );
}
