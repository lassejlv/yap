pub mod paste;
pub mod typing;

use anyhow::Result;

use crate::config::OutputMode;

pub fn deliver(mode: OutputMode, text: &str) -> Result<()> {
    if text.is_empty() {
        return Ok(());
    }
    match mode {
        OutputMode::Paste => paste::paste(text),
        OutputMode::Type => typing::type_text(text),
    }
}
