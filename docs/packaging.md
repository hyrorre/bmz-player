# Packaging

## macOS `.app`

BMZ Player の macOS app bundle は `scripts/package-macos-app.sh` で作る。

```sh
scripts/package-macos-app.sh
```

既定の出力先:

```text
dist/macos/BMZ Player.app
```

bundle layout:

```text
BMZ Player.app/
  Contents/
    Info.plist
    MacOS/
      bmz-player
    Resources/
      skins/
        default/
        Rmz-skin/
      songs/
        sample-playable/
      licenses/
        BMZ-GPL-3.0-only.txt
        license-notes.md
    Frameworks/
      ...
```

`Contents/Resources` が runtime の `resource_dir` になる。`config.toml`,
`library.db`, `profiles`, `score.db`, `replay` などのユーザー状態は bundle
に含めず、`data_dir` 側へ作成する。

同梱スキンは `resource_dir/skins` に置く。編集したい場合は bundle 内を直接
変更せず、ユーザーが `data_dir/skins` へコピーしてユーザースキンとして選ぶ。

### Options

debug build で作る:

```sh
scripts/package-macos-app.sh --debug
```

出力先を変える:

```sh
scripts/package-macos-app.sh --out-dir /tmp/bmz-package
```

ad-hoc 署名する:

```sh
scripts/package-macos-app.sh --ad-hoc-sign
```

Developer ID で署名する:

```sh
scripts/package-macos-app.sh --sign "Developer ID Application: ..."
```

短い packaged smoke を実行する:

```sh
scripts/package-macos-app.sh --smoke
```

### FFmpeg / dylib bundling

既定では Homebrew など、実行環境に存在する dynamic libraries を使う。

`--bundle-dylibs` を付けると、`otool` で見える非 system dylib 依存を
`Contents/Frameworks` へコピーし、`install_name_tool` で参照を書き換える。

```sh
scripts/package-macos-app.sh --bundle-dylibs --ad-hoc-sign
```

この option は FFmpeg と codec library を配布物へ含める可能性がある。
公開用 artifact を作る前に必ず `docs/licenses.md` を確認し、FFmpeg の
version / configure flags / source provenance / `--enable-nonfree` 不使用を記録する。

### Manual smoke

既に作った `.app` を直接起動して smoke する場合:

```sh
BMZ_DATA_DIR=/tmp/bmz-player-package-data \
  "dist/macos/BMZ Player.app/Contents/MacOS/bmz-player" \
  --boot-play-sample \
  --smoke-exit-after-frames 3
```
