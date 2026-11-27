# Renaming "formula": Escape formula

**Status 2026-11-27:** planned, not built. Before the 0.6 release, like
[rename.md](rename.md): the API's names change free while no released
client uses them.

**Why, in the user's words:** "formula is a terrible and confusing name.
Colorings are formulas, fields are formulas, variations are formulas,
there's nothing to distinguish from the 'core' escape-time type."

**Decided (the user, 2026-11-27):** the UI says **Escape formula**.

---

## 1. What "formula" means today

It names two different things:

1. **The escape-time core** (fragment mode A's per-point iteration):
   - the Escape panel's "Formula" list;
   - the config's `escape.formula`, `formula_params`, `custom_formula`;
   - the catalog's and the API's kind `formula`: the type filter
     `formula:mandelbrot`, the curated namespace `formula/…`, the script
     kind `formula`, a component's role `formula`;
   - `FormulaDef` and `escape::FORMULAS` in Rust.
2. **Any Rhai source**, whatever it defines:
   - "your formula library" (`storage::formula_store`, the `formulas`
     folder);
   - "Start a new formula, in Rhai" and "Open a Rhai formula";
   - the Rhai Editor's internal name `formula_editor`;
   - `assets/formulas/`, which holds colourings, solids, fields, tracks
     and effects too.

The second sense makes the first one ambiguous. A rename of only the
first would leave "formula library" holding colourings.

## 2. The proposal

| What | Today | Proposed |
|---|---|---|
| UI: the escape-time core | "Formula" | **"Escape formula"** (decided) |
| UI: a Rhai source in general | "formula" | **"script"** or "Rhai file", as the sentence needs |
| Wire kind (catalog, contract, API, curated namespace, type filter, role) | `formula` | **`escape_formula`** (to confirm) |
| Config keys (`escape.formula`, `formula_params`, `custom_formula`) | as they are | **unchanged** |
| Rust names (`FormulaDef`, `formula_store`, `assets/formulas/`) | as they are | **unchanged** |

**Why `escape_formula` and not `escape`:**
- It matches the other engine-qualified kinds (`sim_model`,
  `field_coloring`).
- `escape` alone would read like the engine value `engine: "escape"`
  and give `type=escape:mandelbrot`.

(The earlier suggestion was `escape`; this corrects it.)

**Why the config keys stay:**
- They're in every saved `.fae` and in PNG metadata.
- Renaming them means a config v8 with a migration, for names no user
  sees.

**Why the Rust names stay:** they're internal, as in the last rename.

## 3. Compatibility

- **Saved files:** unchanged keys, so nothing to migrate.
- **One exception, curated references:** a config saved since 2026-11-18
  may name a curated item `{"kind": "formula", ...}` in its `curated`
  table (`curated::refs`). Reading accepts `formula` as `escape_formula`,
  forever.
- **The API:** no aliases, as no released client uses `/api/v1`.
  - **Data the server stores** is renamed by a migration:
    - `scripts.kind` (the `script_kind` enum value `formula`);
    - `curated_names.kind`;
    - `fractal_types.kind`;
    - `flame_components.role`;
    - `previews.kind`.
  - **The server's catalog summariser** works from the client's
    regenerated catalog.
- **The browser:** a table of the changes in ffapi's browser docs, as
  for `/fractals`.

## 4. Order of work (once the kind name is confirmed)

1. **Here:**
   - the catalog's and contract's kind and role;
   - the inventory's API kind;
   - the manifest;
   - the preview exporter;
   - `curated::refs` reading `formula` as an alias;
   - regenerate the contract and the catalog.
2. **ffapi:**
   - `ScriptKind::EscapeFormula` (wire `escape_formula`);
   - the migration above;
   - `catalog_query` and `/types` kinds;
   - tests and docs.
3. **UI text:**
   - "Formula" → "Escape formula" where it means the core;
   - "formula" → "script" / "Rhai file" where it means any Rhai source;
   - the locale strings (about 50 in en.yml) and the docs (ESCAPE.md
     first).
4. **The curated sync:** `formula/…` names move to `escape_formula/…`.
   The production server has never synced, so its first sync is already
   the new names. Locally, rerun the sync (`--retire` drops the old
   names).

## 5. Open

- **Confirm the wire kind `escape_formula`.**
- **The second sense:** "script" everywhere, or "Rhai file" where the
  file is the point (Open, Save, the library)?
