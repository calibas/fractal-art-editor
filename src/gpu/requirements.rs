//! What the renderer asks of a GPU: the limits and features every device
//! it creates requests, in one place. The app's device, the exports'
//! (high-res, video, the headless PNG), and the gallery module
//! (`wasm/render`) all build theirs here, so they cannot drift; they did,
//! twice -- a video export without FLOAT32_FILTERABLE dropped every
//! density effect, and an export at a buffer size past the limit it had
//! requested panicked.
//!
//! [`requirements_json`] publishes the same facts for a page that wants
//! to know, before it downloads the renderer, whether this device can run
//! it (the gallery module's `requirements()`).

use wgpu::{Features, Limits};

/// An optional feature: requested when the adapter has it, and what is
/// lost without it.
pub struct Optional {
    pub feature: Features,
    /// Its name in WebGPU (`GPUFeatureName`), for a page's own check.
    pub web_name: &'static str,
    /// What renders differently without it.
    pub without: &'static str,
}

/// Bilinear sampling of `Rgba32Float`: the density-effect chain samples
/// the accumulation, so without it the chain skips density effects
/// (`EffectChainRunner`) rather than build an invalid bind group.
pub const FLOAT32_FILTERABLE: Optional = Optional {
    feature: Features::FLOAT32_FILTERABLE,
    web_name: "float32-filterable",
    without: "density effects are skipped",
};

/// GPU timestamps: the escape renderer paces its chunks by measured GPU
/// time instead of inferring the cost from wall-clock gaps.
pub const TIMESTAMP_QUERY: Optional = Optional {
    feature: Features::TIMESTAMP_QUERY,
    web_name: "timestamp-query",
    without: "escape-time renders pace their chunks by wall-clock time",
};

/// The optional features the app and its exports ask for.
pub const APP_OPTIONAL: &[&Optional] = &[&FLOAT32_FILTERABLE, &TIMESTAMP_QUERY];

/// The optional features the gallery module and the high-res export ask
/// for: no GPU-time pacing.
pub const WITHOUT_TIMESTAMPS: &[&Optional] = &[&FLOAT32_FILTERABLE];

/// Required on every device. A native feature with no WebGPU name: wgpu
/// drops it from a browser's request, so a page needs no check for it.
pub const REQUIRED: Features = Features::CLEAR_TEXTURE;

/// Storage buffers one shader stage binds, at most: a flame's layout
/// holds only the buffers its shader uses (`gpu::pipelines::used_bindings`),
/// 8 with every optional one on -- WebGPU's own floor, so every WebGPU
/// adapter has them.
pub const STORAGE_BUFFERS_PER_STAGE: u32 = 8;

/// The limits to request from an adapter with `adapter`'s: the base set
/// (WebGPU's defaults natively; on the web the downlevel set, whose
/// zeroed compute limits a browser fills with its own defaults), raised to
/// the adapter's own for what a large render needs -- the storage-buffer
/// binding size (the histogram), the buffer size (a deep zoom's per-pixel
/// state), the texture size (the picture), and the storage buffers a
/// stage binds.
pub fn limits(adapter: &Limits) -> Limits {
    let mut limits = if cfg!(target_arch = "wasm32") { Limits::downlevel_webgl2_defaults() } else { Limits::default() };
    limits.max_storage_buffer_binding_size = adapter.max_storage_buffer_binding_size;
    limits.max_buffer_size = adapter.max_buffer_size;
    limits.max_texture_dimension_2d = adapter.max_texture_dimension_2d;
    limits.max_storage_buffers_per_shader_stage = adapter.max_storage_buffers_per_shader_stage;
    limits
}

/// The features to request from an adapter with `adapter`'s: the required
/// ones, and each of `optional` the adapter has.
pub fn features(adapter: Features, optional: &[&Optional]) -> Features {
    optional.iter().filter(|o| adapter.contains(o.feature)).fold(REQUIRED, |f, o| f | o.feature)
}

/// What a device with `features` renders differently, one line for each
/// of `optional` it lacks.
pub fn degraded(features: Features, optional: &[&Optional]) -> Vec<String> {
    optional.iter().filter(|o| !features.contains(o.feature)).map(|o| format!("no {}: {}", o.web_name, o.without)).collect()
}

/// What an adapter with `limits` cannot do that the renderer needs; empty
/// for any WebGPU adapter.
pub fn missing(limits: &Limits) -> Vec<String> {
    let mut out = Vec::new();
    if limits.max_storage_buffers_per_shader_stage < STORAGE_BUFFERS_PER_STAGE {
        out.push(format!(
            "maxStorageBuffersPerShaderStage is {}, and a flame binds up to {STORAGE_BUFFERS_PER_STAGE}",
            limits.max_storage_buffers_per_shader_stage
        ));
    }
    out
}

/// The requirements as a page reads them, in WebGPU's names, before it
/// loads the renderer: no required WebGPU feature, the optional ones and
/// what each changes, and the limits -- WebGPU's own floors, so any WebGPU
/// adapter qualifies. How large a picture a device renders is its own
/// `maxTextureDimension2D` (and storage-buffer sizes), which `probe()`
/// reports.
pub fn requirements_json(optional: &[&Optional]) -> serde_json::Value {
    let floor = Limits::default();
    serde_json::json!({
        "api": "webgpu",
        "features": {
            "required": [],
            "optional": optional.iter().map(|o| serde_json::json!({ "name": o.web_name, "without": o.without })).collect::<Vec<_>>(),
        },
        "limits": {
            "maxStorageBuffersPerShaderStage": STORAGE_BUFFERS_PER_STAGE,
            "maxStorageBufferBindingSize": floor.max_storage_buffer_binding_size,
            "maxBufferSize": floor.max_buffer_size,
            "maxTextureDimension2D": floor.max_texture_dimension_2d,
        },
        "largest_render": "the device's maxTextureDimension2D a side (probe() reports it)",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An adapter's larger limits are requested, not the base set's: the
    /// gallery module asked for 2048 px textures and refused anything
    /// larger on a GPU with 16384.
    #[test]
    fn the_adapters_own_sizes_are_requested() {
        let mut adapter = Limits::default();
        adapter.max_texture_dimension_2d = 16384;
        adapter.max_buffer_size = 1 << 34;
        adapter.max_storage_buffer_binding_size = 1 << 31;
        adapter.max_storage_buffers_per_shader_stage = 10;
        let l = limits(&adapter);
        assert_eq!(l.max_texture_dimension_2d, 16384);
        assert_eq!(l.max_buffer_size, 1 << 34);
        assert_eq!(l.max_storage_buffer_binding_size, 1 << 31);
        assert_eq!(l.max_storage_buffers_per_shader_stage, 10);
        assert!(missing(&adapter).is_empty());
        assert!(missing(&Limits::default()).is_empty(), "every WebGPU adapter qualifies");
    }

    /// Optional features are asked for where the adapter has them, and a
    /// device without one says what it loses.
    #[test]
    fn optional_features_follow_the_adapter() {
        let all = Features::FLOAT32_FILTERABLE | Features::TIMESTAMP_QUERY | Features::CLEAR_TEXTURE;
        assert_eq!(features(all, WITHOUT_TIMESTAMPS), REQUIRED | Features::FLOAT32_FILTERABLE);
        assert_eq!(features(all, APP_OPTIONAL), all);
        assert_eq!(features(Features::empty(), APP_OPTIONAL), REQUIRED);
        assert_eq!(degraded(REQUIRED, WITHOUT_TIMESTAMPS), vec!["no float32-filterable: density effects are skipped".to_string()]);
        let json = requirements_json(WITHOUT_TIMESTAMPS);
        assert_eq!(json["features"]["optional"][0]["name"], "float32-filterable");
        assert_eq!(json["limits"]["maxStorageBuffersPerShaderStage"], STORAGE_BUFFERS_PER_STAGE);
    }
}
