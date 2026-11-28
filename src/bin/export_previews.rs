//! Render every app built-in's preview and upload it to the API
//! (docs/projects/previews.md; `fae::preview_export`).
//!
//! ```text
//! cargo run --release --bin export_previews                    # render what is missing or stale
//! cargo run --release --bin export_previews -- --only formula  # one kind (or kind/name)
//! FFAPI_TOKEN=... cargo run --release --bin export_previews -- --upload --api http://localhost:3000
//! cargo run --release --bin export_previews -- --upload --no-render   # upload an earlier render
//! cargo run --release --bin export_previews -- --force         # every picture again
//! ```
//!
//! Pictures go to `output/server-previews/<kind>/<name>.jpg` (`--out DIR`), with
//! `index.json`: kind, name and the source hash each was rendered from.
//! `--upload` PUTs, with an admin's token, what the server lacks or has
//! from another render.

fn main() {
    let mut opts = fae::preview_export::Options {
        out: "output/server-previews".into(),
        only: None,
        render: true,
        upload: false,
        api: "http://localhost:3000".into(),
        force: false,
    };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--out" => opts.out = args.next().expect("--out DIR").into(),
            "--only" => opts.only = Some(args.next().expect("--only kind[/name]")),
            "--upload" => opts.upload = true,
            "--no-render" => opts.render = false,
            "--api" => opts.api = args.next().expect("--api URL"),
            "--force" => opts.force = true,
            other => {
                eprintln!("unknown argument {other}; see the header of src/bin/export_previews.rs");
                std::process::exit(2);
            }
        }
    }
    if let Err(e) = fae::preview_export::run(opts) {
        eprintln!("export_previews: {e}");
        std::process::exit(1);
    }
}
