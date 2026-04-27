use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use rand::seq::SliceRandom;
use thiserror::Error;

use crate::config::ExportProfile;
use crate::sidecar::Sidecar;

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("io error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub fn export_text_path(image: &Path) -> PathBuf {
    image.with_extension("txt")
}

/// Build the final ordered tag list for a single image, applying:
/// - threshold + category exclusion to auto tags
/// - category prefix formatting (e.g. ANIMA artist `@`)
/// - dedup (manual wins on collision; comparison is prefix-stripped, lowercase)
/// - optional shuffle
pub fn build_tags(sidecar: &Sidecar, profile: &ExportProfile) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for raw in &sidecar.manual_tags {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let stem = normalize_stem(trimmed, profile);
        if seen.insert(stem) {
            out.push(trimmed.to_string());
        }
    }

    for at in &sidecar.auto_tags {
        if at.score < profile.threshold {
            continue;
        }
        if profile
            .exclude_categories
            .iter()
            .any(|c| c == &at.category)
        {
            continue;
        }
        let formatted = format_auto_tag(&at.tag, &at.category, profile);
        let stem = normalize_stem(&formatted, profile);
        if seen.insert(stem) {
            out.push(formatted);
        }
    }

    if profile.shuffle {
        let mut rng = rand::thread_rng();
        out.shuffle(&mut rng);
    }
    out
}

pub fn export_image(
    image: &Path,
    sidecar: &Sidecar,
    profile: &ExportProfile,
) -> Result<PathBuf, ExportError> {
    let tags = build_tags(sidecar, profile);
    let body = tags.join(", ");
    let out = export_text_path(image);
    fs::write(&out, body).map_err(|source| ExportError::Io {
        path: out.clone(),
        source,
    })?;
    Ok(out)
}

fn format_auto_tag(tag: &str, category: &str, profile: &ExportProfile) -> String {
    match profile.category_prefix(category) {
        Some(p) => format!("{p}{tag}"),
        None => tag.to_string(),
    }
}

fn normalize_stem(s: &str, profile: &ExportProfile) -> String {
    let trimmed = s.trim();
    let mut current = trimmed;
    for prefix in profile.all_prefixes() {
        if let Some(stripped) = current.strip_prefix(prefix) {
            current = stripped;
            break;
        }
    }
    current.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidecar::AutoTag;

    fn no_shuffle(mut p: ExportProfile) -> ExportProfile {
        p.shuffle = false;
        p
    }

    #[test]
    fn manual_wins_over_auto_with_artist_prefix() {
        let sidecar = Sidecar {
            manual_tags: vec!["tezuka_osamu".into()],
            auto_tags: vec![
                AutoTag {
                    tag: "tezuka_osamu".into(),
                    score: 0.9,
                    category: "artist".into(),
                },
                AutoTag {
                    tag: "1girl".into(),
                    score: 0.95,
                    category: "general".into(),
                },
            ],
            ..Default::default()
        };
        let profile = no_shuffle(ExportProfile::anima());
        let tags = build_tags(&sidecar, &profile);
        // manual `tezuka_osamu` keeps its raw form, and `@tezuka_osamu` from auto is deduped out
        assert_eq!(tags, vec!["tezuka_osamu".to_string(), "1girl".to_string()]);
    }

    #[test]
    fn artist_prefix_applied_when_no_collision() {
        let sidecar = Sidecar {
            auto_tags: vec![AutoTag {
                tag: "tezuka_osamu".into(),
                score: 0.9,
                category: "artist".into(),
            }],
            ..Default::default()
        };
        let profile = no_shuffle(ExportProfile::anima());
        let tags = build_tags(&sidecar, &profile);
        assert_eq!(tags, vec!["@tezuka_osamu".to_string()]);
    }

    #[test]
    fn threshold_filters_auto_only() {
        let sidecar = Sidecar {
            auto_tags: vec![
                AutoTag {
                    tag: "high".into(),
                    score: 0.9,
                    category: "general".into(),
                },
                AutoTag {
                    tag: "low".into(),
                    score: 0.1,
                    category: "general".into(),
                },
            ],
            ..Default::default()
        };
        let mut profile = no_shuffle(ExportProfile::default());
        profile.threshold = 0.5;
        let tags = build_tags(&sidecar, &profile);
        assert_eq!(tags, vec!["high".to_string()]);
    }

    #[test]
    fn excluded_category_dropped() {
        let sidecar = Sidecar {
            auto_tags: vec![
                AutoTag {
                    tag: "watermark".into(),
                    score: 0.9,
                    category: "meta".into(),
                },
                AutoTag {
                    tag: "1girl".into(),
                    score: 0.9,
                    category: "general".into(),
                },
            ],
            ..Default::default()
        };
        let mut profile = no_shuffle(ExportProfile::default());
        profile.exclude_categories = vec!["meta".into()];
        let tags = build_tags(&sidecar, &profile);
        assert_eq!(tags, vec!["1girl".to_string()]);
    }

    #[test]
    fn manual_order_preserved_when_no_shuffle() {
        let sidecar = Sidecar {
            manual_tags: vec!["my_trigger".into(), "outfit_a".into()],
            auto_tags: vec![AutoTag {
                tag: "1girl".into(),
                score: 0.9,
                category: "general".into(),
            }],
            ..Default::default()
        };
        let profile = no_shuffle(ExportProfile::default());
        let tags = build_tags(&sidecar, &profile);
        assert_eq!(
            tags,
            vec![
                "my_trigger".to_string(),
                "outfit_a".to_string(),
                "1girl".to_string()
            ]
        );
    }
}
