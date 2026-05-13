use anyhow::{Context, Result};
use enigo::{Enigo, Keyboard, Settings};

pub fn type_text(text: &str) -> Result<()> {
    let mut enigo = Enigo::new(&Settings::default()).context("init enigo")?;
    enigo.text(text).context("simulated typing")?;
    Ok(())
}
