use std::path::PathBuf;

use anima_tagger_booru::{BooruClient, BooruError};
use anima_tagger_captioner::Captioner;
use anima_tagger_core::config::ProjectConfig;
use anima_tagger_core::export;
use anima_tagger_core::sidecar::{CaptionerInfo, Sidecar, TaggerInfo};
use anima_tagger_core::walk::iter_images;
use anima_tagger_tagger::Tagger;
use anyhow::{Context, Result};
use chrono::Utc;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "anima-tagger",
    about = "Manage manual + auto + booru tags and captions for ANIMA-style LoRA datasets"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the automatic tagger over images in a directory.
    Tag {
        dir: PathBuf,
        /// Name of a `[tagger.<name>]` profile in `anima-tagger.toml`.
        #[arg(long)]
        model: Option<String>,
        /// Re-tag images that already have an auto-tag record.
        #[arg(long)]
        force: bool,
        /// Override the storage threshold from the tagger profile.
        #[arg(long)]
        threshold: Option<f32>,
    },
    /// Run the automatic captioner over images in a directory.
    Caption {
        dir: PathBuf,
        /// Name of a `[captioner.<name>]` profile in `anima-tagger.toml`.
        #[arg(long)]
        model: Option<String>,
        /// Re-caption images that already have a caption record.
        #[arg(long)]
        force: bool,
    },
    /// Fetch tags from a booru API by image MD5 hash.
    Booru {
        dir: PathBuf,
        /// Booru source (`danbooru` is the only one currently implemented).
        #[arg(long, default_value = "danbooru")]
        source: String,
        /// Re-fetch images that already have booru data.
        #[arg(long)]
        force: bool,
    },
    /// Merge manual + auto + booru tags and write `<image>.txt` for training.
    Export {
        dir: PathBuf,
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        threshold: Option<f32>,
    },
    /// Show sidecar status for images in a directory.
    Status { dir: PathBuf },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Tag {
            dir,
            model,
            force,
            threshold,
        } => cmd_tag(dir, model, force, threshold),
        Command::Caption { dir, model, force } => cmd_caption(dir, model, force),
        Command::Booru { dir, source, force } => cmd_booru(dir, source, force),
        Command::Export {
            dir,
            profile,
            threshold,
        } => cmd_export(dir, profile, threshold),
        Command::Status { dir } => cmd_status(dir),
    }
}

fn cmd_tag(
    dir: PathBuf,
    model_name: Option<String>,
    force: bool,
    threshold_override: Option<f32>,
) -> Result<()> {
    let cfg = ProjectConfig::load_or_default(&dir)
        .with_context(|| format!("loading config in {}", dir.display()))?;
    let (resolved_name, profile) = cfg
        .resolve_tagger(model_name.as_deref())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no tagger profile configured. Add a [tagger.<name>] section to anima-tagger.toml \
                 with model_path and tags_path, and either pass --model <name> or set default_tagger."
            )
        })?;
    let threshold = threshold_override.unwrap_or(profile.storage_threshold);

    eprintln!(
        "loading model `{resolved_name}` from {} …",
        profile.model_path.display()
    );
    let mut tagger = Tagger::from_profile(&profile)?;
    eprintln!("model ready ({} tags)", tagger.num_tags());

    let mut tagged = 0usize;
    let mut skipped = 0usize;
    for image in iter_images(&dir) {
        let mut sc = Sidecar::load_or_default(&image)?;
        if !force && sc.is_auto_tagged() {
            skipped += 1;
            continue;
        }
        let tags = tagger.tag_image(&image, threshold)?;
        let n = tags.len();
        sc.auto_tags = tags;
        sc.tagger = Some(TaggerInfo {
            model: resolved_name.clone(),
            tagged_at: Utc::now(),
        });
        sc.save(&image)?;
        tagged += 1;
        println!("tagged {} ({n} tags)", image.display());
    }
    println!("done: {tagged} tagged, {skipped} skipped (use --force to retag)");
    Ok(())
}

fn cmd_caption(dir: PathBuf, model_name: Option<String>, force: bool) -> Result<()> {
    let cfg = ProjectConfig::load_or_default(&dir)
        .with_context(|| format!("loading config in {}", dir.display()))?;
    let (resolved_name, profile) = cfg
        .resolve_captioner(model_name.as_deref())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no captioner profile configured. Add a [captioner.<name>] section to \
                 anima-tagger.toml with model_dir (containing vision_encoder.onnx, \
                 embed_tokens.onnx, encoder_model.onnx, decoder_model.onnx, tokenizer.json) \
                 and either pass --model <name> or set default_captioner."
            )
        })?;

    eprintln!(
        "loading captioner `{resolved_name}` from {} …",
        profile.model_dir.display()
    );
    let mut captioner = Captioner::from_profile(&profile)?;
    eprintln!("captioner ready");

    let mut captioned = 0usize;
    let mut skipped = 0usize;
    for image in iter_images(&dir) {
        let mut sc = Sidecar::load_or_default(&image)?;
        if !force && sc.is_captioned() {
            skipped += 1;
            continue;
        }
        let caption = captioner.caption_image(&image)?;
        let preview: String = caption.chars().take(60).collect();
        sc.caption = Some(caption);
        sc.captioner = Some(CaptionerInfo {
            model: resolved_name.clone(),
            captioned_at: Utc::now(),
        });
        sc.save(&image)?;
        captioned += 1;
        println!("captioned {} — \"{preview}…\"", image.display());
    }
    println!("done: {captioned} captioned, {skipped} skipped (use --force to recaption)");
    Ok(())
}

fn cmd_booru(dir: PathBuf, source: String, force: bool) -> Result<()> {
    let client = match source.as_str() {
        "danbooru" => BooruClient::danbooru(),
        other => anyhow::bail!(
            "unsupported booru source `{other}` (only 'danbooru' is implemented)"
        ),
    };

    let mut fetched = 0usize;
    let mut not_found = 0usize;
    let mut skipped = 0usize;
    for image in iter_images(&dir) {
        let mut sc = Sidecar::load_or_default(&image)?;
        if !force && sc.has_booru() {
            skipped += 1;
            continue;
        }
        match client.fetch_for_image(&image) {
            Ok((tags, info)) => {
                let n = tags.len();
                sc.booru_tags = tags;
                sc.booru = Some(info);
                sc.save(&image)?;
                fetched += 1;
                println!("fetched {} ({n} tags)", image.display());
            }
            Err(BooruError::NotFound(_)) => {
                not_found += 1;
                println!("not on booru: {}", image.display());
            }
            Err(e) => {
                eprintln!("error: {}: {e}", image.display());
            }
        }
    }
    println!("done: {fetched} fetched, {not_found} not found, {skipped} skipped");
    Ok(())
}

fn cmd_export(dir: PathBuf, profile_name: Option<String>, threshold: Option<f32>) -> Result<()> {
    let cfg = ProjectConfig::load_or_default(&dir)
        .with_context(|| format!("loading config in {}", dir.display()))?;
    let mut profile = cfg.resolve_profile(profile_name.as_deref());
    if let Some(t) = threshold {
        profile.threshold = t;
    }

    let mut written = 0usize;
    let mut skipped = 0usize;
    for image in iter_images(&dir) {
        let sidecar = match Sidecar::load(&image)? {
            Some(s) => s,
            None => {
                skipped += 1;
                continue;
            }
        };
        let out = export::export_image(&image, &sidecar, &profile)?;
        println!("wrote {}", out.display());
        written += 1;
    }
    println!("done: {written} written, {skipped} skipped (no sidecar)");
    Ok(())
}

fn cmd_status(dir: PathBuf) -> Result<()> {
    for image in iter_images(&dir) {
        match Sidecar::load(&image)? {
            None => println!("[   ] manual=0   {}", image.display()),
            Some(s) => {
                let auto = if s.is_auto_tagged() { 'T' } else { ' ' };
                let cap = if s.is_captioned() { 'C' } else { ' ' };
                let booru = if s.has_booru() { 'B' } else { ' ' };
                let n = s.manual_tags.len();
                println!("[{auto}{cap}{booru}] manual={n:<3} {}", image.display());
            }
        }
    }
    Ok(())
}
