use crate::file_item::FileItem;
use crate::remote::RemoteUrl;
use std::fs;

pub fn extract_text(html: &str) -> String {
    html2text::from_read(html.as_bytes(), 80).unwrap()
}

pub fn compute_prompt(files: &[FileItem], remote_urls: &[RemoteUrl]) -> String {
    let mut prompt = String::new();
    for file_item in files.iter().filter(|f| f.selected) {
        let content = file_item.content.clone().unwrap_or_else(|| {
            fs::read_to_string(&file_item.path)
                .unwrap_or_else(|e| format!("Error reading {}: {}", file_item.rel_path, e))
        });
        prompt.push_str(&format!("```{}\n", file_item.rel_path));
        prompt.push_str(&content);
        prompt.push_str("\n```\n\n");
    }
    for remote in remote_urls.iter().filter(|r| r.include) {
        if let Some(content) = &remote.content {
            prompt.push_str(&format!("```{}\n", remote.url));
            prompt.push_str(content);
            prompt.push_str("\n```\n\n");
        }
    }
    prompt
}
