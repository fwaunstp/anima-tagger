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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Sidecar {
    #[serde(default)]
    pub manual_tags: Vec<String>,
    #[serde(default)]
    pub auto_tags: Vec<AutoTag>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tagger: Option<TaggerInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub captioner: Option<CaptionerInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutoTag {
    pub tag: String,
    pub score: f32,
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
}
