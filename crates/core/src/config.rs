use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const CONFIG_FILE: &str = "anima-tagger.toml";
pub const DEFAULT_PROFILE_NAME: &str = "anima";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    #[serde(default)]
    pub default_profile: Option<String>,
    #[serde(default)]
    pub default_tagger: Option<String>,
    #[serde(default)]
    pub default_captioner: Option<String>,
    #[serde(default)]
    pub export: BTreeMap<String, ExportProfile>,
    #[serde(default)]
    pub tagger: BTreeMap<String, TaggerProfile>,
    #[serde(default)]
    pub captioner: BTreeMap<String, CaptionerProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaggerProfile {
    pub model_path: PathBuf,
    pub tags_path: PathBuf,
    #[serde(default = "default_input_size")]
    pub input_size: u32,
    #[serde(default = "default_storage_threshold")]
    pub storage_threshold: f32,
}

fn default_input_size() -> u32 {
    448
}

fn default_storage_threshold() -> f32 {
    0.10
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptionerProfile {
    /// Directory containing `vision_encoder.onnx`, `embed_tokens.onnx`,
    /// `encoder_model.onnx`, `decoder_model.onnx`, and `tokenizer.json`.
    pub model_dir: PathBuf,
    #[serde(default = "default_caption_prompt")]
    pub prompt: String,
    #[serde(default = "default_caption_input_size")]
    pub input_size: u32,
    #[serde(default = "default_max_new_tokens")]
    pub max_new_tokens: usize,
}

fn default_caption_prompt() -> String {
    "<MORE_DETAILED_CAPTION>".to_string()
}

fn default_caption_input_size() -> u32 {
    768
}

fn default_max_new_tokens() -> usize {
    1024
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportProfile {
    #[serde(default = "default_threshold")]
    pub threshold: f32,
    #[serde(default = "default_shuffle")]
    pub shuffle: bool,
    #[serde(default)]
    pub exclude_categories: Vec<String>,
    /// Map of category -> string prefix to apply to auto tags of that category
    /// (e.g. ANIMA: `{ "artist" = "@" }`).
    #[serde(default)]
    pub category_prefixes: BTreeMap<String, String>,
}

fn default_threshold() -> f32 {
    0.35
}

fn default_shuffle() -> bool {
    // sd-scripts and most modern LoRA trainers shuffle tags themselves at
    // training time, so don't shuffle on export by default.
    false
}

impl Default for ExportProfile {
    fn default() -> Self {
        Self {
            threshold: default_threshold(),
            shuffle: default_shuffle(),
            exclude_categories: Vec::new(),
            category_prefixes: BTreeMap::new(),
        }
    }
}

impl ExportProfile {
    pub fn anima() -> Self {
        let mut category_prefixes = BTreeMap::new();
        category_prefixes.insert("artist".to_string(), "@".to_string());
        Self {
            threshold: default_threshold(),
            shuffle: default_shuffle(),
            exclude_categories: Vec::new(),
            category_prefixes,
        }
    }

    pub fn category_prefix(&self, category: &str) -> Option<&str> {
        self.category_prefixes.get(category).map(String::as_str)
    }

    pub fn all_prefixes(&self) -> impl Iterator<Item = &str> {
        self.category_prefixes.values().map(String::as_str)
    }
}

impl Default for ProjectConfig {
    fn default() -> Self {
        let mut export = BTreeMap::new();
        export.insert("anima".to_string(), ExportProfile::anima());
        export.insert("plain".to_string(), ExportProfile::default());
        Self {
            default_profile: Some(DEFAULT_PROFILE_NAME.to_string()),
            default_tagger: None,
            default_captioner: None,
            export,
            tagger: BTreeMap::new(),
            captioner: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("io error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("parse error on {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
}

impl ProjectConfig {
    pub fn load(dir: &Path) -> Result<Option<Self>, ConfigError> {
        let path = dir.join(CONFIG_FILE);
        if !path.exists() {
            return Ok(None);
        }
        let s = fs::read_to_string(&path).map_err(|source| ConfigError::Io {
            path: path.clone(),
            source,
        })?;
        let cfg = toml::from_str(&s).map_err(|source| ConfigError::Parse {
            path: path.clone(),
            source,
        })?;
        Ok(Some(cfg))
    }

    pub fn load_or_default(dir: &Path) -> Result<Self, ConfigError> {
        Ok(Self::load(dir)?.unwrap_or_default())
    }

    pub fn resolve_profile(&self, name: Option<&str>) -> ExportProfile {
        let key = name
            .map(str::to_string)
            .or_else(|| self.default_profile.clone());
        if let Some(k) = key.as_deref()
            && let Some(p) = self.export.get(k)
        {
            return p.clone();
        }
        ExportProfile::default()
    }

    /// Resolve a tagger profile by explicit name or fall back to `default_tagger`.
    /// Returns None if neither selects a valid profile (i.e. the user must configure one).
    pub fn resolve_tagger(&self, name: Option<&str>) -> Option<(String, TaggerProfile)> {
        let key = name
            .map(str::to_string)
            .or_else(|| self.default_tagger.clone())?;
        let profile = self.tagger.get(&key)?.clone();
        Some((key, profile))
    }

    pub fn resolve_captioner(&self, name: Option<&str>) -> Option<(String, CaptionerProfile)> {
        let key = name
            .map(str::to_string)
            .or_else(|| self.default_captioner.clone())?;
        let profile = self.captioner.get(&key)?.clone();
        Some((key, profile))
    }
}
