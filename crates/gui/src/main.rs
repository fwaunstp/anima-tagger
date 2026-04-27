use std::collections::HashSet;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use anima_tagger_core::sidecar::Sidecar;
use anima_tagger_core::walk::iter_images;
use base64::Engine;
use dioxus::prelude::*;
use image::ImageFormat;

const THUMB_SIZE: u32 = 256;

fn main() {
    dioxus::launch(App);
}

#[derive(Clone, PartialEq)]
struct ImageItem {
    path: PathBuf,
    thumbnail: String,
    sidecar: Sidecar,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Filter {
    All,
    Untagged,
    AutoTagged,
    NoManual,
}

impl Filter {
    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Untagged => "Untagged",
            Self::AutoTagged => "Auto-tagged",
            Self::NoManual => "No manual tags",
        }
    }
    fn matches(self, item: &ImageItem) -> bool {
        match self {
            Self::All => true,
            Self::Untagged => !item.sidecar.is_auto_tagged() && item.sidecar.manual_tags.is_empty(),
            Self::AutoTagged => item.sidecar.is_auto_tagged(),
            Self::NoManual => item.sidecar.manual_tags.is_empty(),
        }
    }
}

#[component]
fn App() -> Element {
    let folder = use_signal(|| None::<PathBuf>);
    let images = use_signal(Vec::<ImageItem>::new);
    let selected = use_signal(HashSet::<PathBuf>::new);
    let filter = use_signal(|| Filter::All);
    let loading = use_signal(|| false);
    let tag_input = use_signal(String::new);

    let visible: Vec<ImageItem> = images
        .read()
        .iter()
        .filter(|i| filter.read().matches(i))
        .cloned()
        .collect();

    rsx! {
        style { {APP_CSS} }
        div { class: "app",
            Toolbar { folder, images, selected, filter, loading }
            div { class: "workspace",
                Grid { items: visible, selected }
                DetailPanel { images, selected, tag_input }
            }
        }
    }
}

#[component]
fn Toolbar(
    folder: Signal<Option<PathBuf>>,
    mut images: Signal<Vec<ImageItem>>,
    mut selected: Signal<HashSet<PathBuf>>,
    mut filter: Signal<Filter>,
    mut loading: Signal<bool>,
) -> Element {
    let on_open = move |_| {
        let Some(picked) = rfd::FileDialog::new().pick_folder() else {
            return;
        };
        loading.set(true);
        let loaded = load_folder(&picked);
        let mut f = folder;
        f.set(Some(picked));
        images.set(loaded);
        selected.set(HashSet::new());
        loading.set(false);
    };

    let folder_label = match folder.read().as_ref() {
        Some(p) => p.display().to_string(),
        None => "(no folder)".to_string(),
    };
    let count = images.read().len();
    let sel_count = selected.read().len();

    rsx! {
        div { class: "toolbar",
            button { onclick: on_open, "Open folder…" }
            span { class: "muted", "{folder_label}" }
            span { class: "spacer" }
            if *loading.read() { span { class: "muted", "Loading…" } }
            span { class: "muted", "{count} images · {sel_count} selected" }
            select {
                value: "{filter.read().label()}",
                onchange: move |evt| {
                    let f = match evt.value().as_str() {
                        "Untagged" => Filter::Untagged,
                        "Auto-tagged" => Filter::AutoTagged,
                        "No manual tags" => Filter::NoManual,
                        _ => Filter::All,
                    };
                    filter.set(f);
                },
                option { value: "All", "All" }
                option { value: "Untagged", "Untagged" }
                option { value: "Auto-tagged", "Auto-tagged" }
                option { value: "No manual tags", "No manual tags" }
            }
        }
    }
}

#[component]
fn Grid(items: Vec<ImageItem>, mut selected: Signal<HashSet<PathBuf>>) -> Element {
    if items.is_empty() {
        return rsx! { div { class: "grid empty", p { class: "muted", "No images." } } };
    }
    rsx! {
        div { class: "grid",
            for item in items.iter().cloned() {
                Thumb { item: item.clone(), selected }
            }
        }
    }
}

#[component]
fn Thumb(item: ImageItem, mut selected: Signal<HashSet<PathBuf>>) -> Element {
    let is_selected = selected.read().contains(&item.path);
    let class = if is_selected { "thumb selected" } else { "thumb" };
    let auto_flag = if item.sidecar.is_auto_tagged() { "T" } else { " " };
    let manual_flag = if !item.sidecar.manual_tags.is_empty() {
        "M"
    } else {
        " "
    };
    let path_for_click = item.path.clone();

    rsx! {
        div {
            class: "{class}",
            onclick: move |evt| {
                let mods = evt.modifiers();
                let mut sel = selected.write();
                let multi = mods.ctrl() || mods.meta() || mods.shift();
                if multi {
                    if sel.contains(&path_for_click) {
                        sel.remove(&path_for_click);
                    } else {
                        sel.insert(path_for_click.clone());
                    }
                } else {
                    sel.clear();
                    sel.insert(path_for_click.clone());
                }
            },
            img { src: "{item.thumbnail}" }
            span { class: "thumb-status", "{auto_flag}{manual_flag}" }
        }
    }
}

#[component]
fn DetailPanel(
    mut images: Signal<Vec<ImageItem>>,
    selected: Signal<HashSet<PathBuf>>,
    mut tag_input: Signal<String>,
) -> Element {
    let sel_paths: Vec<PathBuf> = selected.read().iter().cloned().collect();
    let n = sel_paths.len();

    if n == 0 {
        return rsx! {
            aside { class: "detail",
                p { class: "muted", "Select one or more images to edit tags." }
            }
        };
    }

    let imgs_snapshot = images.read().clone();
    let single_item = if n == 1 {
        imgs_snapshot.iter().find(|i| i.path == sel_paths[0]).cloned()
    } else {
        None
    };

    let mut do_add = move |raw: String| {
        let tag = raw.trim().to_string();
        if tag.is_empty() {
            return;
        }
        let sel = selected.read().clone();
        let mut imgs = images.write();
        for img in imgs.iter_mut() {
            if !sel.contains(&img.path) {
                continue;
            }
            if img.sidecar.add_manual_tag(tag.clone()) {
                let _ = img.sidecar.save(&img.path);
            }
        }
    };

    let do_remove = move |tag: String| {
        let sel = selected.read().clone();
        let mut imgs = images.write();
        for img in imgs.iter_mut() {
            if !sel.contains(&img.path) {
                continue;
            }
            if img.sidecar.remove_manual_tag(&tag) {
                let _ = img.sidecar.save(&img.path);
            }
        }
    };

    rsx! {
        aside { class: "detail",
            if n == 1 {
                if let Some(item) = single_item.as_ref() {
                    p { class: "muted",
                        "{item.path.file_name().and_then(|s| s.to_str()).unwrap_or(\"\")}"
                    }
                }
            } else {
                p { class: "muted", "{n} images selected — bulk edit" }
            }

            div { class: "section-title", "Manual tags" }
            ManualTagList {
                items: imgs_snapshot.clone(),
                selected_paths: sel_paths.clone(),
                on_remove: EventHandler::new(do_remove),
            }

            div { class: "input-row",
                input {
                    placeholder: "Add tag…",
                    value: "{tag_input}",
                    oninput: move |evt| tag_input.set(evt.value()),
                    onkeydown: {
                        let mut do_add = do_add.clone();
                        move |evt: KeyboardEvent| {
                            if evt.key() == Key::Enter {
                                let v = tag_input.read().clone();
                                do_add(v);
                                tag_input.set(String::new());
                            }
                        }
                    },
                }
                button {
                    onclick: move |_| {
                        let v = tag_input.read().clone();
                        do_add(v);
                        tag_input.set(String::new());
                    },
                    "Add"
                }
            }

            if let Some(item) = single_item {
                div { class: "section-title", "Auto tags" }
                if item.sidecar.auto_tags.is_empty() {
                    p { class: "muted", "(none — run tagger to populate)" }
                } else {
                    div { class: "tag-list",
                        for at in item.sidecar.auto_tags.iter() {
                            span { class: "chip auto",
                                "{at.tag}"
                                span { class: "score", "{at.score:.2}" }
                            }
                        }
                    }
                }
                if let Some(c) = item.sidecar.caption.as_ref() {
                    div { class: "section-title", "Caption" }
                    p { class: "caption", "{c}" }
                }
            }
        }
    }
}

#[component]
fn ManualTagList(
    items: Vec<ImageItem>,
    selected_paths: Vec<PathBuf>,
    on_remove: EventHandler<String>,
) -> Element {
    let n = selected_paths.len();
    let selected_items: Vec<&ImageItem> = items
        .iter()
        .filter(|i| selected_paths.contains(&i.path))
        .collect();

    // Compute (tag, count) pairs preserving first-seen order
    let mut order: Vec<String> = Vec::new();
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for item in &selected_items {
        for tag in &item.sidecar.manual_tags {
            if !counts.contains_key(tag) {
                order.push(tag.clone());
            }
            *counts.entry(tag.clone()).or_insert(0) += 1;
        }
    }

    if order.is_empty() {
        return rsx! { p { class: "muted", "(none)" } };
    }

    rsx! {
        div { class: "tag-list",
            for tag in order.into_iter() {
                {
                    let count = counts[&tag];
                    let label = if n > 1 && count < n {
                        format!("{tag} ({count}/{n})")
                    } else {
                        tag.clone()
                    };
                    let tag_for_remove = tag.clone();
                    rsx! {
                        span { class: "chip manual",
                            "{label}"
                            span {
                                class: "chip-x",
                                onclick: move |_| on_remove.call(tag_for_remove.clone()),
                                "×"
                            }
                        }
                    }
                }
            }
        }
    }
}

fn load_folder(dir: &Path) -> Vec<ImageItem> {
    let mut out = Vec::new();
    for path in iter_images(dir) {
        let sidecar = Sidecar::load_or_default(&path).unwrap_or_default();
        let thumbnail = make_thumbnail(&path, THUMB_SIZE).unwrap_or_default();
        out.push(ImageItem {
            path,
            thumbnail,
            sidecar,
        });
    }
    out
}

fn make_thumbnail(path: &Path, max_size: u32) -> anyhow::Result<String> {
    let img = image::open(path)?;
    let thumb = img.thumbnail(max_size, max_size).to_rgb8();
    let mut buf = Vec::new();
    image::DynamicImage::ImageRgb8(thumb).write_to(&mut Cursor::new(&mut buf), ImageFormat::Jpeg)?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&buf);
    Ok(format!("data:image/jpeg;base64,{b64}"))
}

const APP_CSS: &str = r#"
* { box-sizing: border-box; }
html, body, #main { margin: 0; height: 100%; }
body {
    font-family: -apple-system, "Segoe UI", system-ui, sans-serif;
    background: #1e1e1e;
    color: #e6e6e6;
    font-size: 13px;
}
.app { display: flex; flex-direction: column; height: 100vh; }
.toolbar {
    padding: 8px 12px;
    border-bottom: 1px solid #333;
    background: #252526;
    display: flex;
    gap: 12px;
    align-items: center;
}
.toolbar .spacer { flex: 1; }
.toolbar button, .input-row button {
    background: #4a9eff;
    color: white;
    border: none;
    padding: 6px 14px;
    border-radius: 4px;
    cursor: pointer;
    font-size: 13px;
}
.toolbar button:hover, .input-row button:hover { background: #5fa8ff; }
.toolbar select, .input-row input {
    background: #2a2a2a;
    border: 1px solid #444;
    color: #e6e6e6;
    padding: 5px 8px;
    border-radius: 4px;
    font-size: 13px;
}
.workspace {
    display: flex;
    flex: 1;
    overflow: hidden;
}
.grid {
    flex: 1;
    overflow-y: auto;
    padding: 12px;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: 8px;
    align-content: start;
}
.grid.empty { display: flex; align-items: center; justify-content: center; }
.thumb {
    aspect-ratio: 1;
    border: 2px solid transparent;
    border-radius: 4px;
    overflow: hidden;
    cursor: pointer;
    background: #2a2a2a;
    position: relative;
    user-select: none;
}
.thumb img { width: 100%; height: 100%; object-fit: cover; display: block; pointer-events: none; }
.thumb.selected { border-color: #4a9eff; }
.thumb-status {
    position: absolute; top: 4px; right: 4px;
    font-size: 10px;
    background: rgba(0,0,0,0.65);
    color: #fff;
    padding: 2px 5px;
    border-radius: 2px;
    font-family: ui-monospace, monospace;
    white-space: pre;
}
.detail {
    width: 320px;
    border-left: 1px solid #333;
    overflow-y: auto;
    padding: 12px;
    background: #252526;
}
.section-title {
    font-size: 11px;
    text-transform: uppercase;
    color: #999;
    margin-top: 14px;
    margin-bottom: 4px;
    letter-spacing: 0.04em;
}
.tag-list { display: flex; flex-wrap: wrap; gap: 4px; }
.chip {
    padding: 3px 8px;
    border-radius: 12px;
    font-size: 12px;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    line-height: 1.4;
}
.chip.manual { background: #2d4a6e; color: #cfe3ff; }
.chip.auto { background: #3a3a3a; color: #ccc; }
.chip-x { cursor: pointer; opacity: 0.55; padding: 0 2px; font-weight: bold; }
.chip-x:hover { opacity: 1; }
.score { color: #888; font-size: 10px; margin-left: 2px; }
.input-row { display: flex; gap: 6px; margin-top: 8px; }
.input-row input { flex: 1; }
.muted { color: #999; font-size: 12px; margin: 0; }
.caption { color: #ddd; font-size: 12px; line-height: 1.4; margin: 4px 0; }
"#;
