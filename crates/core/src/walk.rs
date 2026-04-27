use std::path::{Path, PathBuf};

use walkdir::WalkDir;

pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp"];

pub fn is_image_path(p: &Path) -> bool {
    let Some(ext) = p.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    let lower = ext.to_ascii_lowercase();
    IMAGE_EXTENSIONS.iter().any(|e| *e == lower)
}

pub fn iter_images(dir: &Path) -> impl Iterator<Item = PathBuf> + use<> {
    WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter(|e| is_image_path(e.path()))
        .map(|e| e.path().to_path_buf())
}
