use crate::file_item::FileItem;
use std::collections::BTreeMap;

#[derive(Default)]
pub struct FileTree {
    pub folders: BTreeMap<String, FileTree>,
    pub files: Vec<usize>,
}

pub fn build_file_tree(files: &[FileItem]) -> FileTree {
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

pub fn sort_file_tree(tree: &mut FileTree, files: &[FileItem]) {
    tree.files.sort_by(|&a, &b| {
        let name_a = files[a].rel_path.rsplit('/').next().unwrap_or("");
        let name_b = files[b].rel_path.rsplit('/').next().unwrap_or("");
        name_a.cmp(name_b)
    });
    for (_, subtree) in tree.folders.iter_mut() {
        sort_file_tree(subtree, files);
    }
}

pub fn set_folder_selection(tree: &FileTree, files: &mut [FileItem], value: bool) {
    for &i in &tree.files {
        files[i].selected = value;
    }
    for sub_tree in tree.folders.values() {
        set_folder_selection(sub_tree, files, value);
    }
}

pub fn subtree_tokens(tree: &FileTree, files: &[FileItem]) -> usize {
    let mut sum = 0;
    for &i in &tree.files {
        sum += files[i].token_count;
    }
    for sub in tree.folders.values() {
        sum += subtree_tokens(sub, files);
    }
    sum
}

use egui::{CollapsingHeader, Color32, RichText};

pub fn show_file_tree(ui: &mut egui::Ui, tree: &FileTree, files: &mut [FileItem]) {
    for (folder_name, subtree) in &tree.folders {
        ui.horizontal(|ui| {
            let old_spacing = ui.spacing().item_spacing;
            ui.spacing_mut().item_spacing.x = 0.0;

            let (total, selected) = get_folder_selection_counts(subtree, files);
            let mut folder_selected = selected == total;
            let indeterminate = selected > 0 && selected < total;
            let mut cb = egui::Checkbox::new(&mut folder_selected, "").indeterminate(indeterminate);
            if ui.add(cb).changed() {
                set_folder_selection(subtree, files, folder_selected);
            }

            let total_tok = subtree_tokens(subtree, files);
            CollapsingHeader::new(
                RichText::new(format!("{} ({})", folder_name, total_tok))
                    .color(Color32::from_rgb(230, 200, 120)),
            )
            .id_salt(folder_name)
            .show(ui, |ui| {
                show_file_tree(ui, subtree, files);
            });

            ui.spacing_mut().item_spacing = old_spacing;
        });
    }

    for &i in &tree.files {
        let file = &mut files[i];
        let name = file.rel_path.rsplit('/').next().unwrap_or(&file.rel_path);
        let color = if name.ends_with(".rs") {
            Color32::from_rgb(250, 150, 150) // Rust
        } else if name.ends_with(".md") || name.ends_with(".txt") {
            Color32::from_rgb(100, 250, 100) // Markdown/Text
        } else if name.ends_with(".cu") || name.ends_with(".cuda") {
            Color32::from_rgb(100, 150, 250) // CUDA
        } else if name.ends_with(".o") {
            Color32::from_rgb(150, 150, 150) // Object files
        } else if name.ends_with(".py") {
            Color32::from_rgb(50, 100, 250) // Python
        } else if name.ends_with(".html") {
            Color32::from_rgb(250, 100, 50) // HTML
        } else if name.ends_with(".css") {
            Color32::from_rgb(150, 100, 250) // CSS
        } else if name.ends_with(".csv") {
            Color32::from_rgb(100, 250, 150) // CSV
        } else if name.ends_with(".slang") {
            Color32::from_rgb(250, 150, 50) // Slang
        } else if name.ends_with(".wgsl") {
            Color32::from_rgb(250, 100, 250) // WGSL
        } else if name.ends_with(".png")
            || name.ends_with(".exr")
            || name.ends_with(".hdr")
            || name.ends_with(".jpg")
            || name.ends_with(".jpeg")
        {
            Color32::from_rgb(250, 250, 100) // Images
        } else if name.ends_with(".gltf") || name.ends_with(".glb") {
            Color32::from_rgb(100, 250, 250) // GLTF/GLB
        } else if name.ends_with(".spv") || name.ends_with(".spvasm") {
            Color32::from_rgb(150, 100, 150) // SPIR-V
        } else if name.ends_with(".glsl") || name.ends_with(".comp") {
            Color32::from_rgb(100, 150, 250) // GLSL/Compute Shaders
        } else if name.ends_with(".sh") {
            Color32::from_rgb(150, 250, 100) // Shell scripts
        } else if name.ends_with(".lock") {
            Color32::from_rgb(250, 100, 100) // Lock files (same as Rust for consistency)
        } else if name.ends_with(".toml") {
            Color32::from_rgb(250, 150, 150) // TOML
        } else {
            ui.visuals().text_color()
        };
        let label = RichText::new(format!("{} ({})", name, file.token_count)).color(color);
        ui.checkbox(&mut file.selected, label);
    }
}

pub fn generate_file_tree_string(files: &[FileItem], base: &std::path::Path) -> String {
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

pub fn generate_tree_string(tree: &FileTree, files: &[FileItem], prefix: String) -> String {
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

pub fn get_folder_selection_counts(tree: &FileTree, files: &[FileItem]) -> (usize, usize) {
    let mut total = tree.files.len();
    let mut selected = tree.files.iter().filter(|&&i| files[i].selected).count();
    for sub_tree in tree.folders.values() {
        let (sub_total, sub_selected) = get_folder_selection_counts(sub_tree, files);
        total += sub_total;
        selected += sub_selected;
    }
    (total, selected)
}
