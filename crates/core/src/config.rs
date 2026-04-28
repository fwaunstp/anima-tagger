use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const CONFIG_FILE: &str = "anima-tagger.toml";
pub const DEFAULT_PROFILE_NAME: &str = "anima";

/// Built-in tagger profile name + repo, used when nothing is configured.
pub const BUILT_IN_TAGGER_NAME: &str = "wd-eva02-large-v3";
pub const BUILT_IN_TAGGER_REPO: &str = "SmilingWolf/wd-eva02-large-tagger-v3";

/// Built-in captioner profile name + repo, used when nothing is configured.
pub const BUILT_IN_CAPTIONER_NAME: &str = "florence2-base";
pub const BUILT_IN_CAPTIONER_REPO: &str = "onnx-community/Florence-2-base-ft";

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

/// HuggingFace-hosted WD14-family tagger profile. Models are downloaded into
/// the shared hf-hub cache on first use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaggerProfile {
    /// HuggingFace repo id, e.g. `"SmilingWolf/wd-eva02-large-tagger-v3"`.
    pub repo: String,
    /// Optional git revision/branch/tag to pin (defaults to the repo's `main`).
    #[serde(default)]
    pub revision: Option<String>,
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

/// HuggingFace-hosted Florence-2 captioner profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptionerProfile {
    /// HuggingFace repo id, e.g. `"onnx-community/Florence-2-base-ft"`.
    pub repo: String,
    #[serde(default)]
    pub revision: Option<String>,
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

impl TaggerProfile {
    pub fn built_in() -> Self {
        Self {
            repo: BUILT_IN_TAGGER_REPO.to_string(),
            revision: None,
            input_size: default_input_size(),
            storage_threshold: default_storage_threshold(),
        }
    }
}

impl CaptionerProfile {
    pub fn built_in() -> Self {
        Self {
            repo: BUILT_IN_CAPTIONER_REPO.to_string(),
            revision: None,
            prompt: default_caption_prompt(),
            input_size: default_caption_input_size(),
            max_new_tokens: default_max_new_tokens(),
        }
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
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("parse error on {path}: {source}")]
    Parse {
        path: std::path::PathBuf,
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

    /// Resolve a tagger profile. Order: explicit `name`, then `default_tagger`,
    /// then the built-in profile. Always succeeds — auto-download means a
    /// configured profile is no longer required.
    pub fn resolve_tagger(&self, name: Option<&str>) -> (String, TaggerProfile) {
        let key = name
            .map(str::to_string)
            .or_else(|| self.default_tagger.clone());
        if let Some(k) = key
            && let Some(profile) = self.tagger.get(&k)
        {
            return (k, profile.clone());
        }
        (BUILT_IN_TAGGER_NAME.to_string(), TaggerProfile::built_in())
    }

    /// Resolve a captioner profile, falling back to the built-in if nothing
    /// matches. Same logic as `resolve_tagger`.
    pub fn resolve_captioner(&self, name: Option<&str>) -> (String, CaptionerProfile) {
        let key = name
            .map(str::to_string)
            .or_else(|| self.default_captioner.clone());
        if let Some(k) = key
            && let Some(profile) = self.captioner.get(&k)
        {
            return (k, profile.clone());
        }
        (
            BUILT_IN_CAPTIONER_NAME.to_string(),
            CaptionerProfile::built_in(),
        )
    }
}
