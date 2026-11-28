# Previews on the server

**Status 2026-11-28:** steps 1-4 and section 4 (script parameters and
every built-in's twin, "a kind of preview") built; ffapi's all merged
(`previews`, `script-parameters`, `test-cleanup`), this repo's on branch
`previews`. Synced and uploaded against the development database
(section 4's last paragraph). Tried against the development database:
- **Rendered:** 928 pictures of 931 planned, in a few minutes. The
  slowest are the simulations. The three tracks have no Catalog picture.
- **Blank:** 64 of the 928 came out blank and are not kept or uploaded
  (section 5).
- **Uploaded:** 864, through a temporary server and a throwaway admin.
  The second run sent none.
- **Read back:** through `/types`, `/variations/{name}`, a curated
  fractal's and a curated script's `thumbnail_url`, and the JPEG route.

**The goal, in the user's words:** "use JPEGs stored on the server for
all the formula, coloring, fractal presets, variations, etc. previews.
The server cannot generate them, it doesn't have a GPU, but I was
planning on creating them locally and uploading via the API."

**Decided (the user, 2026-11-27):**
- **512 px JPEG**, the size of a fractal's thumbnail.
- **Every app built-in.** Today that is the same as the curated items.

---

## 1. What exists

- **The server stores pictures for fractals only** (`PUT /fractals/{id}/
  thumbnail`, 512 JPEG) and for gallery items. Even those are refused for
  a curated fractal: `authorize_write` answers 409 `curated_static`, and
  the seeding sync carries no thumbnails. So the presets curated by the
  sync have no picture either.
- **The desktop Catalog already makes a picture of every built-in**
  (`ui::inventory_preview::preview`):
  - a variation: its image of the plane;
  - an effect: a reference fractal before and after;
  - a formula, field or solid: its default view;
  - a colouring: on the default formula;
  - a simulation model or colouring: its first preset, run to a capped
    step count;
  - a generator: its run at seed 1; a modifier: the reference fractal,
    modified;
  - a preset: itself.
- **That path already runs headless:**
  - `render_previews_for_inspection` scans the inventory and builds each
    preview with a default `egui::Context`;
  - it renders through `renderer::render_preview` (256 px, lit tiers);
  - each preview carries `key`, a hash of the configs it renders and the
    entry's version, which is the staleness stamp a server copy needs.

## 2. The design

### Server (ffapi)

- **One store, keyed `(kind, name)`.** The kind is a curated kind
  (`fractal`, `palette`, `animation`, a script kind) or `variation` /
  `effect`. The name is the curated name or the built-in key: `formula/
  mandelbrot`, `variation/swirl`, `fractal/grand-julian`, `generator/
  basic_random`. Names compare without case, as curated names do.
- **Migration 123,** `previews`:
  - `kind`, `name`, `source_hash` (what the exporter rendered), the
    file, `updated_at`;
  - unique on `(kind, lower(name))`;
  - files under `THUMBNAIL_DIR/previews/`.
- **Routes:**
  - `PUT /admin/previews/{kind}/{name}?source_hash=…` (admin): JPEG or
    PNG, normalised to a 512 JPEG as a fractal thumbnail is (the same
    function);
  - `DELETE /admin/previews/{kind}/{name}` (admin);
  - `GET /admin/previews` (admin): every `(kind, name, source_hash)`, so
    the exporter uploads only what changed;
  - `GET /previews/{kind}/{name}` (public): the JPEG, cacheable,
    versioned by `?v=`.
- **Where it shows** (`/previews/{kind}/{name}?v=<updated_at>`):
  - **`/types`:** a built-in type's `preview_url` (`kind/key`);
  - **variations and effects:** `preview_url`, list and item;
  - **curated scripts:** `preview_url` (its kind, its canonical name);
  - **curated fractals:** `thumbnail_url` falls back to the preview of
    `fractal/<canonical name>` when the row has no thumbnail. That covers
    the presets wherever a fractal is listed.

### Client (here)

- **`cargo run --release --bin export_previews`:**
  - renders every built-in's preview at 512 into
    `output/server-previews/<kind>/<name>.jpg`, with `index.json` holding
    `(kind, name, source_hash)`;
  - `--upload --api URL` with `FFAPI_TOKEN` (an admin's) reads the
    server's list and PUTs what is missing or changed;
  - `--force` uploads everything;
  - `--only kind[/name]` narrows it.
- **Names, as the API knows each item:**
  - **curated items** (presets, shipped scripts, Rhai examples,
    variations, effects): the manifest's names. `curated::manifest`
    records where each record came from, so the exporter cannot drift
    from the sync;
  - **built-in types not in the manifest** (formulas, colourings,
    fields, solids, simulation models and colourings): their registry
    keys.
  - Where both give one name (an example named for its built-in:
    `formula/mandelbrot`), the built-in's picture wins: it is the type.
- **The picture:**
  - each entry's Catalog preview, rendered at 512 with the thumbnail's
    sample density and lit 3D tiers (`renderer::render_preview_at`);
  - an effect's is its "after";
  - a preset's is itself.
- **`source_hash`:** the preview's key prefixed with a render epoch
  (`preview-512-1`), so a change of size or density renders everything
  again. A renderer fix that changes pixels without changing a config
  needs `--force`.

## 3. Order of building

1. ffapi: the migration, the store, the four routes, their tests.
2. ffapi: `preview_url` / `thumbnail_url` in the four places above.
3. Here: the manifest's origins, `render_preview_at`, the exporter
   (render, index, upload), its tests.
4. Try it against the local server: render all, upload, read one back
   through `/types` and a curated fractal.
5. Docs: BROWSER_APP_0.6.md (where `preview_url` and the fallback come
   from), RELEASE.md (a step after the curated sync: `export_previews
   --upload`), this doc's status.

## 4. Script parameters, and a twin for every built-in

**The ask, from the browser:** show a script's parameters as it shows a
variation's and an effect's, and a definition for every built-in: only 5
of about 116 built-ins had a curated Rhai twin, scripts came without
parameters, and the browser cannot collect a GPU kind's.

**Decided (the user, 2026-11-27):**
1. The examples that hold a built-in's key move aside, as
   `<name>-example`, so the twins take the keys.
2. Parameters reuse the variations' and effects' way: a `parameters`
   column on scripts, the same `Parameter` shape, written by
   `params::api_json`.

**Built:**
- **Here:**
  - `storage::library::param_defs(kind, source)`: any Rhai file's
    parameters by its kind's rules, registering nothing (a GPU kind's
    through `escape::custom::check_entry` and `ir::Program::param_defs`,
    a complex parameter split as the kind splits it; a track's `PARAMS`;
    a CPU script's collected declarations).
  - `api_parameters` / `api_parameter`: each as `params::api_json`, a
    text's or colour's default as itself.
  - The curated manifest gives every script record `content.parameters`,
    and curates the twin of every escape and simulation built-in under its
    key (`curated::twins`: 26 formulas, 28 colourings, 4 solids, 3
    fields, 3 field colourings, 31 models, 13 simulation colourings; 108
    in all), named and described as the built-in type is, origin
    `Origin::Twin`. The preview exporter draws a twin as its built-in.
    The seven flame-backed built-ins (`ifs_flame`, `ifs_flame_3d`, five
    `ifs_*` colourings) have no twin. The variations' and effects' twins
    stay out: those names are the WGSL catalog's.
  - Examples named for a built-in are `mandelbrot-example`,
    `burning-ship-example`, `phoenix-example`, `smooth-example`,
    `orbit-trap-example` (replacing the `builtin_key` naming of
    2026-11-27's earlier commit).
  - The app's own uploads send `parameters` (`api::entries`,
    `api::sync::script_to_create_request`).
- **ffapi (`script-parameters`):**
  - migration 124, `scripts.parameters`, which `curated_static` lets
    change (derived, not content);
  - create and update store it (an update without it clears it; at most
    256 entries, 64 KiB, each named); the detail serves it;
  - the import stores it and refreshes an unchanged, restored or
    taken-over row's;
  - a `coloring` built-in type pairs with, and takes the preview of, the
    `field_coloring` of its key;
  - BROWSER_APP_0.6.md and curated-build.md.

**Synced 2026-11-28** against the development database: 108 twins
`created`; `burning_ship`, `phoenix`, `smooth` and `orbit_trap`
`new_version` (each keeping its example as the older version); the five
examples `created` under their new names; everything else `unchanged`.
Two errors, both fixed:
- `formula/mandelbrot`: an older copy of the same source already held the
  stem `mandelbrot~f5648772`, so moving the current row aside broke the
  unique stem. ffapi's `move_aside` takes the next free one (`-2`); the
  rerun gave `new_version`.
- `effect/kaleidoscope-example`: the effect twin `kaleidoscope.rhai` came
  in with the CPU modifier of that name (scripts were matched by file name
  alone). Matched by store and file now.

The upload sent the five examples' pictures and, unexpectedly, 14 effects
again: an effect's parameters are a `HashMap`, serialized in a different
order every run, so every config with an effect hashed differently. They
serialize in name order now; the next upload sends the effects once more
under their stable keys.

## 5. Blank pictures: the Catalog's previews to fix

A picture under 0.2% lit is blank. The exporter keeps it out of the
upload and prints the list at the end of a run (`index.json` marks it
`blank`, so it is rendered again only when its config changes). The same
recipe draws the desktop Catalog's previews, so the Catalog shows the
same blanks. The 64 of the first run, in groups:
- **z-only and 3D variations,** drawn as a flat image of the plane:
  `zcone`, `zscale`, `zblur`, `ztranslate`, `inflateZ_1`-`6`, `extrude`,
  `julia3Dz`, `julian3Dx`, `pre_dcztransl`, `sphere_volume`,
  `polyhedron_volume`, `pointgrid3d_wf`, `polarplot3d_wf`,
  `lsystem_path_3D`.
- **Variations that draw their own points or colours** and draw nothing
  in this setup:
  - `pdj`, `spirograph`, `harmonograph_js`, `macmillan`, `curliecue2`,
    `CircleTrans1`, `line`, `taurus`;
  - `lsystem_path`, `lsystem_tree`, `truchet`, `checkerboard_wf`,
    `plane_wf`, `parplot2d_wf`, `polarplot2d_wf`;
  - `colorscale_wf`, `post_colorscale_wf`, `farblur`;
  - `npolar`, `julian2`, `post_point_symmetry_wf`, `post_mirror_wf`;
  - `subflame_wf`, `pre_subflame_wf`, `fract_meteors_wf`,
    `fract_salamander_wf`.
- **The Rhai orbit examples:**
  - the formulas `orbits-chip`, `orbits-dynamic-system`,
    `orbits-gingerbreadman`, `orbits-hopalong`, `orbits-latoocarfian`,
    `orbits-martin`, `orbits-pickover-attractor`,
    `orbits-pickover-popcorn`, `orbits-quadruptwo`,
    `orbits-rossler-attractor`, `orbits-threeply`;
  - their variation forms `variation_chip`, `variation_martin`,
    `variation_quadruptwo`, `variation_threeply`.
- **Others:**
  - the colourings `image_trap` (it needs an image) and
    `frothy-basin-coloring`;
  - the Flame Attractor solids `ifs_flame` and `ifs_flame_3d`, which draw
    a flame a default config has none of.

## 6. Not in this plan

- **Palettes:** a gradient draws itself; the browser can render it from
  the colours.
- **Pictures for user items:** fractals keep their own thumbnail upload.
- **A release.py stage for the upload:** add it once the exporter has
  run for real.
