use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use ron::ser::PrettyConfig;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod category {
    pub const GENERAL: &str = "general";
    pub const ARTIST: &str = "artist";
    pub const COPYRIGHT: &str = "copyright";
    pub const CHARACTER: &str = "character";
    pub const META: &str = "meta";
    pub const RATING: &str = "rating";
}

pub const SIDECAR_SUFFIX: &str = ".ron";

/// Manual entries beginning with this character are treated as suppression
/// markers (e.g. `-watermark` removes any auto/booru tag with stem `watermark`
/// from the export, regardless of which tagger produced it).
pub const NEGATIVE_PREFIX: char = '-';

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Sidecar {
    /// Manual entries. `foo` = positive (always exported); `-foo` = suppression
    /// marker (removes matching auto/booru tag from export). Negative entries
    /// are never themselves emitted to the training `.txt` file.
    #[serde(default)]
    pub manual_tags: Vec<String>,
    #[serde(default)]
    pub auto_tags: Vec<AutoTag>,
    #[serde(default)]
    pub booru_tags: Vec<BooruTag>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tagger: Option<TaggerInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub captioner: Option<CaptionerInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub booru: Option<BooruInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutoTag {
    pub tag: String,
    pub score: f32,
    pub category: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BooruTag {
    pub tag: String,
    pub category: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaggerInfo {
    pub model: String,
    pub tagged_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaptionerInfo {
    pub model: String,
    pub captioned_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BooruInfo {
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_url: Option<String>,
    pub fetched_at: DateTime<Utc>,
}

#[derive(Debug, Error)]
pub enum SidecarError {
    #[error("io error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("ron parse error on {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: ron::de::SpannedError,
    },
    #[error("ron serialize error: {0}")]
    Serialize(#[from] ron::Error),
}

pub fn sidecar_path_for(image: &Path) -> PathBuf {
    let mut s = image.as_os_str().to_owned();
    s.push(SIDECAR_SUFFIX);
    PathBuf::from(s)
}

fn pretty_config() -> PrettyConfig {
    PrettyConfig::default()
        .struct_names(false)
        .indentor("  ".to_string())
}

impl Sidecar {
    pub fn load(image: &Path) -> Result<Option<Self>, SidecarError> {
        let path = sidecar_path_for(image);
        if !path.exists() {
            return Ok(None);
        }
        let s = fs::read_to_string(&path).map_err(|source| SidecarError::Io {
            path: path.clone(),
            source,
        })?;
        let parsed = ron::de::from_str(&s).map_err(|source| SidecarError::Parse {
            path: path.clone(),
            source,
        })?;
        Ok(Some(parsed))
    }

    pub fn load_or_default(image: &Path) -> Result<Self, SidecarError> {
        Ok(Self::load(image)?.unwrap_or_default())
    }

    pub fn save(&self, image: &Path) -> Result<(), SidecarError> {
        let path = sidecar_path_for(image);
        let body = ron::ser::to_string_pretty(self, pretty_config())?;
        let mut tmp_os = path.as_os_str().to_owned();
        tmp_os.push(".tmp");
        let tmp = PathBuf::from(tmp_os);
        fs::write(&tmp, body).map_err(|source| SidecarError::Io {
            path: tmp.clone(),
            source,
        })?;
        fs::rename(&tmp, &path).map_err(|source| SidecarError::Io {
            path: path.clone(),
            source,
        })?;
        Ok(())
    }

    pub fn is_auto_tagged(&self) -> bool {
        self.tagger.is_some()
    }

    pub fn is_captioned(&self) -> bool {
        self.captioner.is_some()
    }

    pub fn has_booru(&self) -> bool {
        self.booru.is_some()
    }

    /// Iterates positive manual entries (skipping suppression markers).
    pub fn manual_positive_tags(&self) -> impl Iterator<Item = &str> {
        self.manual_tags
            .iter()
            .filter(|t| !t.trim().starts_with(NEGATIVE_PREFIX))
            .map(|t| t.as_str())
    }

    /// Returns lowercase stems suppressed by `-foo` manual entries.
    pub fn suppressed_set(&self) -> HashSet<String> {
        self.manual_tags
            .iter()
            .filter_map(|t| {
                t.trim()
                    .strip_prefix(NEGATIVE_PREFIX)
                    .map(|s| s.trim().to_lowercase())
                    .filter(|s| !s.is_empty())
            })
            .collect()
    }

    pub fn is_suppressed(&self, tag: &str) -> bool {
        let key = tag.trim().to_lowercase();
        if key.is_empty() {
            return false;
        }
        self.manual_tags.iter().any(|m| {
            m.trim()
                .strip_prefix(NEGATIVE_PREFIX)
                .map(|s| s.trim().to_lowercase() == key)
                .unwrap_or(false)
        })
    }

    /// Append a manual entry verbatim (positive or `-foo` suppression). Returns
    /// `true` if newly added, `false` if it was already present or empty.
    pub fn add_manual_tag(&mut self, tag: impl Into<String>) -> bool {
        let t = tag.into();
        let trimmed = t.trim();
        if trimmed.is_empty() || self.manual_tags.iter().any(|x| x == trimmed) {
            return false;
        }
        self.manual_tags.push(trimmed.to_string());
        true
    }

    pub fn remove_manual_tag(&mut self, tag: &str) -> bool {
        let before = self.manual_tags.len();
        self.manual_tags.retain(|x| x != tag);
        before != self.manual_tags.len()
    }

    /// Add `-tag` as a suppression marker if not already present.
    pub fn suppress(&mut self, tag: &str) -> bool {
        let trimmed = tag.trim();
        if trimmed.is_empty() {
            return false;
        }
        let neg = format!("-{trimmed}");
        if self.manual_tags.iter().any(|x| x == &neg) {
            return false;
        }
        self.manual_tags.push(neg);
        true
    }

    /// Remove the `-tag` suppression marker if present.
    pub fn unsuppress(&mut self, tag: &str) -> bool {
        let neg = format!("-{}", tag.trim());
        let before = self.manual_tags.len();
        self.manual_tags.retain(|x| x != &neg);
        before != self.manual_tags.len()
    }
}
