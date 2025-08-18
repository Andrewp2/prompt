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
        // Prefer new location: .prompt/.promptignore
        let candidate_new = current.join(".prompt").join(".promptignore");
        if candidate_new.exists() {
            eprintln!("Found .prompt/.promptignore at {:?}", candidate_new);
            return Some(candidate_new);
        }

        // Fallback legacy location: ./.promptignore
        let candidate_legacy = current.join(".promptignore");
        if candidate_legacy.exists() {
            eprintln!("Found legacy .promptignore at {:?}", candidate_legacy);
            return Some(candidate_legacy);
        }

        match current.parent() {
            Some(parent) => current = parent,
            None => break,
        }
    }
    None
}

pub fn load_ignore_set_from(base: &Path) -> GlobSet {
    let ignore_path =
        find_ignore_file(base).unwrap_or_else(|| base.join(".prompt").join(".promptignore"));
    eprintln!("Loading ignore patterns from {:?}", ignore_path);
    let mut builder = GlobSetBuilder::new();
    if let Ok(contents) = fs::read_to_string(ignore_path) {
        for line in contents.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let mut patterns: Vec<String> = Vec::new();

            if trimmed.ends_with('/') {
                // Directory pattern: ignore the dir itself and all its contents at any depth
                let d = trimmed.trim_end_matches('/');
                if !d.is_empty() {
                    patterns.push(format!("**/{}", d));
                    patterns.push(format!("**/{}/**", d));
                }
            } else if trimmed.contains('/') {
                // Path pattern (may include globs): match at any depth
                patterns.push(format!("**/{}", trimmed));
            } else {
                // Basename pattern
                let has_glob = trimmed.chars().any(|c| matches!(c, '*' | '?' | '['));
                if has_glob {
                    // e.g., *.tmp -> **/*.tmp
                    patterns.push(format!("**/{}", trimmed));
                } else {
                    // Name without globs: ignore file or directory with this name anywhere
                    patterns.push(format!("**/{}", trimmed));
                    patterns.push(format!("**/{}/**", trimmed));
                }
            }

            for pat in patterns {
                if let Ok(glob) = Glob::new(&pat) {
                    builder.add(glob);
                }
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

pub fn get_all_files_limited(
    base: &Path,
    limit: usize,
    ignore_set: &GlobSet,
) -> (Vec<PathBuf>, usize, usize, usize, usize) {
    let mut files = Vec::new();
    let mut scanned_files: usize = 0; // file entries visited (not counting pruned subtrees)
    let mut ignored_files: usize = 0; // files ignored by patterns
    let mut ignored_dirs: usize = 0; // directories ignored (each counts recursively skipped subtree)
    let mut symlinks_skipped: usize = 0; // symlink files/dirs skipped
    let mut dirs = vec![base.to_path_buf()];
    while let Some(current_dir) = dirs.pop() {
        let rel_dir = current_dir.strip_prefix(base).unwrap_or(&current_dir);
        if ignore_set.is_match(rel_dir.to_string_lossy().as_ref()) {
            ignored_dirs += 1; // this whole subtree is pruned
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

                // Use DirEntry::file_type to avoid following symlinks
                let ft = match entry.file_type() {
                    Ok(t) => t,
                    Err(_) => continue,
                };

                // Skip symlinks entirely to avoid cycles/explosions
                if ft.is_symlink() {
                    symlinks_skipped += 1;
                    continue;
                }

                if ft.is_file() {
                    scanned_files += 1;
                    if ignore_set.is_match(rel_path_str.as_ref()) {
                        ignored_files += 1;
                        continue;
                    }
                    files.push(path);
                    if files.len() >= limit {
                        break;
                    }
                } else if ft.is_dir() {
                    if ignore_set.is_match(rel_path_str.as_ref()) {
                        ignored_dirs += 1; // prune this subtree
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
    (files, scanned_files, ignored_files, ignored_dirs, symlinks_skipped)
}
