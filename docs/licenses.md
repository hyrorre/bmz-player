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

## Release Checklist

Before publishing a binary release:

1. Confirm `Cargo.toml` still declares the intended BMZ Player license.
2. Generate a third-party Rust dependency license report.
3. Record the FFmpeg version, configure flags, source URL, and binary source/provenance.
4. Confirm no bundled FFmpeg build uses `--enable-nonfree`.
5. Include FFmpeg and bundled-skin notices in the release package.
6. Confirm bundled skin submodules such as `data/skins/Rmz-skin` and `data/skins/mz-select` point at the intended commits.
7. Confirm no gitignored third-party skins, songs, databases, profiles, credentials, or `.env` files are included.
