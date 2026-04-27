use std::path::PathBuf;

use anima_tagger_core::config::ProjectConfig;
use anima_tagger_core::export;
use anima_tagger_core::sidecar::Sidecar;
use anima_tagger_core::walk::iter_images;
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "anima-tagger",
    about = "Manage manual + auto tags and captions for ANIMA-style LoRA datasets"
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
        #[arg(long)]
        force: bool,
    },
    /// Run the automatic captioner over images in a directory.
    Caption {
        dir: PathBuf,
        #[arg(long)]
        force: bool,
    },
    /// Merge manual + auto tags and write `<image>.txt` for training.
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
        Command::Tag { .. } => {
            anyhow::bail!("automatic tagger not yet implemented (next iteration)");
        }
        Command::Caption { .. } => {
            anyhow::bail!("automatic captioner not yet implemented");
        }
        Command::Export {
            dir,
            profile,
            threshold,
        } => cmd_export(dir, profile, threshold),
        Command::Status { dir } => cmd_status(dir),
    }
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
            None => println!("[  ] manual=0   {}", image.display()),
            Some(s) => {
                let auto = if s.is_auto_tagged() { 'T' } else { ' ' };
                let cap = if s.is_captioned() { 'C' } else { ' ' };
                let n = s.manual_tags.len();
                println!("[{auto}{cap}] manual={n:<3} {}", image.display());
            }
        }
    }
    Ok(())
}
