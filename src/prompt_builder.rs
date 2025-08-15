use crate::file_item::FileItem;
use crate::remote::RemoteUrl;

pub fn extract_text(html: &str) -> String {
    // 🤖 Keep wrapping modest to preserve code blocks
    html2text::from_read(html.as_bytes(), 80).unwrap()
}

pub fn compute_prompt(files: &[FileItem], remote_urls: &[RemoteUrl]) -> String {
    // 🤖 In-memory only: DO NOT read from disk here. The preview stays fast.
    let mut prompt = String::new();

    for file_item in files.iter().filter(|f| f.selected) {
        let content = file_item.content.as_deref().unwrap_or(""); // 🤖 no disk IO
        prompt.push_str(&format!("```{}\n", file_item.rel_path));
        prompt.push_str(content);
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
