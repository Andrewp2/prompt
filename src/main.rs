use eframe::egui::{self, CentralPanel, Context, Frame, ScrollArea};
use egui::Margin;
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// Maximum number of files to load.
const MAX_FILES: usize = 10_000;

/// Represents a file with its full path, selection state, and optionally its content.
#[derive(Clone)]
struct FileItem {
    path: PathBuf,
    rel_path: String,
    selected: bool,
    content: Option<String>,
}

/// Loads ignore patterns from a file named ".promptignore".
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

/// A file tree structure for grouping files by folder.
#[derive(Default)]
struct FileTree {
    folders: BTreeMap<String, FileTree>,
    files: Vec<usize>,
}

/// Recursively builds a file tree from the list of file items.
fn build_file_tree(files: &[FileItem]) -> FileTree {
    let mut root = FileTree {
        folders: BTreeMap::new(),
        files: Vec::new(),
    };
    for (i, file) in files.iter().enumerate() {
        let path = file.rel_path.replace('\\', "/");
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current = &mut root;
        for (j, part) in parts.iter().enumerate() {
            if j == parts.len() - 1 {
                current.files.push(i);
            } else {
                current = current.folders.entry(part.to_string()).or_default();
            }
        }
    }
    root
}

/// Recursively sort the file tree so that files are in alphabetical order.
fn sort_file_tree(tree: &mut FileTree, files: &[FileItem]) {
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
fn get_folder_all_selected(tree: &FileTree, files: &[FileItem]) -> bool {
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
fn set_folder_selection(tree: &FileTree, files: &mut [FileItem], value: bool) {
    for &i in &tree.files {
        files[i].selected = value;
    }
    for (_, sub_tree) in &tree.folders {
        set_folder_selection(sub_tree, files, value);
    }
}

/// Our app state stores the file tree, text input, and other state.
struct MyApp {
    files: Vec<FileItem>,
    extra_text: String,
    ignore_set: GlobSet,
    generated_prompt: String,
    token_count: usize,
    current_folder: Option<PathBuf>,
    include_file_tree: bool, // Toggle inclusion of file tree.
}

impl MyApp {
    /// Refresh the file list based on the current folder.
    fn refresh_files(&mut self) {
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
            include_file_tree: true, // Now checked by default.
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
fn get_all_files_limited(base: &Path, limit: usize, ignore_set: &GlobSet) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut dirs = vec![base.to_path_buf()];
    while let Some(current_dir) = dirs.pop() {
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
fn show_file_tree(ui: &mut egui::Ui, tree: &FileTree, files: &mut [FileItem]) {
    for (folder_name, subtree) in &tree.folders {
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
            let old_spacing = ui.spacing().item_spacing;
            ui.spacing_mut().item_spacing.x = 0.5;
            let mut folder_selected = get_folder_all_selected(subtree, files);
            if ui.checkbox(&mut folder_selected, "").changed() {
                set_folder_selection(subtree, files, folder_selected);
            }
            ui.collapsing(folder_name, |ui| {
                show_file_tree(ui, subtree, files);
            });
            ui.spacing_mut().item_spacing = old_spacing;
        });
    }
    for &i in &tree.files {
        let file = &mut files[i];
        let file_name = file.rel_path.rsplit('/').next().unwrap_or(&file.rel_path);
        ui.checkbox(&mut file.selected, file_name);
    }
}

/// Generates a textual file tree from the loaded files.
fn generate_file_tree_string(files: &[FileItem], base: &Path) -> String {
    let mut tree = build_file_tree(files);
    sort_file_tree(&mut tree, files);
    let base_name = base
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("root"))
        .to_string_lossy()
        .to_string();
    let mut output = format!("{}/\n", base_name);
    output.push_str(&generate_tree_string(&tree, files, "".to_string()));
    output
}

/// Recursively converts a FileTree into a tree-formatted string.
/// Folders are now listed before files.
fn generate_tree_string(tree: &FileTree, files: &[FileItem], prefix: String) -> String {
    let mut output = String::new();
    let mut entries: Vec<(String, bool, Option<&FileTree>)> = Vec::new();
    for (folder, sub_tree) in &tree.folders {
        entries.push((folder.clone(), true, Some(sub_tree)));
    }
    for &file_index in &tree.files {
        let file_name = files[file_index]
            .rel_path
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_string();
        entries.push((file_name, false, None));
    }
    use std::cmp::Ordering;
    // Sort so that folders come before files, then alphabetically.
    entries.sort_by(|a, b| match (a.1, b.1) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => a.0.cmp(&b.0),
    });
    let total = entries.len();
    for (i, entry) in entries.into_iter().enumerate() {
        let is_last = i == total - 1;
        let connector = if is_last { "└─ " } else { "├─ " };
        output.push_str(&format!("{}{}{}\n", prefix, connector, entry.0));
        if entry.1 {
            if let Some(sub_tree) = entry.2 {
                let new_prefix = if is_last {
                    format!("{}    ", prefix)
                } else {
                    format!("{}│   ", prefix)
                };
                output.push_str(&generate_tree_string(sub_tree, files, new_prefix));
            }
        }
    }
    output
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        let mut visuals = ctx.style().visuals.clone();
        visuals.window_fill = egui::Color32::from_rgba_unmultiplied(0, 0, 0, 128);
        ctx.set_visuals(visuals);

        for file_item in &mut self.files {
            if file_item.selected && file_item.content.is_none() {
                file_item.content = fs::read_to_string(&file_item.path).ok();
            }
        }

        // Compute the base prompt from selected files and extra text.
        let base_prompt = compute_prompt(&self.files, &self.extra_text);
        // If the file tree is included, prepend its text.
        let final_prompt = if self.include_file_tree {
            let base = self.current_folder.as_deref().unwrap_or(Path::new("."));
            let tree_text = generate_file_tree_string(&self.files, base);
            format!("{}\n\n{}", tree_text, base_prompt)
        } else {
            base_prompt
        };
        // Compute token count (using an approximate 4 characters per token).
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
                                file_item.content = fs::read_to_string(&file_item.path).ok();
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

fn main() {
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
    let mut native_options = eframe::NativeOptions::default();
    native_options.initial_window_size = Some(egui::vec2(1000.0, 600.0));
    native_options.transparent = true;
    eframe::run_native(
        "Prompt Generator",
        native_options,
        Box::new(|_cc| Box::new(app)),
    );
}
