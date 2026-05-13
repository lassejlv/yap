use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutputMode {
    Paste,
    Type,
}

impl Default for OutputMode {
    fn default() -> Self {
        Self::Paste
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub openai_api_key: String,

    #[serde(default = "default_stt_model")]
    pub stt_model: String,

    #[serde(default = "default_cleanup_model")]
    pub cleanup_model: String,

    #[serde(default = "default_true")]
    pub cleanup_enabled: bool,

    #[serde(default = "default_cleanup_prompt")]
    pub cleanup_prompt: String,

    #[serde(default)]
    pub vocabulary: Vec<String>,

    #[serde(default)]
    pub output_mode: OutputMode,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            openai_api_key: std::env::var("OPENAI_API_KEY").unwrap_or_default(),
            stt_model: default_stt_model(),
            cleanup_model: default_cleanup_model(),
            cleanup_enabled: true,
            cleanup_prompt: default_cleanup_prompt(),
            vocabulary: Vec::new(),
            output_mode: OutputMode::default(),
        }
    }
}

impl Config {
    pub fn path() -> Result<PathBuf> {
        let dirs = ProjectDirs::from("dev", "whispr", "whispr")
            .context("could not resolve config directory")?;
        Ok(dirs.config_dir().join("config.toml"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::path()?;
        if !path.exists() {
            let cfg = Self::default();
            cfg.save()?;
            return Ok(cfg);
        }
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let mut cfg: Self = toml::from_str(&raw).context("parsing config.toml")?;
        if cfg.openai_api_key.is_empty() {
            if let Ok(env_key) = std::env::var("OPENAI_API_KEY") {
                cfg.openai_api_key = env_key;
            }
        }
        Ok(cfg)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let raw = toml::to_string_pretty(self)?;
        std::fs::write(&path, raw)
            .with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }
}

fn default_stt_model() -> String {
    "gpt-4o-transcribe".into()
}

fn default_cleanup_model() -> String {
    "gpt-4o-mini".into()
}

fn default_true() -> bool {
    true
}

fn default_cleanup_prompt() -> String {
    "You clean up raw dictation. Fix punctuation and capitalization. \
     Remove filler words (um, uh, like, you know). Preserve meaning verbatim. \
     Do not add commentary. Output only the cleaned text."
        .into()
}
