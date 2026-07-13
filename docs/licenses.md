# License Notes

This document is a working checklist for BMZ Player distribution. It is not legal advice.

## BMZ Player

BMZ Player source code is licensed as `GPL-3.0-only`.

- The workspace license is declared in `Cargo.toml`.
- The full GPLv3 text is in `LICENSE`.
- First-party crates under `crates/` inherit the workspace license.

## FFmpeg

BMZ Player uses FFmpeg through the Rust `ffmpeg-next` crate.

Current use sites:

- `crates/bmz-ffmpeg` initializes FFmpeg once per process.
- `crates/bmz-audio` decodes audio samples through FFmpeg.
- `crates/bmz-video` decodes video BGA frames through FFmpeg.

FFmpeg itself is normally licensed under `LGPL-2.1-or-later`, but optional GPL components make the resulting FFmpeg build GPL. Optional nonfree components can make a build non-redistributable. The official FFmpeg legal page is the source of truth:

- https://ffmpeg.org/legal.html

BMZ Player is already GPLv3, so using a GPL-enabled FFmpeg build is compatible with the application license. For redistributable BMZ Player releases, do not ship an FFmpeg build configured with `--enable-nonfree`.

Recommended release policy:

- Prefer dynamic linking to FFmpeg shared libraries.
- Record the exact FFmpeg version and build configuration used for each release.
- If FFmpeg binaries or libraries are bundled, provide the corresponding FFmpeg source or a clear source offer matching the bundled binaries.
- Preserve FFmpeg license notices and mention FFmpeg in any release notes / about dialog.
- Do not rename FFmpeg dynamic libraries in a way that hides their origin.
- Treat codec patent risk as release-jurisdiction dependent, especially for commercial distribution.

Development installs are not redistributable artifacts by themselves:

- Windows instructions currently use `vcpkg install ffmpeg:x64-windows`.
- macOS instructions currently use `brew install ffmpeg`.
- Linux instructions currently use distribution packages such as `ffmpeg-free` / `ffmpeg-free-devel`.

Before publishing an installer, archive, app bundle, or container image, check the concrete FFmpeg binaries included in that artifact.

## ASIO SDK

BMZ Player enables ASIO support on Windows through `cpal/asio`, which depends on
the Rust `asio-sys` crate. ASIO support is Windows-only in this project.

Current use sites:

- `crates/bmz-player/Cargo.toml` enables the `asio` feature by default.
- `crates/bmz-audio/Cargo.toml` maps that feature to `cpal/asio`.
- `docs/packaging.md` documents the Windows release job as an ASIO-enabled build.

Steinberg's current public ASIO open-source license page states that ASIO
technology is available under `GPLv3`, and that the ASIO name and logo are
Steinberg trademarks whose use is separate from the GPL terms:

- https://www.steinberg.net/developers/asiosdk-open/
- https://www.steinberg.net/asiosdk

BMZ Player is already `GPL-3.0-only`, so the ASIO SDK's GPLv3 open-source
variant is compatible with the application license direction. For release
artifacts that include ASIO support:

- Preserve ASIO SDK and Steinberg notices in the third-party notice file.
- Do not bundle or alter the ASIO logo unless the release follows Steinberg's
  ASIO usage guidelines.
- Do not use `ASIO` as part of the BMZ Player product name, company name, or
  release artifact name. Descriptive phrases such as "ASIO support" are safer.
- Record the `asio-sys` and `cpal` versions used by each release.
- If the ASIO SDK license files are copied into the build cache or artifact,
  keep them unmodified and include them alongside the other license texts.

## Microsoft GameInput Redistributable

Windows release installers include the unmodified `GameInputRedist.msi` from
the pinned `Microsoft.GameInput` NuGet package. Microsoft documents that PC
titles using GameInput should install this redistributable as part of their
normal installer:

- https://learn.microsoft.com/gaming/gdk/docs/features/common/input/overviews/input-nuget
- https://www.nuget.org/packages/Microsoft.GameInput

Release policy:

- Download a pinned NuGet version and verify its package SHA256 before staging.
- Preserve the package's `LICENSE.txt` and `NOTICE.txt` as
  `GameInput-LICENSE.txt` and `GameInput-NOTICE.txt`.
- Distribute `GameInputRedist.msi` unmodified and record its NuGet version in
  the Windows dependency provenance file.
- Do not present the GameInput name or Microsoft trademarks as an endorsement
  of BMZ Player.

## Bundled Skins

`data/skins/default` is BMZ Player's first-party minimal skin.

`data/skins/Rmz-skin` is a Git submodule pointing to the BMZ bundled fork of Rm-skin. Its license terms are not identical to BMZ Player's source license. Check the submodule files before distribution:

- `data/skins/Rmz-skin/README.md`
- `data/skins/Rmz-skin/readme.txt`
- `data/skins/Rmz-skin/_license/`

Rmz-skin contains files under `GPLv3` and files under `CC BY-NC-ND 4.0`. Files covered by the NoDerivatives condition must not be modified in the BMZ Player repository. Compatibility fixes should be implemented in `bmz-skin`, `bmz-render`, or `bmz-player` instead of editing the bundled skin assets.

`data/skins/mz-select` is a Git submodule pointing to the BMZ bundled copy of mz-select / m-select. Check the submodule files before distribution:

- `data/skins/mz-select/readme.txt`
- `data/skins/mz-select/license/`

mz-select's readme permits use, modification, and redistribution of the skin and included images. Bundled fonts and VOICEVOX audio have separate terms documented under `license/` and `readme.txt`; preserve those notices in release packages.

Third-party skins copied under `data/skins/` for manual compatibility testing remain gitignored and must not be committed unless they are intentionally added as a documented bundled asset.

## Third-party Notices

Release artifacts include human-readable license notice files:

- Non-Cargo notice source: `THIRD-PARTY-NOTICES.txt`
- Non-Cargo notice path on Windows portable / installer staging:
  `resources/licenses/third-party-notices.txt`
- Non-Cargo notice path inside macOS app bundles:
  `Contents/Resources/licenses/third-party-notices.txt`
- Generated Rust dependency report on Windows portable / installer staging:
  `resources/licenses/rust-dependency-licenses.txt`
- Generated Rust dependency report inside macOS app bundles:
  `Contents/Resources/licenses/rust-dependency-licenses.txt`

The egui license panel reads the packaged `third-party-notices.txt` and
`rust-dependency-licenses.txt` from `resource_dir/licenses/` and displays them
together. Local development can use a repository-root
`rust-dependency-licenses.txt` generated with the command below. In-app display
is useful, but it should not replace shipping readable text files in the release
artifact because users must be able to inspect notices outside the running app.

`THIRD-PARTY-NOTICES.txt` is the hand-maintained notice for bundled components
that are not covered by Cargo crate metadata, such as FFmpeg, the ASIO SDK, and
bundled skins. Before a public binary release, also generate a complete Rust
dependency license report from the exact release lockfile and include it in the
package as `rust-dependency-licenses.txt`.

BMZ Player uses `cargo-about` for the Rust dependency report:

```sh
cargo install --locked --features cli cargo-about
cargo-about generate --workspace --locked --fail \
  --output-file rust-dependency-licenses.txt \
  about.hbs
```

The package scripts pass the relevant target/features when they create release
artifacts. Review `about.toml` before accepting newly introduced license IDs.

## BMZ IR Web

The BMZ IR web application is built from the repository root `package.json` and
`bun.lock`. The package is private, but it declares `GPL-3.0-only` so generated
dependency reports have an explicit first-party license.

For Cloudflare Worker releases, generate the web dependency report from the
Wrangler dry-run bundle metafile instead of from all installed `node_modules`.
This avoids treating dev-only or optional native packages as deployed code.

```sh
bun install --frozen-lockfile
bun run build
```

The generated report is written to:

- `.output/public/licenses/web-dependency-licenses.txt`
- `bmz-ir-web/public/licenses/web-dependency-licenses.txt` (local dev copy, gitignored)

`scripts/generate-web-license-report.mjs` reads the Wrangler `--metafile`
inputs and the matching Nuxt/Nitro sourcemaps under `.output/server`, maps
bundled files back to installed npm packages, applies `web-license-policy.json`,
and fails when a bundled package has an unaccepted or review-required license.
The policy currently chooses the BSD side of
`node-forge`'s `(BSD-3-Clause OR GPL-2.0)` expression and does not accept GPL /
LGPL / AGPL-only packages without review.

The deployed BMZ IR site exposes the raw report at
`/licenses/web-dependency-licenses.txt` and renders it inside the application at
`/licenses`.

## Release Checklist

Before publishing a binary release:

1. Confirm `Cargo.toml` still declares the intended BMZ Player license.
2. Generate a third-party Rust dependency license report with `cargo-about --fail`.
3. Record the FFmpeg version, configure flags, source URL, and binary source/provenance.
4. Confirm no bundled FFmpeg build uses `--enable-nonfree`.
5. Include `THIRD-PARTY-NOTICES.txt`, `rust-dependency-licenses.txt`, GameInput, FFmpeg, ASIO SDK, and bundled-skin notices in the release package.
6. Confirm bundled skin submodules such as `data/skins/Rmz-skin` and `data/skins/mz-select` point at the intended commits.
7. Confirm Windows release artifacts built with default features are intended to include ASIO support.
8. Confirm no gitignored third-party skins, songs, databases, profiles, credentials, or `.env` files are included.
9. Confirm the Windows installer contains the pinned, unmodified GameInput redistributable and matching license notices.

Before deploying BMZ IR web:

1. Run `bun install --frozen-lockfile`.
2. Run `bun run build`.
3. Confirm `/licenses` can display `/licenses/web-dependency-licenses.txt`.
