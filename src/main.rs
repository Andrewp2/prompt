use eframe::egui::{self, CentralPanel, Context, Frame, ScrollArea, Visuals};
use egui::Margin;
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Maximum number of files to load.
const MAX_FILES: usize = 10_000;

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
            // If the pattern doesn't include a path separator, wrap it so that it matches anywhere.
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
        // Fallback defaults.
        builder.add(Glob::new("**/target/**").unwrap());
        builder.add(Glob::new("**/.git/**").unwrap());
        builder.add(Glob::new("**/node_modules/**").unwrap());
        builder.add(Glob::new("**/*.tmp").unwrap());
    }
    builder.build().unwrap()
}

/// A file tree structure for grouping files by folder.
#[derive(Default)]
struct FileTree {
    folders: BTreeMap<String, FileTree>,
    files: Vec<usize>,
}

/// Recursively builds a file tree from the list of file items.
fn build_file_tree(files: &Vec<FileItem>) -> FileTree {
    let mut root = FileTree {
        folders: BTreeMap::new(),
        files: Vec::new(),
    };

    for (i, file) in files.iter().enumerate() {
        // Normalize the path to use forward slashes.
        let path = file.rel_path.replace('\\', "/");
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current = &mut root;
        for (j, part) in parts.iter().enumerate() {
            if j == parts.len() - 1 {
                // Last part is the file name.
                current.files.push(i);
            } else {
                current = current.folders.entry(part.to_string()).or_default();
            }
        }
    }
    root
}

/// Recursively sort the file tree so that files are in alphabetical order.
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

/// Returns true if every file in this tree (including subfolders) is selected.
fn get_folder_all_selected(tree: &FileTree, files: &Vec<FileItem>) -> bool {
    for &i in &tree.files {
        if !files[i].selected {
            return false;
        }
    }
    for (_, sub_tree) in &tree.folders {
        if !get_folder_all_selected(sub_tree, files) {
            return false;
        }
    }
    true
}

/// Recursively set the selection state for all files in this tree.
fn set_folder_selection(tree: &FileTree, files: &mut Vec<FileItem>, value: bool) {
    for &i in &tree.files {
        files[i].selected = value;
    }
    for (_, sub_tree) in &tree.folders {
        set_folder_selection(sub_tree, files, value);
    }
}

/// Our app state stores the file tree plus the text input and other state.
struct MyApp {
    files: Vec<FileItem>,
    extra_text: String,
    ignore_set: GlobSet,
    generated_prompt: String,
    token_count: usize,
    /// Stores the currently selected folder.
    current_folder: Option<PathBuf>,
}

impl MyApp {
    /// Refresh the file list based on the current folder.
    fn refresh_files(&mut self) {
        if let Some(ref folder) = self.current_folder {
            // Get files while applying the ignore rules.
            let file_paths = get_all_files_limited(folder, MAX_FILES, &self.ignore_set);
            self.files.clear();
            for path in file_paths {
                let rel_path = match path.strip_prefix(folder) {
                    Ok(rel) => rel.to_string_lossy().to_string(),
                    Err(_) => path.to_string_lossy().to_string(),
                };
                // (Extra filtering here is optional, since get_all_files_limited already applied the rules.)
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

/// Walks the directory tree starting at `base`, applying ignore rules.
/// It stops collecting files once the specified limit is reached. If more files exist,
/// a warning dialog is shown.
fn get_all_files_limited(base: &Path, limit: usize, ignore_set: &GlobSet) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut dirs = vec![base.to_path_buf()];

    while let Some(current_dir) = dirs.pop() {
        // Compute the relative path for the current directory.
        let rel_dir = current_dir.strip_prefix(base).unwrap_or(&current_dir);
        if ignore_set.is_match(rel_dir.to_string_lossy().as_ref()) {
            continue;
        }

        if let Ok(entries) = fs::read_dir(&current_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let rel_path = match path.strip_prefix(base) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                let rel_path_str = rel_path.to_string_lossy();
                if path.is_file() {
                    if ignore_set.is_match(rel_path_str.as_ref()) {
                        continue;
                    }
                    files.push(path);
                    if files.len() >= limit {
                        break;
                    }
                } else if path.is_dir() {
                    if ignore_set.is_match(rel_path_str.as_ref()) {
                        continue;
                    }
                    dirs.push(path);
                }
            }
        }
        if files.len() >= limit {
            break;
        }
    }

    if files.len() >= limit {
        rfd::MessageDialog::new()
            .set_title("Warning")
            .set_description(&format!(
                "More than {} files detected. Only the first {} files will be loaded.",
                limit, limit
            ))
            .set_level(rfd::MessageLevel::Warning)
            .show();
    }
    files.truncate(limit);
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

/// Recursively show the file tree in the UI using egui's CollapsingHeader.
fn show_file_tree(ui: &mut egui::Ui, tree: &FileTree, files: &mut Vec<FileItem>) {
    for (folder_name, subtree) in &tree.folders {
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
            let old_spacing = ui.spacing().item_spacing;
            ui.spacing_mut().item_spacing.x = 0.5;

            // "Select All" checkbox.
            let mut folder_selected = get_folder_all_selected(subtree, files);
            if ui.checkbox(&mut folder_selected, "").changed() {
                set_folder_selection(subtree, files, folder_selected);
            }
            // CollapsingHeader for the folder.
            ui.collapsing(folder_name, |ui| {
                show_file_tree(ui, subtree, files);
            });
            ui.spacing_mut().item_spacing = old_spacing;
        });
    }
    // Display files at this level.
    for &i in &tree.files {
        let file = &mut files[i];
        let file_name = file.rel_path.rsplit('/').next().unwrap_or(&file.rel_path);
        ui.checkbox(&mut file.selected, file_name);
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // Set a partially transparent background.
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

                    left.horizontal(|ui| {
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

                    left.separator();
                    let mut tree = build_file_tree(&self.files);
                    sort_file_tree(&mut tree, &self.files);
                    show_file_tree(left, &tree, &mut self.files);

                    // Right column: Prompt Text and Generate & Copy.
                    let right = &mut cols[1];
                    right.heading("Prompt Text");
                    right.label("Enter additional prompt text:");
                    right.text_edit_multiline(&mut self.extra_text);
                    right.separator();
                    right.label(format!(
                        "Estimated token count (approx.): {} / 200,000 {:.2}%",
                        self.token_count,
                        (self.token_count as f32 / 200000.0) * 100.0
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
                        ScrollArea::vertical().max_height(200.0).show(right, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.generated_prompt)
                                    .desired_rows(10)
                                    .lock_focus(true)
                                    .desired_width(f32::INFINITY),
                            );
                        });
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
