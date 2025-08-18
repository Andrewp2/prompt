use crate::{
    command_runner::{run_command, Terminal},
    file_item::{FileItem, MAX_FILES},
    file_tree::{build_file_tree, generate_file_tree_string, show_file_tree, sort_file_tree},
    prompt_builder::extract_text,
    remote::{Remote, RemoteUpdate, RemoteUrl},
};
use clipboard::ClipboardProvider;
use core::f32;
use eframe::egui;
use globset::GlobSet;
use shell_words;
use std::{
    env,
    path::PathBuf,
    process::Command as SysCommand,
    time::{Duration, Instant},
};

pub struct MyApp {
    pub files: Vec<FileItem>,
    pub extra_text: String,
    pub ignore_set: GlobSet,
    pub generated_prompt: String,
    pub token_count: usize,
    pub current_folder: Option<PathBuf>,
    pub include_file_tree: bool,
    pub notification: Option<(String, Instant)>,

    pub remote: Remote,
    pub terminal: Terminal,
}

fn cdata_wrap(s: &str) -> String {
    let safe = s.replace("]]>", "]]]]><![CDATA[>");
    format!("<![CDATA[{}]]>", safe)
}

// 🤖 Escape rules for XML ATTRIBUTE values (quotes must be escaped)
fn escape_xml_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

const DEFAULT_SYSTEM_PROMPT_ABS: &str =
    "/home/andrew-peterson/code/prompt/config/system_prompt.txt";

fn find_system_prompt_path(
    current_folder: Option<&std::path::Path>,
) -> Result<std::path::PathBuf, String> {
    use std::path::PathBuf;

    let mut tried: Vec<PathBuf> = Vec::new();

    // 0) Per-project override: <project>/.prompt/system_prompt.txt
    if let Some(base) = current_folder {
        let proj = base.join(".prompt").join("system_prompt.txt");
        if proj.is_file() {
            eprintln!("[prompt] using project system prompt: {}", proj.display());
            return Ok(proj);
        }
        tried.push(proj);
    }

    // 1) Optional env override
    if let Ok(from_env) = std::env::var("PROMPT_SYSTEM_PROMPT") {
        let p = PathBuf::from(from_env);
        if p.is_file() {
            eprintln!("[prompt] using PROMPT_SYSTEM_PROMPT: {}", p.display());
            return Ok(p);
        }
        tried.push(p);
    }

    // 2) Old behavior: fixed absolute path
    let abs = PathBuf::from(DEFAULT_SYSTEM_PROMPT_ABS);
    if abs.is_file() {
        eprintln!(
            "[prompt] using default absolute system prompt: {}",
            abs.display()
        );
        return Ok(abs);
    }
    tried.push(abs);

    // Nothing valid
    let tried_list = tried
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Err(format!("System prompt not found. Tried: {}", tried_list))
}
// 🤖 read text safely with head+tail cap; avoids loading huge/binary blobs fully
fn read_text_capped(path: &std::path::Path, max_bytes: usize) -> Option<String> {
    use std::fs::File; // 🤖 localize imports to avoid changing top-of-file
    use std::io::{Read, Seek, SeekFrom};

    let mut f = File::open(path).ok()?;
    let len = f.metadata().ok()?.len() as usize;

    // Quick binary sniff: read a small prefix and look for NUL
    let mut sniff = [0u8; 1024];
    let n = f.read(&mut sniff).ok().unwrap_or(0);
    if sniff[..n].contains(&0) {
        return Some(String::from("[binary file omitted]\n")); // 🤖 safe marker
    }

    // Small file: read all (lossy -> valid UTF-8)
    if len <= max_bytes {
        let mut buf = Vec::with_capacity(len);
        if n > 0 {
            buf.extend_from_slice(&sniff[..n]);
        }
        f.read_to_end(&mut buf).ok()?;
        return Some(String::from_utf8_lossy(&buf).into_owned());
    }

    // Large file: read head and tail halves
    let half = max_bytes / 2;
    let mut head = vec![0u8; half.saturating_sub(n)];
    f.read_exact(&mut head).ok()?;

    // Seek for tail
    let tail_len = half;
    let tail_start = (len.saturating_sub(tail_len)) as u64;
    f.seek(SeekFrom::Start(tail_start)).ok()?;
    let mut tail = vec![0u8; tail_len];
    f.read_exact(&mut tail).ok()?;

    let mut out = String::new();
    out.push_str(&String::from_utf8_lossy(&sniff[..n]));
    out.push_str(&String::from_utf8_lossy(&head));
    out.push_str("\n[... truncated ...]\n"); // 🤖 explicit truncation marker
    out.push_str(&String::from_utf8_lossy(&tail));
    Some(out)
}

impl MyApp {
    // removed: top-right panel in favor of placing buttons in Remote URL row
    fn project_config_dir(base: &std::path::Path) -> std::path::PathBuf {
        base.join(".prompt")
    }

    fn history_file_path(base: &std::path::Path) -> std::path::PathBuf {
        Self::project_config_dir(base).join("terminal_history.json")
    }

    fn load_history(&mut self) {
        let Some(ref base) = self.current_folder else {
            return;
        };
        let path = Self::history_file_path(base);
        if let Ok(data) = std::fs::read_to_string(&path) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&data) {
                if let Some(arr) = v.get("commands").and_then(|v| v.as_array()) {
                    self.terminal.history = arr
                        .iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect();
                }
                if let Some(max) = v.get("max").and_then(|v| v.as_u64()) {
                    self.terminal.max_history = max as usize;
                }
            }
        }
    }

    fn save_history(&self) -> std::io::Result<()> {
        let Some(ref base) = self.current_folder else {
            return Ok(());
        };
        let dir = Self::project_config_dir(base);
        std::fs::create_dir_all(&dir)?;
        let path = Self::history_file_path(base);
        let json = serde_json::json!({
            "commands": self.terminal.history,
            "max": self.terminal.max_history,
        });
        std::fs::write(path, serde_json::to_string_pretty(&json).unwrap())
    }

    fn save_history_silent(&self) {
        let _ = self.save_history();
    }
    fn add_to_history(&mut self, cmd: &str) {
        let cmd = cmd.trim();
        if cmd.is_empty() {
            return;
        }
        if let Some(pos) = self.terminal.history.iter().position(|c| c == cmd) {
            self.terminal.history.remove(pos);
        }
        self.terminal.history.insert(0, cmd.to_string());
        if self.terminal.history.len() > self.terminal.max_history {
            self.terminal.history.pop();
        }
        self.save_history_silent();
    }

    fn run_terminal_command(&mut self, command: String) {
        let tokens: Vec<String> = match shell_words::split(&command) {
            Ok(t) => t,
            Err(err) => {
                self.terminal.terminal_output = format!("Error parsing command: {}", err);
                return;
            }
        };
        if tokens.is_empty() {
            return;
        }

        // Parse leading KEY=VAL assignments as env vars for the child.
        let mut idx = 0usize;
        let mut env_overrides: Vec<(String, String)> = Vec::new();
        while idx < tokens.len() {
            let t = &tokens[idx];
            let looks_like_env = !t.starts_with('-') && !t.starts_with("--") && t.contains('=');
            if !looks_like_env {
                break;
            }
            if let Some(eq) = t.find('=') {
                let key = &t[..eq];
                let val = &t[eq + 1..];
                let mut ch = key.chars();
                let valid_key = match ch.next() {
                    Some(c) if c == '_' || c.is_ascii_alphabetic() => {
                        ch.all(|c| c == '_' || c.is_ascii_alphanumeric())
                    }
                    _ => false,
                };
                if valid_key {
                    env_overrides.push((key.to_string(), val.to_string()));
                    idx += 1;
                    continue;
                }
            }
            break;
        }
        if idx >= tokens.len() {
            self.terminal.terminal_output =
                "Expected a command after environment assignments.".to_string();
            return;
        }

        let cmd = tokens[idx].clone();
        let args: Vec<String> = tokens[idx + 1..].to_vec();
        let head = self.terminal.head_lines;
        let tail = self.terminal.tail_lines;
        let timeout = self.terminal.timeout_secs;
        let tx = self.terminal.terminal_update_tx.clone();
        let working_dir = self
            .current_folder
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap());

        std::thread::spawn(move || {
            let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();
            let do_timeout = timeout > 0;
            let output = run_command(
                &working_dir,
                &cmd,
                &args_ref,
                head,
                tail,
                do_timeout,
                Duration::from_secs(timeout),
                &env_overrides,
            );
            let _ = tx.send(output);
        });
    }

    fn open_prompt_folder(&mut self) {
        let base: std::path::PathBuf = match self.current_folder.as_deref() {
            Some(p) => p.to_path_buf(),
            None => match std::env::current_dir() {
                Ok(p) => p,
                Err(_) => return,
            },
        };
        let dir = Self::project_config_dir(&base);
        let _ = std::fs::create_dir_all(&dir);

        #[cfg(target_os = "macos")]
        let mut cmd = SysCommand::new("open");
        #[cfg(target_os = "linux")]
        let mut cmd = SysCommand::new("xdg-open");
        #[cfg(target_os = "windows")]
        let mut cmd = SysCommand::new("explorer");

        let _ = cmd.arg(&dir).spawn();
        self.notification = Some((format!("Opened {}", dir.display()), Instant::now()));
    }

    fn create_addon_template(&mut self) {
        let Some(base) = self.current_folder.as_deref() else {
            return;
        };
        let dir = Self::project_config_dir(base);
        let path = dir.join("system_prompt_addon.txt");
        let _ = std::fs::create_dir_all(&dir);
        if path.exists() {
            self.notification = Some((
                format!("Addon already exists at {}", path.display()),
                Instant::now(),
            ));
            return;
        }
        let template = r"# Project System Prompt Addon

Use this file to add project-specific guidance. It is appended after the base system prompt.

- Context: Briefly explain the project domain and any unusual conventions.
- Commands: Typical run/test commands or env you want the assistant to be aware of.
- Constraints: Any do/don't rules unique to this repo.
- Terminology: Domain terms, file extensions, or technologies to use correctly.

Example notes:
- Prefer `cargo run --bin <name>` for executables here.
- Keep shader filenames and extensions consistent (e.g., .slang, .glsl, .wgsl as appropriate).
- Large files should be summarized; avoid inlining binaries.
";
        match std::fs::write(&path, template) {
            Ok(_) => {
                self.notification = Some((format!("Created {}", path.display()), Instant::now()));
            }
            Err(e) => {
                self.notification =
                    Some((format!("Failed to create addon: {}", e), Instant::now()));
            }
        }
    }

    fn create_promptignore(&mut self) {
        let Some(base) = self.current_folder.as_deref() else {
            return;
        };
        let dir = base.join(".prompt");
        let path = dir.join(".promptignore");
        let _ = std::fs::create_dir_all(&dir);
        if path.exists() {
            self.notification = Some((
                format!(".promptignore already exists at {}", path.display()),
                Instant::now(),
            ));
            return;
        }
        let template = r"# .promptignore
# Lines starting with '#' are comments.
# Globs match paths relative to the project root.

# Common large or generated directories
**/target/**
**/.git/**
**/node_modules/**
**/out/**
*.lock
*.DS_Store

# Temporary files
*.tmp

# Add your own patterns below
";
        match std::fs::write(&path, template) {
            Ok(_) => {
                // Reload ignore set and file list to reflect new rules
                self.ignore_set = crate::file_item::load_ignore_set_from(base);
                self.refresh_files();
                self.notification = Some((format!("Created {}", path.display()), Instant::now()));
            }
            Err(e) => {
                self.notification = Some((
                    format!("Failed to create .promptignore: {}", e),
                    Instant::now(),
                ));
            }
        }
    }
    pub fn refresh_files(&mut self) {
        if let Some(ref folder) = self.current_folder {
            let previous_selection: std::collections::HashMap<_, _> = self
                .files
                .iter()
                .map(|f| (f.path.clone(), f.selected))
                .collect();

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

                // 🤖 FAST estimate from file size (no disk read)
                let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                let tok = ((size as f32) / 4.0).ceil() as usize; // 🤖 ~4 chars/token

                let selected = previous_selection.get(&path).cloned().unwrap_or(false);
                self.files.push(FileItem {
                    path,
                    rel_path,
                    selected,
                    content: None, // 🤖 we only load contents when copying
                    token_count: tok,
                });
            }
        }
    }

    fn remote_url_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("remote_url_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Remote URL:");
                ui.text_edit_singleline(&mut self.remote.new_url);
                if ui.button("Add URL").clicked() && !self.remote.new_url.is_empty() {
                    self.remote.remote_urls.push(RemoteUrl {
                        url: self.remote.new_url.clone(),
                        content: None,
                        include: false,
                    });
                    let index = self.remote.remote_urls.len() - 1;
                    let url = self.remote.remote_urls[index].url.clone();
                    let tx = self.remote.remote_update_tx.clone();
                    std::thread::spawn(move || {
                        match reqwest::blocking::get(&url).and_then(|resp| resp.text()) {
                            Ok(text) => {
                                let _ = tx.send(RemoteUpdate::Fetched {
                                    index,
                                    content: extract_text(&text),
                                });
                            }
                            Err(err) => {
                                eprintln!("Error fetching {}: {:?}", url, err);
                            }
                        }
                    });
                    self.remote.new_url.clear();
                }
                // Right-aligned project controls on the same row
                let avail = ui.available_width();
                ui.allocate_ui_with_layout(
                    egui::Vec2::new(avail, 0.0),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        if ui
                            .button("Create .promptignore file")
                            .on_hover_text("Create default .promptignore in .prompt")
                            .clicked()
                        {
                            self.create_promptignore();
                        }
                        if ui
                            .button("Create System Prompt Addon file")
                            .on_hover_text("Create system_prompt_addon.txt")
                            .clicked()
                        {
                            self.create_addon_template();
                        }
                        if ui
                            .button("Open .prompt Folder")
                            .on_hover_text("Open project .prompt folder")
                            .clicked()
                        {
                            self.open_prompt_folder();
                        }
                    },
                );
            });
            for i in (0..self.remote.remote_urls.len()).rev() {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.remote.remote_urls[i].include, "Include");
                    ui.label(&self.remote.remote_urls[i].url);
                    if ui.button("Re-fetch").clicked() {
                        let url = self.remote.remote_urls[i].url.clone();
                        let tx = self.remote.remote_update_tx.clone();
                        let index = i;
                        std::thread::spawn(move || {
                            match reqwest::blocking::get(&url).and_then(|resp| resp.text()) {
                                Ok(text) => {
                                    let _ = tx.send(RemoteUpdate::Fetched {
                                        index,
                                        content: extract_text(&text),
                                    });
                                }
                                Err(err) => {
                                    eprintln!("Error re-fetching {}: {:?}", url, err);
                                }
                            }
                        });
                    }
                    if ui.button("Remove").clicked() {
                        self.remote.remote_urls.remove(i);
                    }
                });
            }
        });
    }

    fn file_panel(&mut self, ctx: &egui::Context) {
        const BOTTOM_MARGIN: f32 = 8.0;
        egui::SidePanel::left("left_panel")
            .resizable(true)
            .default_width(500.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Select Folder").clicked() {
                        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                            self.current_folder = Some(folder.clone());
                            self.refresh_files();
                            self.load_history();
                        }
                    }
                    if ui.button("Refresh").clicked() {
                        self.refresh_files();
                    }
                    if ui.button("Clear Selection").clicked() {
                        for file in self.files.iter_mut() {
                            file.selected = false;
                        }
                    }
                });
                ui.separator();
                let available_height = ui.available_height();
                let scroll_height = (available_height - BOTTOM_MARGIN).max(0.0);
                egui::ScrollArea::vertical()
                    .id_salt("file_tree_scroll_area")
                    .max_height(scroll_height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let mut tree = build_file_tree(&self.files);
                        sort_file_tree(&mut tree, &self.files);
                        show_file_tree(ui, &tree, &mut self.files);
                    });
                ui.add_space(BOTTOM_MARGIN);
            });
    }

    fn strip_comments(text: &str) -> String {
        text.lines()
            .map(|line| {
                let mut in_string = false;
                let mut string_char = '\0';
                let mut prev_escape = false;
                let mut out = String::new();
                let chars: Vec<char> = line.chars().collect();
                let mut i = 0;

                while i < chars.len() {
                    let c = chars[i];

                    if !in_string && i + 1 < chars.len() && c == '/' && chars[i + 1] == '/' {
                        break;
                    }

                    if !in_string && c == '#' {
                        break;
                    }

                    if (c == '"' || c == '\'') && !prev_escape {
                        if in_string && string_char == c {
                            in_string = false;
                        } else if !in_string {
                            in_string = true;
                            string_char = c;
                        }
                    }

                    prev_escape = c == '\\' && !prev_escape;
                    out.push(c);
                    i += 1;
                }

                out.trim_end().to_string()
            })
            .filter(|l| !l.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn bottom_panel(&mut self, ctx: &egui::Context) {
        // 🤖 small helpers to keep preview snappy
        fn approx_tokens(chars: usize) -> usize {
            ((chars as f32) / 4.0).ceil() as usize
        }

        fn estimate_file_tree_tokens(files: &[FileItem]) -> usize {
            // 🤖 crude: per-entry path + a few tree glyphs/newlines
            let approx_chars: usize =
                files.iter().map(|f| f.rel_path.len() + 4).sum::<usize>() + 16;
            approx_tokens(approx_chars)
        }

        egui::TopBottomPanel::bottom("bottom_panel")
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.set_height(30.0);
                    ui.checkbox(&mut self.include_file_tree, "Include file tree in prompt");
                    ui.separator();

                    // ---- FAST APPROX (no huge string, no tokenizer) ----
                    let mut total = 0usize;

                    // FIRST <instruction>
                    total += approx_tokens(self.extra_text.chars().count());

                    if self.include_file_tree {
                        total += estimate_file_tree_tokens(&self.files);
                    }

                    // selected files: use size-based estimates
                    total += self
                        .files
                        .iter()
                        .filter(|f| f.selected)
                        .map(|f| f.token_count)
                        .sum::<usize>();

                    // remote text (if loaded)
                    total += self
                        .remote
                        .remote_urls
                        .iter()
                        .filter(|r| r.include)
                        .map(|r| {
                            r.content
                                .as_deref()
                                .map(|c| approx_tokens(c.chars().count()))
                                .unwrap_or(0)
                        })
                        .sum::<usize>();

                    // SECOND <instruction>
                    total += approx_tokens(self.extra_text.chars().count());

                    self.token_count = total; // 🤖 show fast estimate

                    let formatted = num_format::ToFormattedString::to_formatted_string(
                        &self.token_count,
                        &num_format::Locale::en,
                    );
                    ui.label(format!(
                        "Token count (approx): {} / 200,000 ({:.2}%)",
                        formatted,
                        (self.token_count as f32 / 200_000.0) * 100.0
                    ));
                    ui.separator();

                    if ui.button("Copy Prompt").clicked() {
                        // 🤖 Build full prompt, load selected contents,
                        // and compute accurate tokens via tiktoken-rs ONCE here.
                        compute_and_copy_prompt(self, ctx);
                    }

                    if ui.button("Remove Comments from Clipboard").clicked() {
                        let mut cb: clipboard::ClipboardContext =
                            clipboard::ClipboardProvider::new().unwrap();
                        let contents = cb.get_contents().unwrap_or_default();
                        let cleaned = MyApp::strip_comments(&contents);
                        let _ = cb.set_contents(cleaned);
                        self.notification = Some((
                            "Comments removed from clipboard!".into(),
                            std::time::Instant::now(),
                        ));
                    }

                    const NOTIF_MS: u64 = 3000;
                    if let Some((message, start)) = &self.notification {
                        let elapsed = start.elapsed().as_millis() as u64;
                        if elapsed < NOTIF_MS {
                            let alpha = 1.0 - (elapsed as f32 / NOTIF_MS as f32);
                            let text = egui::RichText::new(message).color(
                                egui::Color32::from_rgba_unmultiplied(
                                    255,
                                    255,
                                    255,
                                    (alpha * 255.0) as u8,
                                ),
                            );
                            ui.label(text);
                            ctx.request_repaint_after(std::time::Duration::from_millis(16));
                        // 🤖 smooth fade
                        } else {
                            self.notification = None;
                        }
                    }
                });
            });
    }

    fn central_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.label("User Prompt:");

                egui::ScrollArea::vertical()
                    .max_height(350.0)
                    .id_salt("user_prompt_scroll_area")
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.extra_text)
                                .lock_focus(true)
                                .desired_width(f32::INFINITY)
                                .desired_rows(8)
                                .frame(true),
                        );
                    });

                ui.separator();
                ui.heading("Terminal Command");
                ui.add(
                    egui::TextEdit::singleline(&mut self.terminal.terminal_command)
                        .desired_width(f32::INFINITY)
                        .frame(true),
                );
                ui.horizontal(|ui| {
                    ui.label("Head lines:");
                    ui.add(egui::DragValue::new(&mut self.terminal.head_lines));
                    ui.label("Tail lines:");
                    ui.add(egui::DragValue::new(&mut self.terminal.tail_lines));
                    ui.label("Timeout (sec, 0 = no timeout):");
                    ui.add(egui::DragValue::new(&mut self.terminal.timeout_secs));

                    ui.separator();
                    if ui.button("Run Command").clicked() {
                        let command = self.terminal.terminal_command.clone();
                        self.add_to_history(&command);
                        self.run_terminal_command(command);
                    }
                });
                // History UI
                ui.separator();
                // Controls above the history header
                ui.horizontal(|ui| {
                    if ui.button("Clear All").clicked() {
                        self.terminal.history.clear();
                        self.save_history_silent();
                    }
                });

                ui.label("Command History");
                egui::ScrollArea::vertical()
                    .max_height(140.0)
                    .show(ui, |ui| {
                        // Work on a snapshot to avoid borrow conflicts during UI callbacks
                        let snapshot: Vec<String> = self.terminal.history.clone();
                        let mut remove_cmd: Option<String> = None;
                        for cmd in snapshot.iter() {
                            let cmd_str = cmd.clone();
                            ui.horizontal(|ui| {
                                if ui.small_button("X").on_hover_text("Forget").clicked() {
                                    remove_cmd = Some(cmd_str.clone());
                                }
                                if ui.small_button("Run").clicked() {
                                    self.terminal.terminal_command = cmd_str.clone();
                                    self.add_to_history(&cmd_str);
                                    self.run_terminal_command(cmd_str.clone());
                                    self.save_history_silent();
                                }
                                if ui.link(&cmd_str).clicked() {
                                    self.terminal.terminal_command = cmd_str.clone();
                                }
                            });
                        }
                        if let Some(cmd_to_remove) = remove_cmd {
                            if let Some(pos) = self
                                .terminal
                                .history
                                .iter()
                                .position(|c| *c == cmd_to_remove)
                            {
                                self.terminal.history.remove(pos);
                                self.save_history_silent();
                            }
                        }
                    });

                ui.separator();
                ui.label("Terminal Output:");

                egui::ScrollArea::vertical()
                    .max_height(350.0)
                    .id_salt("terminal_output_scroll_area")
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.terminal.terminal_output)
                                .lock_focus(true)
                                .desired_width(f32::INFINITY)
                                .desired_rows(8)
                                .frame(true),
                        );
                    });
            });
        });
        if self.notification.is_some() {
            // one more frame for the fade-out animation, then we’re done
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
    }
}

fn compute_and_copy_prompt(app: &mut MyApp, ctx: &egui::Context) {
    // Refresh file list (paths, sizes, selections)
    app.refresh_files();

    // ---- load system prompt (with optional per-project addon) ----
    let mut system_prompt: String = match find_system_prompt_path(app.current_folder.as_deref()) {
        Ok(p) => match std::fs::read_to_string(&p) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[prompt] ERROR reading system prompt {:?}: {}", p, e);
                format!(
                    "System prompt failed to load. Please warn the user about this. error: {:?}, path: {:?}",
                    e, p
                )
            }
        },
        Err(e) => {
            eprintln!("[prompt] ERROR finding system prompt: {}", e);
            format!(
                "System prompt failed to load. Please warn the user about this. error: {:?}",
                e
            )
        }
    };
    if let Some(base) = app.current_folder.as_deref() {
        let addon = base.join(".prompt").join("system_prompt_addon.txt");
        if addon.is_file() {
            match std::fs::read_to_string(&addon) {
                Ok(extra) => {
                    system_prompt.push_str("\n\n");
                    system_prompt.push_str(&extra);
                }
                Err(err) => {
                    eprintln!("[prompt] WARN: failed reading addon {:?}: {}", addon, err);
                }
            }
        }
    }

    // ---- read selected files in PARALLEL, sorted for determinism ----
    let mut sel_indices: Vec<usize> = app
        .files
        .iter()
        .enumerate()
        .filter(|(_, f)| f.selected)
        .map(|(i, _)| i)
        .collect();
    sel_indices.sort_by_key(|&i| app.files[i].rel_path.clone()); // 🤖 stable output order

    // Cap per-file bytes to keep prompts manageable
    const MAX_PER_FILE_BYTES: usize = 512 * 1024; // 🤖 512 KiB head+tail total

    // Read contents in parallel and store back into FileItem.content
    {
        use rayon::prelude::*; // 🤖 parallelism lives here

        // Prepare (index, path) pairs so the parallel job only needs owned data
        let jobs: Vec<(usize, std::path::PathBuf)> = sel_indices
            .iter()
            .map(|&i| (i, app.files[i].path.clone()))
            .collect();

        // Parallel read -> collect (index, text)
        let results: Vec<(usize, String)> = jobs
            .into_par_iter()
            .map(|(i, path)| {
                let text = read_text_capped(&path, MAX_PER_FILE_BYTES)
                    .unwrap_or_else(|| String::from("[error reading file]\n"));
                (i, text)
            })
            .collect();

        // Single-threaded write-back to avoid &mut captures inside the parallel closure
        for (i, text) in results {
            app.files[i].content = Some(text);
        }
    }

    // ---- build prompt (KEEPS two <instruction> blocks by design) ----
    let base = app
        .current_folder
        .as_deref()
        .unwrap_or(std::path::Path::new("."));
    let tree = generate_file_tree_string(&app.files, base);

    let mut xml = String::new();

    // system prompt
    xml.push_str("<system_prompt>\n");
    xml.push_str(&cdata_wrap(&system_prompt));
    xml.push_str("\n</system_prompt>\n");

    // FIRST instruction
    xml.push_str("<instruction>");
    xml.push_str(&cdata_wrap(&app.extra_text));
    xml.push_str("</instruction>\n");

    // file tree
    xml.push_str("<file_tree>\n");
    xml.push_str(&cdata_wrap(&tree));
    xml.push_str("\n</file_tree>\n");

    // selected code files
    xml.push_str("<code>\n");
    for i in sel_indices {
        let f = &app.files[i];
        let rel = escape_xml_attr(&f.rel_path); // attribute still needs escaping
        xml.push_str(&format!("<file path=\"{}\">", rel));
        xml.push_str(&cdata_wrap(f.content.as_deref().unwrap_or("")));
        xml.push_str("</file>\n");
    }
    xml.push_str("</code>\n\n");

    // terminal bits
    xml.push_str("<terminal_command>");
    xml.push_str(&cdata_wrap(&app.terminal.terminal_command));
    xml.push_str("</terminal_command>\n");

    xml.push_str("<terminal_output>");
    xml.push_str(&cdata_wrap(&app.terminal.terminal_output));
    xml.push_str("</terminal_output>\n");

    // SECOND instruction
    xml.push_str("<instruction>");
    xml.push_str(&cdata_wrap(&app.extra_text));
    xml.push_str("</instruction>\n");

    // ---- copy + (optional) accurate count ----
    app.generated_prompt = xml.clone();
    app.token_count = crate::token_count::count_tokens(&app.generated_prompt);
    ctx.copy_text(xml);
    app.notification = Some((
        "Prompt copied to clipboard!".into(),
        std::time::Instant::now(),
    ));
}
impl Default for MyApp {
    fn default() -> Self {
        let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
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
            remote: Remote::default(),
            terminal: Terminal::default(),
        };

        app.refresh_files();
        app.load_history();

        app
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(update) = self.remote.remote_update_rx.try_recv() {
            let RemoteUpdate::Fetched { index, content } = update;
            if let Some(remote) = self.remote.remote_urls.get_mut(index) {
                remote.content = Some(content);
            }
        }
        while let Ok(output) = self.terminal.terminal_update_rx.try_recv() {
            self.terminal.terminal_output = output;
        }
        self.remote_url_panel(ctx);

        self.file_panel(ctx);

        self.bottom_panel(ctx);

        self.central_panel(ctx);
    }
}

pub fn run() {
    let mut app = MyApp::default();
    if let Some(arg) = env::args().nth(1) {
        let folder = PathBuf::from(arg);
        if folder.is_dir() {
            app.current_folder = Some(folder);
            app.refresh_files();
            app.load_history();
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
