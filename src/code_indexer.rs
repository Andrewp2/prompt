use serde_json::Value;
use std::io;
use std::path::Path;
use std::process::Command;

/// Represents a code snippet extracted from the codebase.
#[derive(Debug)]
pub struct CodeSnippet {
    pub file: String,
    pub line: usize,
    pub snippet: String,
}

/// Invokes the `ast-grep` CLI on the given directory with the specified pattern.
/// This function assumes that `ast-grep` is installed and available in the system PATH.
/// It runs the search with `--json` output for easier parsing.
pub fn index_codebase(directory: &Path, pattern: &str) -> io::Result<Vec<CodeSnippet>> {
    let output = Command::new("ast-grep")
        .arg(pattern)
        .arg("--json")
        .current_dir(directory)
        .output()?;

    if !output.status.success() {
        return Err(io::Error::new(io::ErrorKind::Other, "ast-grep failed"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut snippets = Vec::new();

    // Assuming each line is one JSON result. (Adjust according to ast-grep output.)
    for line in stdout.lines() {
        if let Ok(json_val) = serde_json::from_str::<Value>(line) {
            // Adapt the keys below to match the actual JSON structure produced by ast-grep.
            let file = json_val
                .get("file")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let line_number = json_val.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let snippet = json_val
                .get("match")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            snippets.push(CodeSnippet {
                file,
                line: line_number,
                snippet,
            });
        }
    }

    Ok(snippets)
}

/// Given a user query and code snippets, generate a prompt that includes
/// the relevant code fragments along with the query context.
pub fn generate_prompt_from_snippets(query: &str, snippets: &[CodeSnippet]) -> String {
    let mut prompt = String::new();
    prompt.push_str("Here are some relevant code snippets from your codebase:\n\n");
    for snippet in snippets {
        prompt.push_str(&format!(
            "File: {} (line {}):\n",
            snippet.file, snippet.line
        ));
        prompt.push_str("```\n");
        prompt.push_str(&snippet.snippet);
        prompt.push_str("\n```\n\n");
    }
    prompt.push_str("User Query:\n");
    prompt.push_str(query);
    prompt
}

/// A helper function to run the index and then generate a prompt.
/// You can call this from your main application logic whenever you need to fetch snippets.
pub fn index_and_generate_prompt(
    directory: &Path,
    pattern: &str,
    query: &str,
) -> io::Result<String> {
    let snippets = index_codebase(directory, pattern)?;
    Ok(generate_prompt_from_snippets(query, &snippets))
}
