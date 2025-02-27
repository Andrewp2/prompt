use globset::{Glob, GlobSet, GlobSetBuilder};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// Maximum number of files to load.
pub const MAX_FILES: usize = 10_000;

/// Represents a file with its full path, relative path, selection state, and optional cached content.
#[derive(Clone)]
pub struct FileItem {
    pub path: PathBuf,
    pub rel_path: String,
    pub selected: bool,
    pub content: Option<String>,
}

/// Loads ignore patterns from a file named ".promptignore".
pub fn load_ignore_set() -> GlobSet {
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

/// Walks the directory tree starting at `base`, applying ignore rules.
pub fn get_all_files_limited(base: &Path, limit: usize, ignore_set: &GlobSet) -> Vec<PathBuf> {
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
