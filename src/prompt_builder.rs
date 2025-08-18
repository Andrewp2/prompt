pub fn extract_text(html: &str) -> String {
    // 🤖 Keep wrapping modest to preserve code blocks
    html2text::from_read(html.as_bytes(), 80).unwrap()
}
