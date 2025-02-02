use eframe::egui::{self, CentralPanel, Context, Frame, Visuals};
use egui::Margin;
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Represents a file with its full path, selection state, and optionally its content.
struct FileItem {
    /// The absolute path to the file.
    path: PathBuf,
    /// The relative path (to the chosen folder) used for display.
    rel_path: String,
    selected: bool,
    /// Cached content; once loaded, this is reused.
    content: Option<String>,
}

/// Loads ignore patterns from a file named ".promptignore".
/// Lines starting with '#' or blank lines are ignored.
/// Returns a GlobSet built from the patterns, or falls back to default patterns.
fn load_ignore_set() -> GlobSet {
    let mut builder = GlobSetBuilder::new();
    if let Ok(contents) = fs::read_to_string(".promptignore") {
        for line in contents.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let pattern = if !trimmed.contains('/') {
                format!("**/{}**", trimmed)
            } else {
                trimmed.to_string()
            };
            if let Ok(glob) = Glob::new(&pattern) {
                builder.add(glob);
            }
        }
    } else {
        builder.add(Glob::new("**/target/**").unwrap());
        builder.add(Glob::new("**/.git/**").unwrap());
        builder.add(Glob::new("**/node_modules/**").unwrap());
        builder.add(Glob::new("**/*.tmp").unwrap());
    }
    builder.build().unwrap()
}

/// Our app state stores the file tree plus the text box input.
struct MyApp {
    files: Vec<FileItem>,
    extra_text: String,
    ignore_set: GlobSet,
    generated_prompt: String,
    token_count: usize,
}

impl Default for MyApp {
    fn default() -> Self {
        let mut app = Self {
            files: Vec::new(),
            extra_text: String::new(),
            ignore_set: load_ignore_set(),
            generated_prompt: String::new(),
            token_count: 0,
        };
        if let Ok(cwd) = std::env::current_dir() {
            let file_paths = get_all_files(&cwd);
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

/// Recursively collects all files (not directories) under `dir`.
fn get_all_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                files.push(path);
            } else if path.is_dir() {
                files.extend(get_all_files(&path));
            }
        }
    }
    files
}

/// Compute the prompt preview string based on currently selected files and extra text.
fn compute_prompt(files: &[FileItem], extra_text: &str) -> String {
    let mut prompt = String::new();
    for file_item in files.iter().filter(|f| f.selected) {
        let content = if let Some(ref cached) = file_item.content {
            cached.clone()
        } else {
            match fs::read_to_string(&file_item.path) {
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

/// A simple tree structure for grouping files by folder.
#[derive(Default)]
struct FileTree {
    expanded: bool,
    folders: BTreeMap<String, FileTree>,
    files: Vec<usize>,
}

/// Build a file tree from the list of file items.
fn build_file_tree(files: &Vec<FileItem>) -> FileTree {
    let mut tree = FileTree::default();
    for (i, file) in files.iter().enumerate() {
        let path = file.rel_path.replace('\\', "/");
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current = &mut tree;
        for (j, part) in parts.iter().enumerate() {
            if j == parts.len() - 1 {
                current.files.push(i);
            } else {
                current = current.folders.entry(part.to_string()).or_default();
            }
        }
    }
    tree
}

/// Recursively sort the file tree.
fn sort_file_tree(tree: &mut FileTree, files: &Vec<FileItem>) {
    tree.files.sort_by(|&a, &b| {
        let name_a = files[a].rel_path.rsplit('/').next().unwrap_or("");
        let name_b = files[b].rel_path.rsplit('/').next().unwrap_or("");
        name_a.cmp(name_b)
    });
    for (_, subtree) in tree.folders.iter_mut() {
        sort_file_tree(subtree, files);
    }
}

/// Recursively show the file tree in the UI, with folders first.
fn show_file_tree(ui: &mut egui::Ui, tree: &mut FileTree, files: &mut Vec<FileItem>) {
    // First, show folders.
    for (folder_name, subtree) in tree.folders.iter_mut() {
        egui::CollapsingHeader::new(folder_name)
            .default_open(subtree.expanded)
            .show(ui, |ui| {
                subtree.expanded = true;
                show_file_tree(ui, subtree, files);
            });
    }
    // Then show files.
    for &i in &tree.files {
        let file = &mut files[i];
        let file_name = file.rel_path.rsplit('/').next().unwrap_or(&file.rel_path);
        ui.checkbox(&mut file.selected, file_name);
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // Set partially transparent background.
        let mut visuals = ctx.style().visuals.clone();
        visuals.window_fill = egui::Color32::from_rgba_unmultiplied(0, 0, 0, 128);
        ctx.set_visuals(visuals);

        // Update cached content for selected files.
        for file_item in &mut self.files {
            if file_item.selected && file_item.content.is_none() {
                file_item.content = fs::read_to_string(&file_item.path).ok();
            }
        }
        let preview_prompt = compute_prompt(&self.files, &self.extra_text);
        self.token_count = (preview_prompt.chars().count() as f32 / 4.0).ceil() as usize;

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

                ui.columns(2, |cols| {
                    // Left column: File Tree View.
                    let left = &mut cols[0];
                    if left.button("Select Folder").clicked() {
                        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                            let file_paths = get_all_files(&folder);
                            self.files.clear();
                            for path in file_paths {
                                let rel_path = match path.strip_prefix(&folder) {
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
                    left.separator();
                    let mut tree = build_file_tree(&self.files);
                    sort_file_tree(&mut tree, &self.files);
                    show_file_tree(left, &mut tree, &mut self.files);

                    // Right column: Prompt Text and Generate & Copy.
                    let right = &mut cols[1];
                    right.heading("Prompt Text");
                    right.label("Enter additional prompt text:");
                    right.text_edit_multiline(&mut self.extra_text);
                    right.separator();
                    right.label(format!(
                        "Estimated token count (approx.): {}",
                        self.token_count
                    ));
                    right.separator();
                    if right.button("Generate & Copy Prompt").clicked() {
                        self.generated_prompt = preview_prompt.clone();
                        ctx.output_mut(|o| {
                            o.copied_text = self.generated_prompt.clone();
                        });
                        right.label("Prompt generated and copied to clipboard!");
                    }
                    if !self.generated_prompt.is_empty() {
                        right.separator();
                        right.label("Generated Prompt Preview:");
                        right.text_edit_multiline(&mut self.generated_prompt);
                    }
                });
            });
    }
}

fn main() {
    let mut native_options = eframe::NativeOptions::default();
    native_options.initial_window_size = Some(egui::vec2(1000.0, 600.0));
    native_options.transparent = true; // Enable transparency.
    eframe::run_native(
        "Prompt Generator",
        native_options,
        Box::new(|_cc| Box::new(MyApp::default())),
    );
}
