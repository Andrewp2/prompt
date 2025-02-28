use crate::file_item::FileItem;
use std::collections::BTreeMap;

/// A file tree structure for grouping files by folder.
#[derive(Default)]
pub struct FileTree {
    pub folders: BTreeMap<String, FileTree>,
    pub files: Vec<usize>,
}

/// Recursively builds a file tree from the list of file items.
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

/// Recursively sort the file tree so that files are in alphabetical order.
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

/// Recursively sets the selection state for all files in this tree.
pub fn set_folder_selection(tree: &FileTree, files: &mut [FileItem], value: bool) {
    for &i in &tree.files {
        files[i].selected = value;
    }
    for (_, sub_tree) in &tree.folders {
        set_folder_selection(sub_tree, files, value);
    }
}

pub fn show_file_tree(ui: &mut egui::Ui, tree: &FileTree, files: &mut [FileItem]) {
    for (folder_name, subtree) in &tree.folders {
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
            let old_spacing = ui.spacing().item_spacing;
            ui.spacing_mut().item_spacing.x = 0.5;

            // Get counts and determine the state.
            let (total, selected) = get_folder_selection_counts(subtree, files);
            let is_indeterminate = selected > 0 && selected < total;
            // The underlying boolean: true only if all files are selected.
            let mut folder_selected = selected == total;

            // Build the checkbox and set its indeterminate state.
            let mut checkbox = egui::Checkbox::new(&mut folder_selected, "");
            if is_indeterminate {
                checkbox = checkbox.indeterminate(true);
            }
            if ui.add(checkbox).changed() {
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

/// Recursively converts a FileTree into a tree-formatted string.
/// Folders are listed before files.
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

/// Returns (total number of files, number of selected files) in the tree.
pub fn get_folder_selection_counts(tree: &FileTree, files: &[FileItem]) -> (usize, usize) {
    let mut total = tree.files.len();
    let mut selected = tree.files.iter().filter(|&&i| files[i].selected).count();
    for (_, sub_tree) in &tree.folders {
        let (sub_total, sub_selected) = get_folder_selection_counts(sub_tree, files);
        total += sub_total;
        selected += sub_selected;
    }
    (total, selected)
}
