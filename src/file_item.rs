use globset::{Glob, GlobSet, GlobSetBuilder};
use std::fs;
use std::path::{Path, PathBuf};

pub const MAX_FILES: usize = 10_000;

#[derive(Clone)]
pub struct FileItem {
    pub path: PathBuf,
    pub rel_path: String,
    pub selected: bool,
    pub content: Option<String>,
    pub token_count: usize,
}

pub fn find_ignore_file(start: &Path) -> Option<PathBuf> {
    let mut current = start;
    loop {
        let candidate = current.join(".promptignore");
        if candidate.exists() {
            eprintln!("Found .promptignore at {:?}", candidate);
            return Some(candidate);
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => break,
        }
    }
    None
}

pub fn load_ignore_set_from(base: &Path) -> GlobSet {
    let ignore_path = find_ignore_file(base).unwrap_or_else(|| base.join(".promptignore"));
    eprintln!("Loading ignore patterns from {:?}", ignore_path);
    let mut builder = GlobSetBuilder::new();
    if let Ok(contents) = fs::read_to_string(ignore_path) {
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
    let gs = builder.build().unwrap();
    eprintln!("Loaded {} ignore patterns.", gs.len());
    gs
}

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
