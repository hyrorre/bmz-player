# Song archive regression fixtures

These files contain only synthetic data generated for BMZ tests. No song,
audio recording, or third-party asset is included. They are distributed under
the repository's GPL-3.0-only license.

- `rar4-solid.rar`: RAR 2.9/4 solid stream, six files, 295108 expanded bytes.
- `rar5-solid.rar`: RAR5 solid stream containing the same six files.
- `rar4-cp932.rar`: legacy CP932 name `ソ/譜面.bms`; `ソ` contains byte 0x5c.

To regenerate, create a temporary standalone Rust crate (edition 2024), use
`generate.rs` as `src/main.rs`, and depend on:

```toml
rars = { version = "=0.10.0", default-features = false, features = ["write"] }
```

Run `cargo run -- <output-directory>`. The generator also writes the original
six files to `expected/`. Both solid archives were independently decoded with
7-Zip and compared against the original files, then read using `rars` with
default features disabled. The product does not enable the RAR writer feature.
ZIP and 7z test archives are generated in each test's private temporary folder.
