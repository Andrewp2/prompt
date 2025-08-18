// ... a couple lines above
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::time::Duration;

pub struct Terminal {
    pub terminal_command: String,
    pub head_lines: usize,
    pub tail_lines: usize,
    pub timeout_secs: u64,
    pub terminal_output: String,
    pub terminal_update_rx: mpsc::Receiver<String>,
    pub terminal_update_tx: mpsc::Sender<String>,
    pub history: Vec<String>,
    pub max_history: usize,
    pub is_running: bool,
}

impl Default for Terminal {
    fn default() -> Self {
        let (term_tx, term_rx) = mpsc::channel();
        Self {
            terminal_command: String::new(),
            head_lines: 1000,
            tail_lines: 1000,
            timeout_secs: 25,
            terminal_output: String::new(),
            terminal_update_rx: term_rx,
            terminal_update_tx: term_tx,
            history: Vec::new(),
            max_history: 50,
            is_running: false,
        }
    }
}

// 🤖 Added `env_overrides` to pass leading KEY=VAL tokens into the child process
pub fn run_command(
    working_dir: &Path,
    cmd: &str,
    args: &[&str],
    first_n: usize,
    last_n: usize,
    do_timeout: bool,
    max_duration: Duration,
    env_overrides: &[(String, String)],
) -> String {
    let mut command = Command::new(cmd);
    command
        .args(args)
        .current_dir(working_dir) // 🤖 run inside the selected folder
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .envs(env_overrides.iter().map(|(k, v)| (k.as_str(), v.as_str()))); // 🤖 apply env vars

    let child = command.spawn().expect("Failed to spawn command");

    println!(
        "Starting child command {} {:?} in {:?}",
        cmd, args, working_dir
    );

    let output = if do_timeout {
        let child_id = child.id();
        let (tx, rx) = mpsc::channel();

        // 🤖 wait for output in a helper thread
        std::thread::spawn(move || {
            let output = child
                .wait_with_output()
                .expect("Failed to wait on child process");
            let _ = tx.send(output);
        });

        match rx.recv_timeout(max_duration) {
            Ok(output) => output,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                println!("Timeout reached after {:?}", max_duration);
                // 🤖 hard-kill on timeout to avoid zombie processes
                #[cfg(unix)]
                {
                    let _ = Command::new("kill")
                        .arg("-9")
                        .arg(child_id.to_string())
                        .status();
                }
                #[cfg(windows)]
                {
                    let _ = Command::new("taskkill")
                        .arg("/PID")
                        .arg(child_id.to_string())
                        .arg("/F")
                        .status();
                }

                rx.recv().unwrap_or_else(|_| Output {
                    status: std::process::ExitStatus::from_raw(1),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            }
            Err(e) => panic!("Error waiting for command output: {:?}", e),
        }
    } else {
        child
            .wait_with_output()
            .expect("Failed to wait on child process")
    };

    get_head_and_tail(first_n, last_n, output)
}

// ... a couple lines below
fn get_head_and_tail(first_n: usize, last_n: usize, output: Output) -> String {
    let mut combined = String::new();
    combined.push_str(&String::from_utf8_lossy(&output.stdout));
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    let lines: Vec<&str> = combined.lines().collect();
    let total = lines.len();
    let mut result = String::new();
    if total <= first_n + last_n {
        for line in lines {
            result.push_str(line);
            result.push('\n');
        }
    } else {
        for line in &lines[..first_n] {
            result.push_str(line);
            result.push('\n');
        }
        result.push_str("[... output truncated ...]\n");
        for line in &lines[total - last_n..] {
            result.push_str(line);
            result.push('\n');
        }
    }
    result
}
