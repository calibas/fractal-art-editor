//! One library over the user's two stores (docs/archive/projects/rhai-plugins.md,
//! P3): the CPU scripts (`scripts/`, [`crate::script::store`]) and the
//! GPU formulas, colourings, variations, solids and effects
//! (`formulas/`, [`crate::storage::formula_store`]), with what ships of
//! each. The two folders stay -- UF files live in `formulas/` -- and each
//! store keeps its own file rules; what is one is what a person reads of
//! an entry, its link record ([`crate::storage::links`]) and the scan.

use crate::storage::links::Link;

/// Which store an entry is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Store {
    /// The CPU scripts: generators, modifiers, libraries.
    Scripts,
    /// The GPU language's files: Rhai, UF, Fractint.
    Formulas,
}

/// What an entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Generator,
    Modifier,
    ScriptLibrary,
    Formula,
    Coloring,
    Variation,
    Solid,
    Effect,
    /// A simulation's colouring (P4).
    SimColoring,
    /// A simulation's model (P4).
    SimModel,
    /// A mode B field, and a field's colouring
    /// (docs/archive/projects/rhai-solids-fields.md).
    Field,
    FieldColoring,
    /// An animation track (docs/archive/projects/rhai-tracks.md).
    Track,
    /// An Ultra Fractal or Fractint file: many entries, of its kind.
    UfFormulas,
    UfColorings,
    Fractint,
    /// A file whose kind could not be read (it does not parse).
    Unknown,
}

impl Kind {
    /// A GPU Rhai entry that can be a plug-in (`storage::links::Link::plugin`).
    pub fn can_be_plugin(self) -> bool {
        matches!(self, Kind::Variation | Kind::Effect | Kind::Formula | Kind::Coloring | Kind::Solid | Kind::SimColoring | Kind::SimModel | Kind::Field | Kind::FieldColoring | Kind::Track)
    }
}

impl Kind {
    /// What the server's scripts resource calls it (ffapi's `script_kind`,
    /// the app's spellings: docs/archive/projects/rhai-scripts-online.md, decision
    /// 1). `None` for what is not published: a UF or Fractint file, one
    /// whose kind could not be read.
    pub fn wire(self) -> Option<&'static str> {
        Some(match self {
            Kind::Generator => "generator",
            Kind::Modifier => "modifier",
            Kind::ScriptLibrary => "library",
            Kind::Formula => "formula",
            Kind::Coloring => "coloring",
            Kind::Variation => "variation",
            Kind::Solid => "solid",
            Kind::Effect => "effect",
            Kind::SimColoring => "sim_coloring",
            Kind::SimModel => "sim_model",
            Kind::Field => "field",
            Kind::FieldColoring => "field_coloring",
            Kind::Track => "track",
            Kind::UfFormulas | Kind::UfColorings | Kind::Fractint | Kind::Unknown => return None,
        })
    }

    /// The kind the server names `wire`; `None` for one this build does
    /// not know (a newer server's).
    pub fn from_wire(wire: &str) -> Option<Kind> {
        ALL_PUBLISHED.iter().copied().find(|k| k.wire() == Some(wire))
    }

    /// A kind the formula library holds: one Rhai entry, published under
    /// its own `INFO.id`. The rest are the CPU scripts.
    pub fn is_entry(self) -> bool {
        self.can_be_plugin()
    }
}

/// Every kind the scripts resource stores, in the order a kind filter
/// offers them.
pub const ALL_PUBLISHED: &[Kind] = &[
    Kind::Generator,
    Kind::Modifier,
    Kind::ScriptLibrary,
    Kind::Formula,
    Kind::Coloring,
    Kind::Solid,
    Kind::Field,
    Kind::FieldColoring,
    Kind::Variation,
    Kind::Effect,
    Kind::SimModel,
    Kind::SimColoring,
    Kind::Track,
];

/// What a Rhai file of the formula store holds, by its functions
/// ([`crate::lang::rhai::kind_of`]), a track by its shape.
pub fn rhai_kind(source: &str) -> Kind {
    use crate::lang::{rhai, EntryKind};
    // A track first: a stateful one has `start` and `step` as a formula
    // does, but only a track declares `TARGETS` and a `value` function.
    if crate::animation::script::looks_like_track(source) {
        return Kind::Track;
    }
    match rhai::kind_of(source) {
        Some(EntryKind::Formula) => Kind::Formula,
        Some(EntryKind::Coloring) => Kind::Coloring,
        Some(EntryKind::Transform) => Kind::Variation,
        Some(EntryKind::Solid) => Kind::Solid,
        Some(EntryKind::Effect) => Kind::Effect,
        Some(EntryKind::SimColoring) => Kind::SimColoring,
        Some(EntryKind::SimModel) => Kind::SimModel,
        Some(EntryKind::Field) => Kind::Field,
        Some(EntryKind::FieldColoring) => Kind::FieldColoring,
        None => Kind::Unknown,
    }
}

/// Where an entry came from (design rule 7's namespaces, D6): the one
/// origin model every kind shares.
pub use crate::provenance::Origin;

/// One file of the library, as a person reads it.
#[derive(Debug, Clone)]
pub struct Entry {
    pub store: Store,
    /// Its file name in the store (a script's stem with `.rhai`), or a
    /// shipped one's name.
    pub file: String,
    pub kind: Kind,
    /// In the current language; the file's name when it declares none.
    pub title: String,
    pub description: String,
    pub author: String,
    /// `INFO.id`, when it has one.
    pub id: Option<String>,
    /// The first 8 hex of the source's SHA-256 (`scripted::version_of`).
    pub version: String,
    pub origin: Origin,
    pub link: Link,
    pub source: String,
}

/// Every entry of both stores, the shipped ones first, each group by
/// title.
pub fn scan() -> Vec<Entry> {
    let mut out = Vec::new();
    for (name, source) in crate::script::library::EMBEDDED {
        out.push(script_entry(name, source, Origin::Builtin, Link::default()));
    }
    for (name, source) in crate::storage::formula_store::EXAMPLES {
        out.push(formula_entry(name, source, Origin::Builtin, Link::default()));
    }
    for (name, source) in crate::effects::custom::BUILTIN_TWINS {
        out.push(formula_entry(&format!("{name}.rhai"), source, Origin::Builtin, Link::default()));
    }
    for (name, source) in crate::sim::custom::SHIPPED.iter().chain(crate::sim::custom::SHIPPED_MODELS) {
        out.push(formula_entry(&format!("sim-{name}.rhai"), source, Origin::Builtin, Link::default()));
    }
    let script_links = crate::script::store::load_links();
    for (stem, source) in crate::script::store::list() {
        let link = script_links.get(&stem).cloned().unwrap_or_default();
        out.push(script_entry(&format!("{stem}.rhai"), &source, origin_of_link(&link), link));
    }
    let formula_links = crate::storage::formula_store::links();
    for file in crate::storage::formula_store::list() {
        let Some(source) = crate::storage::formula_store::load(&file) else { continue };
        let link = formula_links.get(&file).cloned().unwrap_or_default();
        out.push(formula_entry(&file, &source, origin_of_link(&link), link));
    }
    out.sort_by_key(|e| (e.origin != Origin::Builtin, e.store == Store::Formulas, e.title.to_lowercase()));
    out
}

/// What a list shows after an entry's title -- the Scripts panel's and
/// the Formula Editor's alike (P3, step 4): the origin's mark
/// ([`Origin::mark`]).
pub fn marks(origin: Origin) -> String {
    origin.mark().to_string()
}

/// A stored file's origin, from its link.
pub fn origin_of_link(link: &Link) -> Origin {
    if link.from_others {
        Origin::Api
    } else {
        Origin::Local
    }
}

fn version(source: &str) -> String {
    crate::scripted::version_of(source)[..8].to_string()
}

fn stem(file: &str) -> String {
    file.rsplit_once('.').map_or(file, |(s, _)| s).to_string()
}

/// A CPU script, read from its `script("Name", "kind")` declaration by
/// parsing alone: running each script to learn it costs ~5 ms
/// (`script::library::discover`), which a scan of the library must not.
fn script_entry(file: &str, source: &str, origin: Origin, link: Link) -> Entry {
    let (name, kind) = crate::script::library::declared(source).unwrap_or_else(|| (String::new(), crate::script::ScriptKind::Generator));
    let kind = match kind {
        crate::script::ScriptKind::Generator => Kind::Generator,
        crate::script::ScriptKind::Modifier => Kind::Modifier,
        crate::script::ScriptKind::Library => Kind::ScriptLibrary,
    };
    Entry {
        store: Store::Scripts,
        file: file.to_string(),
        kind,
        title: if name.is_empty() { stem(file) } else { name },
        // The header comment's first paragraph, as the Scripts panel
        // shows it (`script::parse_doc`).
        description: crate::script::parse_doc(source).summary,
        author: String::new(),
        id: None,
        version: version(source),
        origin,
        link,
        source: source.to_string(),
    }
}

/// A shipped Rhai file the stores do not list (a built-in's escape twin,
/// `escape::twins`), read as a formula store file is.
pub fn shipped_entry(file: &str, source: &str) -> Entry {
    formula_entry(file, source, Origin::Builtin, Link::default())
}

/// A formula store file: a Rhai one read for its kind and `INFO`, a UF or
/// Fractint one by its extension.
fn formula_entry(file: &str, source: &str, origin: Origin, link: Link) -> Entry {
    use crate::storage::formula_store::{is_coloring_file, is_fractint, is_rhai};
    let mut e = Entry {
        store: Store::Formulas,
        file: file.to_string(),
        kind: Kind::Unknown,
        title: stem(file),
        description: String::new(),
        author: String::new(),
        id: None,
        version: version(source),
        origin,
        link,
        source: source.to_string(),
    };
    if !is_rhai(file) {
        e.kind = if is_fractint(file) {
            Kind::Fractint
        } else if is_coloring_file(file) {
            Kind::UfColorings
        } else {
            Kind::UfFormulas
        };
        return e;
    }
    use crate::lang::rhai;
    e.kind = rhai_kind(source);
    if let Some(labels) = rhai::source_labels(source) {
        let title = labels.title.resolve();
        if !title.is_empty() {
            e.title = title;
        }
        e.description = labels.description.resolve();
        e.author = labels.author;
    }
    e.id = rhai::info_string(source, "id");
    e
}

// ============================================================================
// Compiled files (docs/projects/one-list.md; once the plug-ins, P3 step 2)
// ============================================================================

/// A GPU Rhai file of the formula store -- a shipped example or a file of
/// the user's -- compiled: registered in its kind's registry, its source as
/// a config carries it. Every usable file is compiled when a list first
/// needs it (L1: there is no plug-in flag; any file of a pickable kind is
/// offered), once per version.
#[derive(Debug, Clone)]
pub struct Plugin {
    pub kind: Kind,
    /// Its file in the formula store (a shipped example's name, when
    /// `shipped`).
    pub file: String,
    /// A shipped example, not a file of the user's.
    pub shipped: bool,
    pub title: String,
    /// What it registered as, which a config names: a variation's or an
    /// effect's key, an escape slot's name.
    pub key: String,
    /// Its source as a config carries it, under `key`: choosing it from a
    /// list puts this in the config, as the Formula Editor's Use does.
    pub carried: crate::config::escape::CustomSource,
}

fn registered() -> &'static std::sync::RwLock<Vec<Plugin>> {
    static P: std::sync::OnceLock<std::sync::RwLock<Vec<Plugin>>> = std::sync::OnceLock::new();
    P.get_or_init(Default::default)
}

/// What each `(shipped, file, version)` compiled to, a failure included,
/// so neither is repeated every frame.
type CompileCache = std::collections::HashMap<(bool, String, String), Result<Plugin, String>>;

fn compiled_cache() -> &'static std::sync::Mutex<CompileCache> {
    static C: std::sync::OnceLock<std::sync::Mutex<CompileCache>> = std::sync::OnceLock::new();
    C.get_or_init(Default::default)
}

/// The files compiled this session, in the order they compiled.
pub fn plugins() -> Vec<Plugin> {
    registered().read().map(|p| p.clone()).unwrap_or_default()
}

/// The files of a kind compiled this session.
pub fn plugins_of(kind: Kind) -> Vec<Plugin> {
    plugins().into_iter().filter(|p| p.kind == kind).collect()
}

/// Whether `key` was registered by compiling a file (what a list offers as
/// that file, not as the registry's own entry).
pub fn is_file_key(key: &str) -> bool {
    registered().read().is_ok_and(|all| all.iter().any(|p| p.key == key))
}

/// A file compiled: from the cache by `(shipped, file, version)`, or
/// compiled now from `source()` and kept, a failure too. A file that
/// needs an id of its own to register (an effect, a variation without a
/// name) gets one derived from its name -- the same every session --
/// without the file being rewritten.
/// What [`compile_file`] made of a file at a version, if it was asked
/// already: never compiles (so it never takes a registry's lock).
pub fn compiled_cached(file: &str, shipped: bool, version: &str) -> Option<Result<Plugin, String>> {
    let key = (shipped, file.to_string(), version.to_string());
    compiled_cache().lock().ok()?.get(&key).cloned()
}

pub fn compile_file(file: &str, shipped: bool, version: &str, source: impl FnOnce() -> Option<String>) -> Result<Plugin, String> {
    let key = (shipped, file.to_string(), version.to_string());
    if let Some(done) = compiled_cache().lock().ok().and_then(|c| c.get(&key).cloned()) {
        return done;
    }
    let result = source().ok_or_else(|| format!("`{file}` could not be read")).and_then(|text| {
        let needs_id = match rhai_kind(&text) {
            Kind::Variation => crate::variations::custom::needs_id(&text),
            Kind::Effect => crate::api::entries::needs_id(&text),
            _ => false,
        };
        let text = if needs_id {
            let owner = if shipped { "shipped" } else { "own" };
            crate::lang::rhai::with_own_id(&text, &crate::scripted::derived_id(&format!("{owner}:{file}")))
        } else {
            text
        };
        let mut plugin = prepare_plugin(file, &text)?;
        plugin.shipped = shipped;
        Ok(plugin)
    });
    if let Ok(plugin) = &result {
        if let Ok(mut all) = registered().write() {
            all.retain(|p| !(p.file == plugin.file && p.shipped == plugin.shipped));
            all.push(plugin.clone());
        }
    }
    if let Ok(mut cache) = compiled_cache().lock() {
        cache.insert(key, result.clone());
    }
    result
}

/// What a file makes -- registered in its kind's registry, its source as a
/// config carries it -- without keeping it: the Catalog's preview of a file
/// goes through [`compile_file`] instead, which keeps it.
pub fn prepare_plugin(file: &str, source: &str) -> Result<Plugin, String> {
    let entry = formula_entry(file, source, Origin::Local, Link::default());
    if !entry.kind.can_be_plugin() {
        return Err("only a Rhai variation, effect, formula, colouring, solid or track can be used from a list".to_string());
    }
    let (key, carried) = register_kind(entry.kind, &entry.title, source)?;
    Ok(Plugin { kind: entry.kind, file: file.to_string(), shipped: false, title: entry.title, key, carried })
}

/// Forget what a file of the user's compiled to (it was renamed, edited or
/// deleted). What it registered stays registered until the app restarts --
/// a config may already be using it -- but the lists no longer offer it.
pub fn forget(file: &str) {
    if let Ok(mut all) = registered().write() {
        all.retain(|p| !(p.file == file && !p.shipped));
    }
    if let Ok(mut cache) = compiled_cache().lock() {
        cache.retain(|(shipped, f, _), _| *shipped || f != file);
    }
}

/// A Rhai file's parameters as its kind takes them -- what the app draws
/// for it once it is used -- read without registering anything: a GPU
/// kind's from its compiled program (`ir::Program::param_defs`, a complex
/// parameter split in two as the kind splits it: `c.re`, `c.im`; a
/// variation's `c_re`), a track's `PARAMS`, a CPU script's declarations
/// (collected, not run). What a published script carries as `parameters`
/// (`params::api_json` each; `curated::manifest`, the Scripts panel's
/// upload). A library declares none.
pub fn param_defs(kind: Kind, source: &str) -> Result<Vec<crate::params::ParamDef>, String> {
    use crate::lang::EntryKind;
    let entry = match kind {
        Kind::Generator | Kind::Modifier => return crate::script::headless::param_defs(source),
        Kind::Track => return crate::animation::script::declare(source).map(|d| d.params),
        Kind::Formula => EntryKind::Formula,
        Kind::Coloring => EntryKind::Coloring,
        Kind::Variation => EntryKind::Transform,
        Kind::Solid => EntryKind::Solid,
        Kind::Effect => EntryKind::Effect,
        Kind::SimColoring => EntryKind::SimColoring,
        Kind::SimModel => EntryKind::SimModel,
        Kind::Field => EntryKind::Field,
        Kind::FieldColoring => EntryKind::FieldColoring,
        Kind::ScriptLibrary | Kind::UfFormulas | Kind::UfColorings | Kind::Fractint | Kind::Unknown => return Ok(Vec::new()),
    };
    let src = crate::config::escape::CustomSource {
        dialect: crate::config::escape::CustomDialect::Rhai,
        entry: String::new(),
        source: source.to_string(),
        variations: Vec::new(),
    };
    let program = crate::escape::custom::check_entry(entry, &src).map_err(|d| {
        d.iter()
            .find(|x| x.severity == crate::lang::Severity::Error)
            .map(|x| x.render(source))
            .unwrap_or_else(|| "it does not compile".to_string())
    })?;
    program_params(kind, &program, source)
}

/// A GPU kind's parameters from its checked program, as [`param_defs`]
/// reads them.
fn program_params(kind: Kind, program: &crate::lang::ir::Program, source: &str) -> Result<Vec<crate::params::ParamDef>, String> {
    use crate::lang::ir::ComplexParams;
    match kind {
        Kind::Variation => crate::variations::custom::param_defs(program, source),
        Kind::Effect | Kind::SimModel | Kind::SimColoring => program.param_defs(ComplexParams::Refuse("its parameters are real")),
        _ => program.param_defs(ComplexParams::Split(".")),
    }
}

/// [`param_defs`] as the API spells a script's `parameters`: each in the
/// variations' and effects' shape (`params::api_json`), with a text's or
/// a colour's default as itself (a string, `[r, g, b]`), which the
/// float-only shape cannot hold.
pub fn api_parameters(kind: Kind, source: &str) -> Result<Vec<serde_json::Value>, String> {
    Ok(param_defs(kind, source)?.iter().map(api_parameter).collect())
}

/// One parameter of [`api_parameters`].
pub fn api_parameter(d: &crate::params::ParamDef) -> serde_json::Value {
    use crate::params::ParamKind;
    let mut v = crate::params::api_json(d);
    match &d.kind {
        ParamKind::Text { default, .. } => v["default_value"] = default.to_string().into(),
        ParamKind::Color { default } => v["default_value"] = serde_json::json!(default),
        _ => {}
    }
    v
}

/// One thing a [`check`] found wrong, where in the source when it is
/// known (1-based), and at which stage: `"kind"` (the source is another
/// kind than the one asked for), `"check"` (the language: parsing, names,
/// types), `"build"` (the kind's own compile: a variation's, an effect's
/// or a simulation's module, a solid's walk, a colouring that colours
/// nothing).
#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub message: String,
    pub stage: &'static str,
}

impl Problem {
    fn at(source: &str, d: &crate::lang::Diag, stage: &'static str) -> Self {
        let (line, column) = crate::lang::line_col(source, d.span.start);
        Problem { line: Some(line), column: Some(column), message: d.message.clone(), stage }
    }

    /// From a message the compilers render as `line:col: error: text`, or
    /// a plain one.
    fn rendered(message: &str, stage: &'static str) -> Self {
        let parsed = (|| {
            let (line, rest) = message.split_once(':')?;
            let (column, rest) = rest.split_once(':')?;
            let text = rest.trim_start().strip_prefix("error:").unwrap_or(rest).trim_start();
            Some((line.trim().parse().ok()?, column.trim().parse().ok()?, text.to_string()))
        })();
        match parsed {
            Some((line, column, text)) => Problem { line: Some(line), column: Some(column), message: text, stage },
            None => Problem { line: None, column: None, message: message.to_string(), stage },
        }
    }

    /// As the Formula Editor lists it: `line:col: error: text`, or the
    /// text alone without a position.
    pub fn render(&self) -> String {
        match (self.line, self.column) {
            (Some(l), Some(c)) => format!("{l}:{c}: error: {}", self.message),
            _ => self.message.clone(),
        }
    }
}

/// What [`check`] found: the source's kind (as asked for, else as read),
/// its title, its problems, and -- when there are none -- its parameters
/// as a published script carries them ([`api_parameters`]).
#[derive(Debug, Clone)]
pub struct Checked {
    pub kind: Kind,
    pub title: Option<String>,
    pub errors: Vec<Problem>,
    pub parameters: Vec<serde_json::Value>,
}

impl Checked {
    /// The report as JSON, the gallery modules' `compile()`:
    /// `{ ok, kind, title, errors: [{ line, column, message, stage }],
    /// warnings: [], parameters: [...] }`.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "ok": self.errors.is_empty(),
            "kind": self.kind.wire(),
            "title": self.title,
            "errors": self.errors.iter().map(|p| serde_json::json!({
                "line": p.line, "column": p.column, "message": p.message, "stage": p.stage,
            })).collect::<Vec<_>>(),
            "warnings": [],
            "parameters": self.parameters,
        })
    }
}

/// The CPU scripts' half of a check: a generator's or modifier's
/// parameters collected (not run), a track run once as the editor checks
/// it; its title, when it has one, and its parameters. A check is handed
/// it by a caller that carries the script host ([`script_check`]);
/// without it (`None`) a check needs none of that code -- the renderer
/// module's, which would otherwise carry the script host and the track
/// runner for kinds it never draws.
pub type ScriptCheck = fn(Kind, &str) -> Result<(Option<String>, Vec<crate::params::ParamDef>), String>;

/// [`ScriptCheck`] by the script host and the track runner.
pub fn script_check(kind: Kind, source: &str) -> Result<(Option<String>, Vec<crate::params::ParamDef>), String> {
    match kind {
        Kind::Track => crate::animation::script::try_run(source).map(|d| (Some(d.title), d.params)),
        Kind::Generator | Kind::Modifier => crate::script::headless::param_defs(source).map(|p| (None, p)),
        _ => Ok((None, Vec::new())),
    }
}

/// Check a Rhai source as the Formula Editor checks it before Use: the
/// language, then what its kind builds -- everything short of rendering
/// -- and, when there is nothing wrong, its parameters. `expected` is the
/// kind it is to be (a script's published kind); `None` reads it from the
/// source. Registers nothing, so it can run on every keystroke. A shader
/// the compiler produced that a GPU refuses is not found here (the web
/// build has no WGSL validator): a render reports that.
pub fn check(source: &str, expected: Option<Kind>) -> Checked {
    check_with(source, expected, Some(script_check), true)
}

/// [`check`] without the parameters: what the Formula Editor lists as it
/// is typed in.
pub fn check_only(source: &str, expected: Option<Kind>) -> Checked {
    check_with(source, expected, Some(script_check), false)
}

/// [`check`] for a page, the script module's `compile(kind, source)`:
/// `kind` a published kind's name (`"formula"`, `"field_coloring"`,
/// `"generator"` ... as the API spells them), or empty to read it from
/// the source. The report as JSON ([`Checked::to_json`]); a kind this
/// build does not know is a report too, not an error.
pub fn compile_json(kind: &str, source: &str) -> String {
    compile_json_with(kind, source, Some(script_check))
}

/// [`compile_json`] for the GPU kinds alone, the renderer module's: a
/// generator, modifier or track is the script module's to check, and
/// leaving them out keeps the script host out of the renderer.
pub fn compile_json_gpu(kind: &str, source: &str) -> String {
    compile_json_with(kind, source, None)
}

fn compile_json_with(kind: &str, source: &str, scripts: Option<ScriptCheck>) -> String {
    let expected = match kind.trim() {
        "" => None,
        wire => match Kind::from_wire(wire) {
            Some(k) => Some(k),
            None => {
                let known: Vec<&str> = ALL_PUBLISHED.iter().filter_map(|k| k.wire()).collect();
                return serde_json::json!({
                    "ok": false,
                    "kind": serde_json::Value::Null,
                    "title": serde_json::Value::Null,
                    "errors": [{ "line": null, "column": null, "stage": "kind",
                        "message": format!("no kind `{wire}`: {}", known.join(", ")) }],
                    "warnings": [],
                    "parameters": [],
                })
                .to_string();
            }
        },
    };
    check_with(source, expected, scripts, true).to_json().to_string()
}

/// [`check`], with the CPU scripts' half given or not ([`ScriptCheck`]),
/// and the parameters read or not.
pub fn check_with(source: &str, expected: Option<Kind>, scripts: Option<ScriptCheck>, with_params: bool) -> Checked {
    use crate::lang::EntryKind;
    let read = rhai_kind(source);
    let kind = expected.unwrap_or(read);
    let mut errors = Vec::new();
    let mut parameters = Vec::new();
    let mut title = crate::lang::rhai::title(source);
    let refuse = |kind: Kind, title: Option<String>, message: String| Checked {
        kind,
        title,
        errors: vec![Problem { line: None, column: None, message, stage: "kind" }],
        parameters: Vec::new(),
    };
    if read.is_entry() && read != kind {
        let wire = |k: Kind| k.wire().unwrap_or("unknown");
        return refuse(kind, title, format!("this source is a {} by its functions, not a {}", wire(read), wire(kind)));
    }
    let entry = match kind {
        Kind::Formula => Some(EntryKind::Formula),
        Kind::Coloring => Some(EntryKind::Coloring),
        Kind::Variation => Some(EntryKind::Transform),
        Kind::Solid => Some(EntryKind::Solid),
        Kind::Effect => Some(EntryKind::Effect),
        Kind::SimColoring => Some(EntryKind::SimColoring),
        Kind::SimModel => Some(EntryKind::SimModel),
        Kind::Field => Some(EntryKind::Field),
        Kind::FieldColoring => Some(EntryKind::FieldColoring),
        Kind::Generator | Kind::Modifier | Kind::Track | Kind::ScriptLibrary => None,
        Kind::UfFormulas | Kind::UfColorings | Kind::Fractint | Kind::Unknown => {
            return refuse(
                kind,
                title,
                "not a Rhai entry this build knows: a formula has `fn step`, a colouring `fn finish`, a variation `fn variation` ...".to_string(),
            );
        }
    };
    if let Some(entry) = entry {
        let src = crate::config::escape::CustomSource {
            dialect: crate::config::escape::CustomDialect::Rhai,
            entry: String::new(),
            source: source.to_string(),
            variations: Vec::new(),
        };
        match crate::escape::custom::check_entry(entry, &src) {
            Err(diags) => {
                errors.extend(diags.iter().filter(|d| d.severity == crate::lang::Severity::Error).map(|d| Problem::at(source, d, "check")));
            }
            Ok(program) => {
                title = Some(program.name.clone());
                let built: Result<(), String> = match kind {
                    // A colouring that sets no index draws nothing.
                    Kind::Coloring if !crate::lang::wgsl::colours_anything(&program) => Err(t!("formula_editor.colours_nothing").to_string()),
                    // A transformation becomes a variation, under the id
                    // Use gives it when it needs one.
                    Kind::Variation => {
                        let with_id = if crate::variations::custom::needs_id(source) {
                            crate::lang::rhai::with_own_id(source, &crate::scripted::mint_id())
                        } else {
                            source.to_string()
                        };
                        crate::variations::custom::compile(&with_id).map(|_| ())
                    }
                    Kind::Effect => {
                        let with_id = crate::lang::rhai::with_own_id(source, &crate::scripted::mint_id());
                        crate::effects::custom::compile(&with_id, crate::provenance::Provenance::Local).map(|_| ())
                    }
                    Kind::SimColoring => crate::sim::custom::compile(source, "custom-check").map(|_| ()),
                    Kind::SimModel => crate::sim::custom::compile_model(source, "custom-check").map(|_| ()),
                    Kind::Solid => crate::escape::custom::solid_error(&program, source).map_or(Ok(()), Err),
                    _ => Ok(()),
                };
                if let Err(e) = built {
                    errors.push(Problem::rendered(&e, "build"));
                }
                if with_params && errors.is_empty() {
                    match program_params(kind, &program, source) {
                        Ok(p) => parameters = p.iter().map(api_parameter).collect(),
                        Err(e) => errors.push(Problem::rendered(&e, "build")),
                    }
                }
            }
        }
    } else if kind != Kind::ScriptLibrary {
        // A library declares nothing and runs nothing on its own.
        let Some(scripts) = scripts else {
            return refuse(
                kind,
                title,
                "a generator, modifier or track is checked by the script module's compile(); this one carries the renderer alone".to_string(),
            );
        };
        match scripts(kind, source) {
            Ok((own_title, params)) => {
                if own_title.is_some() {
                    title = own_title;
                }
                if with_params {
                    parameters = params.iter().map(api_parameter).collect();
                }
            }
            Err(e) => errors.push(Problem::rendered(&e, "check")),
        }
    }
    Checked { kind, title, errors, parameters }
}

fn register_kind(kind: Kind, title: &str, source: &str) -> Result<(String, crate::config::escape::CustomSource), String> {
    match kind {
        Kind::Variation => {
            let key = crate::variations::custom::register(source)?;
            let mut carried = crate::variations::custom::carried(source).ok_or("it does not name itself")?;
            carried.entry = key.clone();
            Ok((key, carried))
        }
        Kind::Effect => {
            let key = crate::effects::custom::register(source, crate::provenance::Provenance::Local)?;
            let mut carried = crate::effects::custom::carried(source).ok_or("it has no id")?;
            carried.entry = key.clone();
            Ok((key, carried))
        }
        Kind::Formula | Kind::Coloring | Kind::Solid | Kind::Field | Kind::FieldColoring => {
            use crate::config::escape::{CustomDialect, CustomSource};
            use crate::escape::custom;
            use crate::lang::EntryKind;
            let ek = match kind {
                Kind::Formula => EntryKind::Formula,
                Kind::Coloring => EntryKind::Coloring,
                Kind::Field => EntryKind::Field,
                Kind::FieldColoring => EntryKind::FieldColoring,
                _ => EntryKind::Solid,
            };
            let src = custom::with_called_variations(ek, CustomSource { dialect: CustomDialect::Rhai, entry: title.to_string(), source: source.to_string(), variations: Vec::new() });
            let name = custom::name_of(ek, &src);
            custom::compile_as(ek, &src, &name).map_err(|d| d.first().map(|x| x.render(source)).unwrap_or_else(|| "it does not compile".to_string()))?;
            Ok((name, src))
        }
        Kind::SimColoring => {
            let name = crate::sim::custom::register(source)?;
            Ok((name, crate::sim::custom::carried(source)))
        }
        Kind::SimModel => {
            let name = crate::sim::custom::register_model(source)?;
            Ok((name, crate::sim::custom::carried(source)))
        }
        // Nothing to register: the track editor offers it, and adding it
        // puts its source in the animation. Its key is its id, else its
        // title.
        Kind::Track => {
            let declared = crate::animation::script::declare(source)?;
            let key = crate::lang::rhai::info_string(source, "id").unwrap_or_else(|| declared.title.clone());
            use crate::config::escape::{CustomDialect, CustomSource};
            Ok((key, CustomSource { dialect: CustomDialect::Rhai, entry: title.to_string(), source: source.to_string(), variations: Vec::new() }))
        }
        _ => Err("not a kind of plug-in".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shipped entries are all there, each read: the script starters
    /// by their declarations, the formula examples and the effects by
    /// their kinds and titles.
    /// An animation track reads as one (docs/archive/projects/rhai-tracks.md), and
    /// can be a plug-in, keyed by its id.
    #[test]
    fn a_track_is_a_track() {
        let src = "const INFO = #{ title: \"Turn\", id: \"5b0f8c1e-2d3a-4b5c-8d9e-0f1a2b3c4d5e\" };
const TARGETS = #{ rotation: \"set\" };
fn value(t, p, ctx) { #{ rotation: t } }";
        let e = formula_entry("turn.rhai", src, Origin::Local, Link::default());
        assert_eq!((e.kind, e.title.as_str()), (Kind::Track, "Turn"));
        assert!(Kind::Track.can_be_plugin());
        let (key, carried) = register_kind(Kind::Track, "Turn", src).expect("registers");
        assert_eq!(key, "5b0f8c1e-2d3a-4b5c-8d9e-0f1a2b3c4d5e");
        assert_eq!(carried.source, src);
    }

    /// A stateful track has `start` and `step`, as a formula does: it was
    /// read as a formula (the shipped Springy Zoom and Smoothed Jitter).
    #[test]
    fn a_stateful_track_is_a_track_not_a_formula() {
        for (file, source) in crate::storage::formula_store::EXAMPLES {
            if file.starts_with("track-") {
                assert_eq!(rhai_kind(source), Kind::Track, "{file}");
            }
        }
    }

    #[test]
    fn the_scan_reads_every_shipped_entry() {
        let all = scan();
        let shipped: Vec<&Entry> = all.iter().filter(|e| e.origin == Origin::Builtin).collect();
        let scripts = shipped.iter().filter(|e| e.store == Store::Scripts).count();
        assert_eq!(scripts, crate::script::library::EMBEDDED.len());
        assert!(shipped.iter().filter(|e| e.store == Store::Scripts).all(|e| e.kind != Kind::Unknown && !e.title.is_empty()));
        let basic = shipped.iter().find(|e| e.file == "basic_random.rhai").expect("a starter");
        assert_eq!((basic.kind, basic.title.as_str()), (Kind::Generator, "Basic Random"));
        let vignette = shipped.iter().find(|e| e.file == "vignette.rhai" && e.kind == Kind::Effect).expect("the effect");
        assert_eq!((vignette.title.as_str(), vignette.id.as_deref()), ("Vignette", Some("vignette")));
        assert_eq!(vignette.version.len(), 8);
        assert!(shipped.iter().any(|e| e.kind == Kind::Formula), "the formula examples");
    }

    /// Both lists mark an entry alike: whose it is.
    #[test]
    fn the_marks_say_whose_it_is() {
        assert_eq!(marks(Origin::Builtin), "");
        assert_eq!(marks(Origin::Local), " *");
        assert_eq!(marks(Origin::Api), " ↓");
    }

    /// A formula store file's link: set, read back, cleared by deleting the
    /// file, so a name reused later inherits nothing. Against the real
    /// store, as the script store's tests are, and cleaned up.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_formula_files_link_goes_with_it() {
        use crate::storage::formula_store as store;
        let file = store::save("__library link test__.rhai", "fn variation(z, p) { z }\n").expect("save");
        let cloud = Some("link-test-cloud".to_string());
        store::set_link(&file, Link { cloud_id: cloud.clone(), ..Default::default() }).expect("link");
        assert!(store::link_of(&file).is_some_and(|l| l.cloud_id == cloud));
        assert!(scan().iter().any(|e| e.file == file && e.origin == Origin::Local && e.link.cloud_id == cloud), "the scan reads it");
        store::delete(&file).expect("delete");
        assert!(store::link_of(&file).is_none(), "deleting the file forgets its link");
        // An empty link is no entry at all.
        let file = store::save("__library link test__.rhai", "x").expect("save");
        store::set_link(&file, Link::default()).expect("link");
        assert!(!store::links().contains_key(&file));
        store::delete(&file).expect("delete");
    }

    /// An effect file compiles under its key, its source carried under it,
    /// and is offered; the second ask is the cache's (the source is not read
    /// again); forgetting stops the offer; a file of no list's kind is
    /// refused (docs/projects/one-list.md).
    #[test]
    fn an_effect_file_compiles_once_and_is_offered() {
        let src = "const INFO = #{ title: \"Tint test\", id: \"1d2e3f4a-5b6c-4d7e-8f9a-0b1c2d3e4f5a\" };\nfn effect(c, p) { c }\n";
        let p = compile_file("__tint test__.rhai", false, "v1", || Some(src.to_string())).expect("compiles");
        assert_eq!(p.kind, Kind::Effect);
        assert!(crate::effects::global_effect_registry().get(&p.key).is_some_and(|i| i.provenance == crate::provenance::Provenance::Local));
        assert_eq!(p.carried.entry, p.key);
        assert!(plugins_of(Kind::Effect).iter().any(|q| q.file == p.file));
        assert!(is_file_key(&p.key));
        let again = compile_file("__tint test__.rhai", false, "v1", || None).expect("from the cache");
        assert_eq!(again.key, p.key);
        forget(&p.file);
        assert!(!plugins_of(Kind::Effect).iter().any(|q| q.file == p.file));
        crate::effects::global_effect_registry_mut().remove_by_name(&p.key);
        assert!(compile_file("__lib__.rhai", false, "v1", || Some("script(\"x\", \"generator\");\n".to_string())).is_err());
    }

    /// An effect file with no id of its own compiles under one derived
    /// from its name -- the same every time -- and the file is not
    /// rewritten (it was, by the plug-in flag).
    #[test]
    fn a_file_without_an_id_compiles_under_a_derived_one() {
        let src = "const INFO = #{ title: \"No id test\" };\nfn effect(c, p) { c }\n";
        let a = compile_file("__no id test__.rhai", false, "a", || Some(src.to_string())).expect("compiles");
        forget("__no id test__.rhai");
        let b = compile_file("__no id test__.rhai", false, "b", || Some(src.to_string())).expect("compiles again");
        assert_eq!(a.key, b.key, "the same id: derived from the name");
        forget("__no id test__.rhai");
        crate::effects::global_effect_registry_mut().remove_by_name(&a.key);
    }

    /// A formula and a colouring plug-in register in the escape engine's
    /// registry under their slot names, which a config names.
    #[test]
    fn an_escape_plugin_registers_in_its_slot() {
        let formula = "const INFO = #{ title: \"Plug test\" };\nfn start(pixel, p) { #{ z: complex(0.0, 0.0), c: pixel } }\n\
                       fn step(s, p) { s.z = s.z * s.z + s.c; s }\nfn escaped(s, p) { s.z.norm_sqr() > 4.0 }\n";
        let p = compile_file("__plug test__.rhai", false, "v1", || Some(formula.to_string())).expect("compiles");
        assert_eq!((p.kind, p.title.as_str()), (Kind::Formula, "Plug test"));
        assert!(p.key.starts_with(crate::escape::custom::PREFIX));
        assert!(crate::escape::custom::formula(&p.key).is_some(), "registered under its slot name");
        assert_eq!(p.carried.source, formula);
        forget(&p.file);
    }

    /// A variation plug-in registers under its key. Writes the global
    /// variation registry: run alone (VARIATIONS.md, Testing).
    #[test]
    #[ignore = "writes the global variation registry: run alone"]
    fn a_variation_plugin_registers_under_its_key() {
        let src = "const INFO = #{ title: \"Plug var\", name: \"rhai_plug_test\" };\nfn variation(z, p) { z * 0.5 }\n";
        let p = compile_file("__plug var__.rhai", false, "v1", || Some(src.to_string())).expect("compiles");
        assert_eq!(p.kind, Kind::Variation);
        assert!(crate::scripted::is_key(&p.key));
        assert!(crate::variations::global_registry().has(&p.key));
        forget(&p.file);
        crate::variations::global_registry_mut().remove_by_name(&p.key);
    }

    /// `INFO.description` and `INFO.author` reach the entry, a
    /// description in the current language.
    #[test]
    fn a_formula_entry_reads_its_info() {
        let src = "const INFO = #{ title: \"Spiral\", description: #{ en: \"A spiral\", ja: \"螺旋\" }, author: \"Ada\" };\n\
                   fn variation(z, p) { z }\n";
        let e = formula_entry("spiral.rhai", src, Origin::Local, Link::default());
        assert_eq!((e.kind, e.title.as_str(), e.description.as_str(), e.author.as_str()), (Kind::Variation, "Spiral", "A spiral", "Ada"));
        assert!(crate::lang::rhai::transformation_any(src).is_ok(), "the checker accepts both");
        let uf = formula_entry("traps.ucl", "x {\n}\n", Origin::Api, Link::default());
        assert_eq!(uf.kind, Kind::UfColorings);
    }

    /// A source that opens with a def's doc block -- a twin of a built-in
    /// does -- has its prose and authors from it: the description in one
    /// paragraph as the API keeps it, the authors joined.
    #[test]
    fn a_doc_block_reaches_the_entry() {
        let src = "/// Voronoi cells, picked in a noise-distorted\n/// square lattice.\n///\n/// # Authors\n/// - slobo777\n/// - Andreas Maschke\n\
                   const INFO = #{ title: \"Crackle\", name: \"rhai_crackle\" };\nfn variation(z, p) { z }\n";
        let e = formula_entry("crackle.rhai", src, Origin::Local, Link::default());
        assert_eq!(e.description, "Voronoi cells, picked in a noise-distorted square lattice.");
        assert_eq!(e.author, "slobo777, Andreas Maschke");
        assert!(crate::lang::rhai::transformation_any(src).is_ok(), "the block is a comment to Rhai");
    }

    /// The browser's `compile()`: a twin checks clean, with the parameters
    /// a published script carries -- Phoenix's and Smooth's, which the
    /// script module's `collect_params` refused (GPU dialects).
    #[test]
    fn a_twin_checks_clean_with_its_parameters() {
        for (kind, source) in [(Kind::Formula, crate::escape::twins::source("phoenix").unwrap()), (Kind::Coloring, crate::escape::twins::coloring_source("smooth").unwrap())] {
            let c = check(source, Some(kind));
            assert!(c.errors.is_empty(), "{:?}", c.errors);
            assert_eq!(c.kind, kind);
            assert!(!c.parameters.is_empty(), "{kind:?} has parameters");
            assert_eq!(c.parameters, api_parameters(kind, source).unwrap());
            // Read from the source when no kind is asked for.
            assert_eq!(check(source, None).kind, kind);
        }
        let field = crate::escape::twins::FIELD_COLORING_TWINS[0].1;
        let c = check(field, None);
        assert_eq!((c.kind, c.errors.len()), (Kind::FieldColoring, 0), "{:?}", c.errors);
        let generator = crate::script::library::EMBEDDED[0].1;
        let c = check(generator, Some(Kind::Generator));
        assert!(c.errors.is_empty(), "{:?}", c.errors);
    }

    /// A mistake comes back where it is, at its stage; another kind than
    /// the one asked for is refused before anything compiles; an unknown
    /// kind is a report.
    #[test]
    fn a_mistake_comes_back_where_it_is() {
        let broken = "fn start(pixel, p) { #{ z: complex(0.0, 0.0), c: pixel } }\nfn step(s, p) {\n    s.z = s.z * s.z + nonsense;\n    s\n}\nfn escaped(s, p) { s.z.norm_sqr() > 4.0 }\n";
        let c = check(broken, Some(Kind::Formula));
        let first = c.errors.first().expect("an error");
        assert_eq!((first.line, first.stage), (Some(3), "check"), "{first:?}");
        assert!(c.parameters.is_empty());

        let phoenix = crate::escape::twins::source("phoenix").unwrap();
        let c = check(phoenix, Some(Kind::Coloring));
        assert_eq!(c.errors[0].stage, "kind", "{:?}", c.errors);

        let json: serde_json::Value = serde_json::from_str(&compile_json("palette", phoenix)).unwrap();
        assert_eq!((json["ok"].as_bool(), json["errors"][0]["stage"].as_str()), (Some(false), Some("kind")));
        let json: serde_json::Value = serde_json::from_str(&compile_json("", broken)).unwrap();
        assert_eq!(json["kind"], "formula");
        assert_eq!(json["errors"][0]["line"], 3);
        // The renderer's check leaves the CPU scripts to the script module.
        let generator = crate::script::library::EMBEDDED[0].1;
        let json: serde_json::Value = serde_json::from_str(&compile_json_gpu("generator", generator)).unwrap();
        assert_eq!(json["errors"][0]["stage"], "kind", "{json}");
        let json: serde_json::Value = serde_json::from_str(&compile_json("generator", generator)).unwrap();
        assert_eq!(json["ok"], true, "{json}");
    }

    /// A rendered compiler message keeps its position.
    #[test]
    fn a_rendered_message_keeps_its_position() {
        let p = Problem::rendered("12:5: error: no such name", "build");
        assert_eq!((p.line, p.column, p.message.as_str()), (Some(12), Some(5), "no such name"));
        assert_eq!(p.render(), "12:5: error: no such name");
        let plain = Problem::rendered("an effect has `fn effect(c, p)`", "build");
        assert_eq!((plain.line, plain.render().as_str()), (None, "an effect has `fn effect(c, p)`"));
    }
}
