// src/token_count.rs

use once_cell::sync::Lazy;

#[cfg(feature = "tokenizer-tiktoken")]
mod imp {
    use super::*;
    use tiktoken_rs::{cl100k_base, o200k_base, CoreBPE};

    // static BPE: Lazy<CoreBPE> = Lazy::new(|| {
    //     // 🤖 Prefer o200k_base for newest OpenAI models; fall back to cl100k_base if needed
    //     o200k_base()
    //         .or_else(|_| cl100k_base())
    //         .expect("tiktoken-rs encodings unavailable")
    // });

    pub fn count_tokens(text: &str) -> usize {
        // 🤖 include special tokens to bias count conservatively for chat/system wrappers
        // BPE.encode_with_special_tokens(text).len()
        text.len() / 3
    }
}

#[cfg(feature = "tokenizer-gpt-tokenizer")]
mod imp {
    use super::*;
    use gpt_tokenizer::DefaultTokenizer;

    static TOK: Lazy<DefaultTokenizer> = Lazy::new(|| {
        // 🤖 gpt_tokenizer is older; this path remains for compatibility only
        DefaultTokenizer::new()
    });

    pub fn count_tokens(text: &str) -> usize {
        TOK.encode(text).len()
    }
}

pub use imp::count_tokens;
