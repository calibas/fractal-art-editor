//! The server's previews (docs/projects/previews.md): a picture of every
//! app built-in, rendered here -- the server has no GPU -- and uploaded by
//! an admin.
//!
//! Each picture is the Catalog's own preview of the entry
//! ([`super::inventory_preview::preview`]: a variation's image of the
//! plane, a colouring on the default formula, a simulation's first preset,
//! a generator's run at seed 1, ...), rendered at [`SIZE`] at the
//! thumbnail's sample density (`renderer::render_preview_at_async`). An
//! effect's picture is its "after"; a preset's is itself.
//!
//! Named as the API knows the item: a curated item (a preset, a shipped
//! script or Rhai example, a variation, an effect) by the manifest's name
//! ([`crate::curated::manifest`], whose origins say where each record came
//! from), a built-in type by its registry key. Where both give one name --
//! an example named for its built-in -- the built-in's picture is kept.
//!
//! `cargo run --release --bin export_previews` (`src/bin/export_previews.rs`
//! has the flags): render into `output/server-previews/<kind>/<name>.jpg` with an
//! `index.json`, and with `--upload`, PUT what the server lacks or has
//! from another render (`source_hash`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::inventory::{Entry, EntryKey, Kind};

/// The pictures' side, the size of a fractal's thumbnail.
pub const SIZE: u32 = 512;

/// Prefixed to every `source_hash`: a change to how previews are rendered
/// (size, density) moves it, and every picture is made again. A renderer
/// fix that changes pixels without changing a config needs `--force`.
pub const EPOCH: &str = "preview-512-1";

/// What a run does.
#[derive(Debug, Clone)]
pub struct Options {
    /// Where the JPEGs and `index.json` go.
    pub out: PathBuf,
    /// `kind` or `kind/name`: only those.
    pub only: Option<String>,
    /// Render what is missing or stale in `out` (otherwise use `out` as
    /// it is, to upload a render made earlier).
    pub render: bool,
    /// PUT to the server what it lacks or has from another render.
    pub upload: bool,
    /// The server, e.g. `http://localhost:3000`.
    pub api: String,
    /// Render and upload everything, stale or not.
    pub force: bool,
}

/// One picture to make: the API's name for it, and how.
struct Planned {
    kind: &'static str,
    name: String,
    how: How,
}

enum How {
    /// A Catalog entry's preview.
    Entry(usize),
    /// A preset, itself (the index in `load_embedded_presets`).
    Preset(usize),
}

/// An entry of `index.json`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Indexed {
    pub kind: String,
    pub name: String,
    pub source_hash: String,
    /// Relative to the output folder; empty for a blank one.
    pub file: String,
    /// The picture came out blank (all but black): not saved or uploaded,
    /// and listed so its Catalog preview can be fixed. Kept in the index so
    /// a later run renders it again only when its config changes.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub blank: bool,
}

/// Whether a picture is blank: under 0.2% of its pixels brighter than
/// near-black. A blank picture is worse than none -- a client draws its own
/// in place of a missing one.
fn blank(img: &image::RgbaImage) -> bool {
    let lit = img.pixels().filter(|p| p.0[0].max(p.0[1]).max(p.0[2]) > 24).count();
    lit * 500 < (img.width() * img.height()) as usize
}

/// The API's kind for an inventory kind, `None` for one with no
/// published form (a solid's colouring, a palette, ...).
fn api_kind(kind: Kind) -> Option<&'static str> {
    Some(match kind {
        Kind::Variation => "variation",
        Kind::Effect => "effect",
        Kind::Formula => "formula",
        Kind::Coloring => "coloring",
        Kind::Field => "field",
        Kind::FieldColoring => "field_coloring",
        Kind::Solid => "solid",
        Kind::SimModel => "sim_model",
        Kind::SimColoring => "sim_coloring",
        Kind::Generator => "generator",
        Kind::Modifier => "modifier",
        Kind::Track => "track",
        _ => return None,
    })
}

/// Every picture, named as the API knows it; built-ins first, so an
/// example named for its built-in yields to it.
fn plan(entries: &[Entry], only: Option<&str>) -> Vec<Planned> {
    let mut out: Vec<Planned> = Vec::new();
    let mut seen: std::collections::HashSet<(String, String)> = Default::default();
    let mut add = |p: Planned, out: &mut Vec<Planned>| {
        if seen.insert((p.kind.to_string(), p.name.to_ascii_lowercase())) {
            out.push(p);
        }
    };
    for (i, e) in entries.iter().enumerate() {
        if e.origin != crate::provenance::Origin::Builtin {
            continue;
        }
        if let (EntryKey::Registered(_, name), Some(kind)) = (&e.key, api_kind(e.kind)) {
            add(Planned { kind, name: name.clone(), how: How::Entry(i) }, &mut out);
        }
    }
    let manifest = crate::curated::manifest();
    for (record, origin) in manifest.records.iter().zip(&manifest.origins) {
        let name = record["name"].as_str().unwrap_or_default().to_string();
        match origin {
            crate::curated::Origin::Preset(index) => add(Planned { kind: "fractal", name, how: How::Preset(*index) }, &mut out),
            crate::curated::Origin::Script(file) => {
                // A shipped file the Catalog lists, of the record's kind (two
                // stores can ship one file name: the `kaleidoscope.rhai`
                // modifier and effect); or a built-in's twin,
                // which it does not (the built-in's entry stands for it:
                // `kaleidoscope.rhai`, `sim-gray_scott.rhai`) -- the same
                // picture.
                let stem = file.trim_end_matches(".rhai");
                let stem = stem.strip_prefix("sim-").unwrap_or(stem);
                let kind = record["kind"].as_str().unwrap_or_default();
                let found = entries
                    .iter()
                    .position(|e| matches!(&e.key, EntryKey::File { file: f, shipped: true, .. } if f == file) && api_kind(e.kind) == Some(kind))
                    .or_else(|| entries.iter().position(|e| matches!(&e.key, EntryKey::Registered(_, n) if n == stem) && api_kind(e.kind) == Some(kind)));
                if let Some((i, kind)) = found.and_then(|i| Some((i, api_kind(entries[i].kind)?))) {
                    add(Planned { kind, name, how: How::Entry(i) }, &mut out);
                }
            }
            // A twin's picture is its built-in's, planned above by its key.
            crate::curated::Origin::Builtin | crate::curated::Origin::Twin | crate::curated::Origin::Palette => {}
        }
    }
    out.retain(|p| match only {
        None => true,
        Some(o) => match o.split_once('/') {
            Some((k, n)) => p.kind == k && p.name.eq_ignore_ascii_case(n),
            None => p.kind == o,
        },
    });
    out
}

fn read_index(out: &Path) -> Vec<Indexed> {
    std::fs::read_to_string(out.join("index.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

/// Render, then upload, as `opts` says. Prints a line an item.
pub fn run(opts: Options) -> Result<(), String> {
    std::fs::create_dir_all(&opts.out).map_err(|e| format!("{}: {e}", opts.out.display()))?;
    let mut index = read_index(&opts.out);
    if opts.render {
        render(&opts, &mut index)?;
    }
    if opts.upload {
        upload(&opts, &index)?;
    }
    Ok(())
}

fn render(opts: &Options, index: &mut Vec<Indexed>) -> Result<(), String> {
    let library = crate::scene::palette::global_palette_library().read().map_err(|_| "the palette library")?.clone();
    let entries = crate::inventory::scan(&library, crate::inventory::Online::default());
    let planned = plan(&entries, opts.only.as_deref());
    let presets = crate::resources::presets::load_embedded_presets().map_err(|e| format!("the presets: {e}"))?;
    let ctx = egui::Context::default();
    let (device, queue) = pollster::block_on(crate::app::export::headless_device())?;
    device.on_uncaptured_error(std::sync::Arc::new(|e| log::error!("wgpu error in a preview: {e}")));
    println!("{} previews to check", planned.len());

    let (mut made, mut kept, mut none, mut blanks) = (0, 0, 0, 0);
    for p in &planned {
        let (config, key) = match &p.how {
            How::Preset(i) => {
                let Some(c) = presets.get(*i) else { continue };
                (c.clone(), crate::storage::thumbnail_cache::ThumbnailCache::config_hash(c))
            }
            How::Entry(i) => {
                let e = &entries[*i];
                if super::inventory::compile_pending(e) {
                    super::inventory::compile_requested();
                }
                let Some(preview) = super::inventory_preview::preview(e, &ctx, &[]) else {
                    println!("  none   {}/{}", p.kind, p.name);
                    none += 1;
                    continue;
                };
                // An effect's is its "after"; every other entry has one.
                let Some(config) = preview.configs.last().cloned() else { continue };
                (config, preview.key)
            }
        };
        let source_hash = format!("{EPOCH}:{key}");
        let file = format!("{}/{}.jpg", p.kind, p.name.to_ascii_lowercase());
        let path = opts.out.join(&file);
        let current = index
            .iter()
            .any(|x| x.kind == p.kind && x.name.eq_ignore_ascii_case(&p.name) && x.source_hash == source_hash && (x.blank || path.exists()));
        if current && !opts.force {
            kept += 1;
            continue;
        }
        let t = std::time::Instant::now();
        let img = pollster::block_on(crate::renderer::render_preview_at_async(&device, &queue, &config, SIZE));
        index.retain(|x| !(x.kind == p.kind && x.name.eq_ignore_ascii_case(&p.name)));
        if blank(&img) {
            let _ = std::fs::remove_file(&path);
            index.push(Indexed { kind: p.kind.to_string(), name: p.name.clone(), source_hash, file: String::new(), blank: true });
            blanks += 1;
            println!("  BLANK  {}/{}", p.kind, p.name);
            write_index(&opts.out, index)?;
            continue;
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let rgb = image::DynamicImage::ImageRgba8(img).to_rgb8();
        let mut bytes = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 92)
            .encode_image(&rgb)
            .map_err(|e| format!("{file}: {e}"))?;
        std::fs::write(&path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        index.push(Indexed { kind: p.kind.to_string(), name: p.name.clone(), source_hash, file, blank: false });
        made += 1;
        println!("  made   {}/{}  {} ms", p.kind, p.name, t.elapsed().as_millis());
        // Saved as it goes: an interrupted run keeps what it made.
        write_index(&opts.out, index)?;
    }
    write_index(&opts.out, index)?;
    println!("{made} made, {kept} current, {none} with no picture, {blanks} blank (not kept)");
    let all_blank: Vec<String> = index.iter().filter(|x| x.blank).map(|x| format!("{}/{}", x.kind, x.name)).collect();
    if !all_blank.is_empty() {
        println!("blank, to fix in the Catalog's previews ({}): {}", all_blank.len(), all_blank.join(", "));
    }
    Ok(())
}

fn write_index(out: &Path, index: &[Indexed]) -> Result<(), String> {
    let mut sorted = index.to_vec();
    sorted.sort_by(|a, b| (&a.kind, &a.name).cmp(&(&b.kind, &b.name)));
    let text = serde_json::to_string_pretty(&sorted).map_err(|e| e.to_string())?;
    std::fs::write(out.join("index.json"), text + "\n").map_err(|e| e.to_string())
}

fn upload(opts: &Options, index: &[Indexed]) -> Result<(), String> {
    let token = std::env::var("FFAPI_TOKEN").map_err(|_| "--upload needs FFAPI_TOKEN, an admin's token".to_string())?;
    let api = opts.api.trim_end_matches('/');
    let auth = format!("Bearer {token}");
    let listed: Vec<serde_json::Value> = ureq::get(&format!("{api}/api/v1/admin/previews"))
        .set("Authorization", &auth)
        .call()
        .map_err(|e| format!("the server's previews: {e}"))?
        .into_string()
        .map_err(|e| e.to_string())
        .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))?;
    let on_server: HashMap<(String, String), String> = listed
        .iter()
        .filter_map(|p| Some(((p["kind"].as_str()?.to_string(), p["name"].as_str()?.to_ascii_lowercase()), p["source_hash"].as_str()?.to_string())))
        .collect();
    let wanted = |x: &&Indexed| match opts.only.as_deref() {
        None => true,
        Some(o) => match o.split_once('/') {
            Some((k, n)) => x.kind == k && x.name.eq_ignore_ascii_case(n),
            None => x.kind == o,
        },
    };
    let (mut sent, mut same, mut failed) = (0, 0, 0);
    for x in index.iter().filter(|x| !x.blank).filter(wanted) {
        let there = on_server.get(&(x.kind.clone(), x.name.to_ascii_lowercase()));
        if there == Some(&x.source_hash) && !opts.force {
            same += 1;
            continue;
        }
        let bytes = match std::fs::read(opts.out.join(&x.file)) {
            Ok(b) => b,
            Err(e) => {
                println!("  missing {}: {e}", x.file);
                failed += 1;
                continue;
            }
        };
        let url = format!("{api}/api/v1/admin/previews/{}/{}?source_hash={}", x.kind, x.name, x.source_hash);
        match ureq::put(&url).set("Authorization", &auth).set("Content-Type", "image/jpeg").send_bytes(&bytes) {
            Ok(_) => {
                sent += 1;
                println!("  sent   {}/{}", x.kind, x.name);
            }
            Err(e) => {
                failed += 1;
                println!("  FAILED {}/{}: {e}", x.kind, x.name);
            }
        }
    }
    println!("{sent} uploaded, {same} already current, {failed} failed");
    if failed > 0 {
        return Err(format!("{failed} uploads failed"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every built-in type and every curated item the manifest names has a
    /// planned picture, under the manifest's name; nothing twice.
    #[test]
    fn the_plan_names_items_as_the_api_does() {
        let library = crate::scene::palette::global_palette_library().read().unwrap().clone();
        let entries = crate::inventory::scan(&library, crate::inventory::Online::default());
        let planned = plan(&entries, None);
        let has = |k: &str, n: &str| planned.iter().any(|p| p.kind == k && p.name == n);
        assert!(has("formula", "mandelbrot") && has("formula", "burning_ship"), "built-in types by key");
        assert!(has("variation", "linear") && has("effect", "vignette"));
        assert!(has("sim_model", "gray_scott") && has("generator", "basic_random"));
        let manifest = crate::curated::manifest();
        for (r, o) in manifest.records.iter().zip(&manifest.origins) {
            if matches!(o, crate::curated::Origin::Preset(_) | crate::curated::Origin::Script(_) | crate::curated::Origin::Twin) {
                let (k, n) = (r["kind"].as_str().unwrap(), r["name"].as_str().unwrap());
                assert!(has(k, n), "{k}/{n} has a planned picture");
            }
        }
        let mut seen = std::collections::HashSet::new();
        for p in &planned {
            assert!(seen.insert((p.kind, p.name.to_ascii_lowercase())), "{}/{} twice", p.kind, p.name);
        }
        let only = plan(&entries, Some("formula/Mandelbrot"));
        assert_eq!(only.len(), 1);
    }
}
