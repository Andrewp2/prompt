mod app;
mod command_runner;
mod file_item;
mod file_tree;
mod prompt_builder;
mod remote;
mod token_count; // 🤖 NEW: tokenizer-backed counting

fn main() {
    app::run();
}
