//! fae-render — the Endless Gallery's renderer.
//!
//! One call: a `FractalConfig` JSON string (exactly what the script
//! module's `config_json` field carries, or any `.fflame` file) plus
//! dimensions and an iteration budget, out come raw RGBA pixels for
//! `canvas.putImageData`. Internally this is the main crate's unified
//! headless render path, unchanged.
//!
//! Device lifecycle: **one device and one renderer, reused across
//! tiles.** This was per-render once, destroying the device each time,
//! and that is what a real gallery scroll exhausted — reported as
//! `requestDevice` itself failing with "Not enough memory left".
//!
//! The reason is specific to WebGPU: wgpu implements `Drop` for
//! `WebBuffer` and `WebTexture` as a **no-op**, so a finished renderer's
//! GPU memory survives until the JS garbage collector collects the
//! wrappers. `device.destroy()` is the only synchronous reclamation, and
//! it can only sweep a whole device — so a device per tile made
//! reclamation a race against the scroll, and the scroll won.
//!
//! Reuse removes the garbage rather than sweeping it faster.
//! `load_config` is a full reset point (transforms, variation params,
//! palette size, accumulation), so a new config needs no new renderer;
//! only a size change does, and that branch destroys the old one
//! explicitly. The shader cache surviving is a real bonus: consecutive
//! seeds of one generator usually share a variation set, so the compile
//! amortizes across the hallway.
//!
//! The same rule extends below the renderer: the per-render readback
//! staging buffer is persistent in `FlameRenderer` (grow-only,
//! re-mapped each read), and the effect chain in `render_with` is only
//! built when an effect is enabled and is destroyed after the pixel
//! read. Steady-state, a render creates **zero** GPU buffers — the
//! client measured 2,137 `createBuffer` calls and zero `destroy()`
//! across 1,000 renders before that, growing without bound because the
//! JS GC feels no GPU-memory pressure.
//!
//! Renders are serial. A second call while one is in flight gets an
//! error rather than sharing the renderer — see `render_impl`. `release()`
//! frees everything for a page that is done.
//!
//! **A render that cannot draw its config fails** rather than drawing
//! something else: a name this build has no engine entry for, or a
//! carried Rhai source that does not compile, is an error naming it (and
//! the source's first error), where it used to render the engine's
//! default (`unknown_names`) -- a Mandelbrot for a broken formula. A GPU
//! validation or out-of-memory error during the render is an error too,
//! caught by error scopes, where it used to come back as a blank image.
//!
//! The device asks for what every device the renderer makes asks for
//! (`fae::gpu::requirements`); `requirements()` publishes it for a page
//! deciding, before the download, whether to offer a live render.

use fae::config::defaults::DEFAULT_EXPORT_ITERATIONS_PER_THREAD;
use fae::config::FractalConfig;
use fae::renderer::compute_kernel::FlameRenderer;
use fae::renderer::render::{render_with, NoProgress, RenderJob};
use std::cell::RefCell;

/// The device, queue and renderer, kept alive across tiles.
///
/// Single-threaded by construction: wasm has one thread, and the render
/// entry point is `async` but not re-entrant — a second call while one is
/// in flight would find the `RefCell` already borrowed and error rather
/// than corrupt anything. The gallery renders in series.
struct Live {
    device: wgpu::Device,
    queue: wgpu::Queue,
    max_dim: u32,
    /// `None` until the first render; carries the size it was built for,
    /// because a dimension change needs `resize`, not just `load_config`.
    renderer: Option<(FlameRenderer, u32, u32)>,
}

/// Wrapper whose `Drop` deliberately **leaks** the device.
///
/// Dropping a wgpu `Device` reaches into other thread-locals. At thread
/// exit those may already have been destroyed, and the access panics —
/// inside a destructor, which Rust turns into an abort
/// (`STATUS_STACK_BUFFER_OVERRUN`). The native smoke test hit exactly
/// that the moment the device outlived a single call.
///
/// Leaking is the right answer rather than a dodge: this only runs when
/// the thread is ending, which on wasm means the page is being torn down
/// and the whole linear memory plus every GPU resource goes with it.
/// Callers wanting deterministic cleanup have `release()`, which runs
/// while the thread is alive and can drop safely.
struct LiveCell(RefCell<Option<Live>>);

impl Drop for LiveCell {
    fn drop(&mut self) {
        if let Ok(mut slot) = self.0.try_borrow_mut() {
            std::mem::forget(slot.take());
        }
    }
}

thread_local! {
    static LIVE: LiveCell = const { LiveCell(RefCell::new(None)) };
}

pub struct RenderedTile {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub iterations: u64,
    pub ms: f64,
}

/// The high-performance adapter, or an error saying WebGPU is missing.
async fn adapter() -> Result<wgpu::Adapter, String> {
    let backends = if cfg!(target_arch = "wasm32") {
        wgpu::Backends::BROWSER_WEBGPU
    } else {
        // Native builds exist for the smoke test only.
        wgpu::Backends::PRIMARY
    };
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });

    instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
        })
        .await
        .map_err(|e| format!("no GPU adapter (WebGPU unavailable?): {e:?}"))
}

/// The device on `adapter`, its queue, and its maximum 2D texture
/// dimension — the ceiling a render must respect.
async fn create_device(adapter: &wgpu::Adapter) -> Result<(wgpu::Device, wgpu::Queue, u32), String> {
    // What every device the renderer makes asks for
    // (`fae::gpu::requirements`): the adapter's own buffer and texture
    // sizes, FLOAT32_FILTERABLE where it has it. This module once asked
    // for the downlevel set's 2048 px textures and read its ceiling off
    // the request, refusing a 4096 px render on a GPU with 16384.
    let limits = fae::gpu::requirements::limits(&adapter.limits());
    let required_features =
        fae::gpu::requirements::features(adapter.features(), fae::gpu::requirements::WITHOUT_TIMESTAMPS);

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("Gallery Renderer Device"),
            required_features,
            required_limits: limits,
            memory_hints: wgpu::MemoryHints::Performance,
            experimental_features: Default::default(),
            trace: Default::default(),
        })
        .await
        .map_err(|e| format!("device request failed: {e:?}"))?;
    // The ceiling is what the device was given, never less than asked.
    let max_dim = device.limits().max_texture_dimension_2d;

    // wgpu's default handler PANICS on a validation error. In wasm a
    // panic poisons the module: every later call fails and the page has
    // to reload. This module promises `Result`, so route uncaptured
    // errors to the log and let the render return its own error
    // instead. (Dimensions are checked up front, but a config can be
    // hostile in ways no up-front check enumerates.)
    device.on_uncaptured_error(std::sync::Arc::new(|e| {
        log::error!("wgpu error (render will fail): {e}");
    }));

    Ok((device, queue, max_dim))
}

/// Probe for a usable adapter without rendering — lets a page fail
/// early with a clear message instead of on the first tile. An error
/// where there is no adapter or no device; otherwise what was found, as
/// JSON: `{ ok, adapter: { name, backend, device_type, driver },
/// max_dimension, features: [...], missing: [...], degraded: [...] }`
/// -- `features` the optional ones the device got (WebGPU names),
/// `missing` what the renderer needs and the adapter lacks (then `ok` is
/// false), `degraded` what renders differently without an optional one.
pub async fn probe_impl() -> Result<String, String> {
    use fae::gpu::requirements as req;
    let adapter = adapter().await?;
    let info = adapter.get_info();
    let missing = req::missing(&adapter.limits());
    let (device, _queue, max_dim) = create_device(&adapter).await?;
    let features = device.features();
    device.destroy();
    let present: Vec<&str> = req::WITHOUT_TIMESTAMPS.iter().filter(|o| features.contains(o.feature)).map(|o| o.web_name).collect();
    let report = serde_json::json!({
        "ok": missing.is_empty(),
        "adapter": {
            "name": info.name,
            "backend": format!("{:?}", info.backend),
            "device_type": format!("{:?}", info.device_type),
            "driver": info.driver,
        },
        "max_dimension": max_dim,
        "features": present,
        "missing": missing,
        "degraded": req::degraded(features, req::WITHOUT_TIMESTAMPS),
    });
    Ok(report.to_string())
}

/// Check a Rhai source of a GPU kind as the Formula Editor does before
/// Use, needing no GPU (`fae::storage::library::compile_json_gpu`): a
/// generator, modifier or track is the script module's to check, which
/// keeps the script host out of this module.
pub fn compile_impl(kind: &str, source: &str) -> String {
    fae::storage::library::compile_json_gpu(kind, source)
}

/// What the renderer asks of a GPU, as JSON in WebGPU's names, needing
/// no GPU: for a page's own check before it downloads this module
/// (`fae::gpu::requirements::requirements_json`).
pub fn requirements_impl() -> String {
    fae::gpu::requirements::requirements_json(fae::gpu::requirements::WITHOUT_TIMESTAMPS).to_string()
}

/// Upper bound on a single render's chaos-game budget.
///
/// A config carries its own `max_iterations`, and a config is a
/// shareable artifact — one asking for 5e11 ground for 90 seconds and
/// was still going when killed, which in a browser is a frozen tab or a
/// GPU reset. 8e9 is far above any sensible tile (the gallery uses 3e7)
/// while keeping a hostile file to seconds rather than forever.
const MAX_RENDER_ITERATIONS: u64 = 8_000_000_000;

/// Iterations-per-thread bounds. ipt is how many iterations each GPU
/// thread runs in ONE dispatch. Orbits persist between dispatches, so it
/// slices the budget rather than capping how deep an orbit gets — but
/// an unbounded value still turns a single dispatch into a
/// browser-watchdog-length stall the same way an unbounded iteration
/// budget would. Mirrors the CLI's documented range.
const MIN_ITERATIONS_PER_THREAD: u32 = 64;
const MAX_ITERATIONS_PER_THREAD: u32 = 4096;

pub async fn render_impl(
    config_json: &str,
    width: u32,
    height: u32,
    target_iterations: Option<u64>,
    iterations_per_thread: Option<u32>,
) -> Result<RenderedTile, String> {
    if width == 0 || height == 0 {
        return Err("width and height must be nonzero".into());
    }
    let mut config = FractalConfig::from_json(config_json)
        .map_err(|e| format!("config did not parse: {e}"))?;

    // A name the engines would replace with their default -- one this
    // build lacks, or a carried source that does not compile -- is not
    // this config's picture: refused, as the CLI refuses it, with the
    // source's first error. Needs no GPU, so it comes first.
    let unknown = fae::renderer::render::unknown_names(&config);
    if !unknown.is_empty() {
        return Err(format!("this config cannot be drawn: no {}", unknown.join(", no ")));
    }

    // Create the device once and keep it. Creating one per tile is what
    // exhausted the GPU: wgpu's WebGPU backend implements `Drop` for
    // buffers and textures as a NO-OP, so a finished renderer's memory
    // lives until the JS garbage collector runs, and `device.destroy()`
    // — the only synchronous reclamation — can just sweep a whole
    // device. A scroll of tiles created devices faster than the browser
    // reclaimed them, and eventually `requestDevice` itself failed with
    // "Not enough memory left".
    //
    // Reuse removes the garbage instead of sweeping it faster, and keeps
    // the shader cache warm: consecutive seeds of one generator usually
    // share a variation set, so the per-tile compile amortizes.
    if LIVE.with(|l| l.0.borrow().is_none()) {
        let (device, queue, max_dim) = create_device(&adapter().await?).await?;
        LIVE.with(|l| {
            *l.0.borrow_mut() = Some(Live { device, queue, max_dim, renderer: None });
        });
    }

    config.max_iterations = config.max_iterations.min(MAX_RENDER_ITERATIONS);
    let iters = target_iterations.map(|i| i.min(MAX_RENDER_ITERATIONS));

    // Take the whole `Live` out for the duration, put it back on every
    // path. That is also the re-entrancy guard — a second concurrent
    // call finds the slot empty and gets a clear error instead of two
    // renders sharing one renderer. The borrow is released before the
    // `await`, because holding a `RefCell` borrow across a yield point
    // is how a single-threaded async runtime deadlocks itself.
    let mut live = LIVE
        .with(|l| l.0.borrow_mut().take())
        .ok_or_else(|| {
            "a render is already in flight — this module renders one tile at              a time; await the previous call"
                .to_string()
        })?;

    // The CLI's and Python's default when the caller names none, so a
    // tile matches their pictures pixel for pixel.
    let ipt = iterations_per_thread
        .unwrap_or(DEFAULT_EXPORT_ITERATIONS_PER_THREAD)
        .clamp(MIN_ITERATIONS_PER_THREAD, MAX_ITERATIONS_PER_THREAD);

    // A validation or out-of-memory error anywhere in the render -- a
    // shader that does not compile, a buffer past a limit -- is this
    // render's error, not a log line beside a blank image. Popped in the
    // reverse order of pushing.
    let oom = live.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let validation = live.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let outcome = render_with_live(&mut live, &config, width, height, iters, ipt).await;
    let gpu_error = match validation.pop().await {
        Some(e) => Some(e),
        None => oom.pop().await,
    };

    LIVE.with(|l| *l.0.borrow_mut() = Some(live));
    match (gpu_error, outcome) {
        (Some(e), Err(render)) => Err(format!("the GPU refused the render: {e} ({render})")),
        (Some(e), Ok(_)) => Err(format!("the GPU refused the render: {e}")),
        (None, outcome) => outcome,
    }
}

async fn render_with_live(
    live: &mut Live,
    config: &FractalConfig,
    width: u32,
    height: u32,
    target_iterations: Option<u64>,
    iterations_per_thread: u32,
) -> Result<RenderedTile, String> {
    // Build the renderer on first use, or resize it if the tile geometry
    // changed. `load_config` is a full reset for everything else —
    // transforms, variation params, palette size, accumulation — so the
    // renderer does not need to have been built for THIS config, only
    // for this size.
    if width > live.max_dim || height > live.max_dim {
        return Err(format!(
            "{width}x{height} exceeds this device's maximum texture dimension of {}",
            live.max_dim
        ));
    }

    let needs_new = match &live.renderer {
        None => true,
        Some((_, w, h)) => *w != width || *h != height,
    };
    if needs_new {
        if let Some((old, _, _)) = live.renderer.take() {
            // Explicit, because dropping frees nothing on WebGPU.
            old.destroy();
        }
        let renderer = FlameRenderer::with_palette_size(
            &live.device,
            &live.queue,
            wgpu::TextureFormat::Rgba8Unorm,
            width,
            height,
            &config.flame,
            config.palette_size,
        );
        live.renderer = Some((renderer, width, height));
    }

    let (renderer, _, _) = live.renderer.as_mut().expect("built above");

    let mut job = RenderJob::new(config, width, height);
    if let Some(iters) = target_iterations {
        job = job.with_iterations(iters);
    }
    job = job.with_iterations_per_thread(iterations_per_thread);

    let out = render_with(renderer, &live.device, &live.queue, job, &mut NoProgress)
        .await
        .map_err(|e| e.to_string())?;

    Ok(RenderedTile {
        pixels: out.rgba_data,
        width: out.width,
        height: out.height,
        iterations: out.total_iterations,
        ms: out.render_time_ms,
    })
}

/// Release the device and everything on it.
///
/// Optional — the module works without it — but a page that is done
/// rendering (navigating away, closing the gallery) can free the GPU
/// memory immediately rather than waiting for the JS garbage collector
/// to notice the wrappers.
pub fn release_impl() {
    LIVE.with(|l| {
        if let Some(live) = l.0.borrow_mut().take() {
            if let Some((renderer, _, _)) = live.renderer {
                renderer.destroy();
            }
            live.device.destroy();
        }
    });
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(start)]
    pub fn start() {
        console_error_panic_hook::set_once();
        // Errors to the console from the start (a GPU error outside a
        // render's scopes, a source that did not compile); more with
        // `set_log_level`.
        if console_log::init_with_level(log::Level::Trace).is_ok() {
            log::set_max_level(log::LevelFilter::Error);
        }
    }

    /// How much the module logs to the console: "off", "error" (the
    /// default), "warn" (a source that did not compile, with every
    /// diagnostic), "info", "debug" or "trace".
    #[wasm_bindgen]
    pub fn set_log_level(level: &str) -> Result<(), JsValue> {
        let filter = match level.to_ascii_lowercase().as_str() {
            "off" => log::LevelFilter::Off,
            "error" => log::LevelFilter::Error,
            "warn" => log::LevelFilter::Warn,
            "info" => log::LevelFilter::Info,
            "debug" => log::LevelFilter::Debug,
            "trace" => log::LevelFilter::Trace,
            other => return Err(JsValue::from_str(&format!("unknown log level `{other}`: off, error, warn, info, debug or trace"))),
        };
        log::set_max_level(filter);
        Ok(())
    }

    /// Check a Rhai source as the desktop Formula Editor does before
    /// Use, without rendering: `kind` the API's name for it
    /// ("formula", "coloring", "field", "field_coloring", "solid",
    /// "sim_model", "sim_coloring", "variation", "effect"), or "" to read
    /// it from the source. JSON: `{ ok, kind, title, errors: [{ line,
    /// column, message, stage }], warnings, parameters }` -- `parameters`
    /// in the API's shape, what a script published from the source
    /// carries. A generator, modifier or track is the script module's
    /// `compile()` (this module does not carry the script host): here it
    /// is a `"kind"` error.
    #[wasm_bindgen]
    pub fn compile(kind: &str, source: &str) -> String {
        crate::compile_impl(kind, source)
    }

    /// What the renderer asks of a GPU, as JSON in WebGPU's names
    /// (`{ api, features: { required, optional: [{ name, without }] },
    /// limits: {...} }`). Needs no GPU: a page's check before it offers a
    /// live render.
    #[wasm_bindgen]
    pub fn requirements() -> String {
        crate::requirements_impl()
    }

    /// A finished tile. `pixels` is RGBA8, `width * height * 4` bytes —
    /// feed it to `new ImageData(new Uint8ClampedArray(...), w, h)`.
    #[wasm_bindgen]
    pub struct RenderResult {
        inner: crate::RenderedTile,
    }

    #[wasm_bindgen]
    impl RenderResult {
        #[wasm_bindgen(getter)]
        pub fn pixels(&self) -> js_sys::Uint8Array {
            js_sys::Uint8Array::from(&self.inner.pixels[..])
        }
        #[wasm_bindgen(getter)]
        pub fn width(&self) -> u32 {
            self.inner.width
        }
        #[wasm_bindgen(getter)]
        pub fn height(&self) -> u32 {
            self.inner.height
        }
        #[wasm_bindgen(getter)]
        pub fn iterations(&self) -> f64 {
            self.inner.iterations as f64
        }
        #[wasm_bindgen(getter)]
        pub fn ms(&self) -> f64 {
            self.inner.ms
        }
    }

    /// Fail early if WebGPU is unavailable, with a clear message; where
    /// it is, what was found, as JSON (`{ ok, adapter, max_dimension,
    /// features, missing, degraded }`; see `probe_impl`).
    #[wasm_bindgen]
    pub async fn probe() -> Result<String, JsValue> {
        crate::probe_impl().await.map_err(|e| JsValue::from_str(&e))
    }

    /// Render a config. `iterations` caps the chaos-game budget
    /// (defaults to the config's own `max_iterations`).
    /// `iterations_per_thread` is how many iterations each GPU thread
    /// runs per dispatch (default 1024, the CLI's and Python's; clamped
    /// to 64–4096). Orbits
    /// persist across dispatches and burn-in is paid once, so it slices
    /// the budget rather than setting orbit depth: the picture does not
    /// depend on it, though the exact pixels do (each dispatch seeds its
    /// own random streams). Larger means fewer, longer dispatches.
    #[wasm_bindgen]
    pub async fn render(
        config_json: &str,
        width: u32,
        height: u32,
        iterations: Option<f64>,
        iterations_per_thread: Option<u32>,
    ) -> Result<RenderResult, JsValue> {
        let tile = crate::render_impl(
            config_json,
            width,
            height,
            iterations.map(|i| i as u64),
            iterations_per_thread,
        )
        .await
        .map_err(|e| JsValue::from_str(&e))?;
        Ok(RenderResult { inner: tile })
    }

    /// Release the GPU device and everything on it.
    ///
    /// Optional. The module holds one device across tiles, which is what
    /// keeps GPU memory flat during a long scroll; call this when the
    /// page is done rendering (navigating away, closing the gallery) to
    /// free it immediately instead of waiting for the JS garbage
    /// collector to notice. The next `render` transparently builds a new
    /// device, so calling it early costs a device request, not a bug.
    #[wasm_bindgen]
    pub fn release() {
        crate::release_impl();
    }
}
