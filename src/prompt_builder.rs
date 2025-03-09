use crate::file_item::FileItem;
use crate::remote::RemoteUrl;
use std::fs;

pub fn extract_text(html: &str) -> String {
    html2text::from_read(html.as_bytes(), 80).unwrap()
}

pub fn compute_prompt(files: &[FileItem], extra_text: &str, remote_urls: &[RemoteUrl]) -> String {
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
    for remote in remote_urls.iter().filter(|r| r.include) {
        if let Some(ref content) = remote.content {
            prompt.push_str(&format!("```{}\n", remote.url));
            prompt.push_str(&content);
            prompt.push_str("\n```\n\n");
        }
    }
    prompt.push_str(extra_text);
    prompt
}
