# Linux tar archive validation (2026-09-15)

This note preserves historical validation results. Current packaging and verification
instructions are in [linux-tar.md](../../docs/linux-tar.md).

## Split-archive validation (2026-09-15)

[Manual run 34933066815](https://github.com/khanwul/bmz-player/actions/runs/34933066815)
passed on Ubuntu 22.04 with Docker and Rust 1.98.1 in 49 minutes 24 seconds.
Both archives were built from version `0.4.0`, commit
`f37ea95e85cb24422cb3d4e7156fa649e717e91c`. Subsequent documentation-only commits
record these results; that SHA identifies the actual packaged source.

| Archive | Bytes | SHA256 |
| --- | ---: | --- |
| `bmz-player-v0.4.0-linux-x64.tar.gz` | 963,634,642 | `66da6c30bd06c2a928a4c85e68d35e908922df6a80d48747b43764bc30f3a76e` |
| `bmz-player-v0.4.0-linux-x64-sources.tar.gz` | 1,207,718,740 | `a67cdf12f10d0c3d4cdbc2384b4a825837223b95fc0886934ea636231f5fbe1c` |

Both files are below 2 GiB. No compression-format change was needed. The
[verified Actions artifact](https://github.com/khanwul/bmz-player/actions/runs/34933066815/artifacts/10383234579)
contains exactly these two archives and `SHA256SUMS.txt`. Its aggregate ZIP size
is not the per-archive size checked above.

- `cargo fmt --check`, `cargo check --locked`, `cargo clippy --locked` passed.
- `cargo test --locked`: 1,941 passed, 0 failed, 3 existing ignored tests.
- All 10 packaging regression tests passed, including mismatched commits,
  missing vendor/source files, changed source/skin/binary content, `.dsc`
  version/checksum errors, directory symlink inventory and the size limit.
- Both extracted file inventories and the committed source snapshot matched.
  Cargo version/lockfile, FFmpeg source/configuration and Ubuntu versions matched.
- Runtime-only, read-only container verification passed: ELF relocation and
  dependency checks, missing-FFmpeg negative control, CLI, relative paths,
  BMZ/XDG settings, SQLite writes, resource access and sample playback.
- All 18 Ubuntu source packages passed checksum/version checks and `.dsc`
  extraction. Empty-cache offline Cargo dependency resolution passed.
- A separate container mounted only the extracted source archive and rebuilt
  FFmpeg and BMZ release successfully with networking disabled.

The downloaded artifact also passed the two-archive `--verify` interface locally
with Podman on 2026-09-15. With all three artifact files extracted into
`dist/linux-tar/ci-34933066815/`, the successful command was:

```bash
CONTAINER_ENGINE=podman bash scripts/package-linux-tar.sh --verify \
  dist/linux-tar/ci-34933066815/bmz-player-v0.4.0-linux-x64.tar.gz \
  dist/linux-tar/ci-34933066815/bmz-player-v0.4.0-linux-x64-sources.tar.gz
```

This independently repeated the archive checks, runtime-only smoke tests,
Ubuntu source extraction and empty-cache offline FFmpeg/BMZ release rebuild.
The command exited successfully and wrote `validation.md` beside the archives.

Physical GPU/Wayland/audio latency/controller checks, rebuilding all Ubuntu
libraries, and byte-identical binary reproducibility remain outside this result.

## Historical single-archive validation (2026-09-15)

The following results precede the runtime/source split and do not validate the
new pair. New sizes and workflow evidence must come from the split implementation.

The archive built from `308ca195` was verified with the final `--verify` checker
using rootless Podman, Ubuntu 22.04/glibc 2.35, Rust 1.98.1 and GCC 11.4.0.
Later changes only adjust the verification fixtures and documentation; the Rust
sources, launcher and packaged libraries/resources are unchanged.

- `cargo fmt --check`, `cargo check --locked`, `cargo clippy --locked`: passed.
- `cargo test --locked`: 1,941 passed, 0 failed, 3 existing ignored tests.
- Extracted archive: all 25 bundled libraries and their relocations resolved;
  the missing-library negative control, read-only sample play, resource opens,
  relative CLI arguments, BMZ/XDG overrides and user DB/profile writes passed.
- All 3,483 packaged skin/font/sample files matched their committed Git blobs,
  including executable modes. Notices, corresponding sources for 18 Ubuntu
  packages, FFmpeg source and vendored application source were present.
- The archive is 2,172,396,883 bytes (about 2.02 GiB), including sources;
  `sha256sum --check SHA256SUMS.txt` passed.

GitHub-hosted Actions validation also passed on the Ubuntu 22.04 runner using
Docker: [manual run 34915419246](https://github.com/khanwul/bmz-player/actions/runs/34915419246)
built commit `dbdabb85`, passed the required Cargo checks (1,941 tests passed,
3 ignored), and verified the extracted archive before successfully uploading
`optional-linux-x86_64-tar` (2,172,285,632 bytes including the artifact ZIP wrapper).
The run took approximately 22 minutes. Subsequent documentation changes do not
change the tested packaging implementation.

An Ubuntu 22.04 physical desktop, specific GPU drivers, Wayland, latency and
physical game controllers remain unverified. The ignored Rust tests are two
manual skin profiling helpers and a GPU/external-FFmpeg video export test.
