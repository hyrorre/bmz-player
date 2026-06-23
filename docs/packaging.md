# Packaging

## App icons

BMZ Player の desktop app icon は `bmz-ir-web/public/icon.svg` を source とし、
platform 用 asset は `scripts/generate-app-icons.sh` で生成する。

```sh
scripts/generate-app-icons.sh
```

生成先:

```text
assets/app-icon/
  bmz-player.png
  bmz-player-window.png
  bmz-player.ico
  bmz-player.icns
```

`bmz-player-window.png` は winit の実行時ウィンドウ icon として `bmz-player`
binary に埋め込む。`bmz-player.ico` は Windows installer / shortcut 用、
`bmz-player.icns` は macOS `.app` bundle 用。

## Windows installer (Inno Setup)

Windows の staging directory と Inno Setup installer は `scripts/package-windows.ps1`
で作る。

PowerShell 5.1 以降で実行する:

```powershell
.\scripts\package-windows.ps1
```

既定の staging 出力先:

```text
dist/windows/BMZ Player/
```

staging layout:

```text
BMZ Player/
  bmz-player.exe
  resources/
    bmz-player.ico
    skins/
      default/
      Rmz-skin/
      mz-select/
    songs/
      sample-playable/
    licenses/
      BMZ-GPL-3.0-only.txt
      license-notes.md
```

`bmz-player.exe` の隣の `resources` が runtime の `resource_dir` になる。
`config.toml`, `library.db`, `profiles`, `score.db`, `replay` などのユーザー状態は
installer に含めず、既存の Windows path 解決で `data_dir` 側へ作成する。

Inno Setup installer まで作る:

```powershell
.\scripts\package-windows.ps1 -Installer
```

Inno Setup の script は `installer/inno/bmz-player.iss`。既定では将来の自動更新を
入れやすいよう、per-user install として
`%LOCALAPPDATA%\Programs\BMZ Player` へインストールする。`Program Files` へ入れる
per-machine installer は UAC が必要になりやすいため、現時点では既定にしない。
installer 本体と shortcut の icon は `assets/app-icon/bmz-player.ico` を使う。

既定の installer 出力先:

```text
dist/windows/installer/bmz-player-<version>-windows-<arch>-setup.exe
```

### Windows options

debug build で作る:

```powershell
.\scripts\package-windows.ps1 -Profile Debug
```

ASIO SDK / LLVM 周りで Windows build が失敗する場合は default feature を外す:

```powershell
.\scripts\package-windows.ps1 -NoDefaultFeatures
```

出力先を変える:

```powershell
.\scripts\package-windows.ps1 -OutDir C:\tmp\bmz-package
```

Inno Setup compiler の path を指定する:

```powershell
.\scripts\package-windows.ps1 -Installer -IsccPath "C:\Program Files (x86)\Inno Setup 6\ISCC.exe"
```

DLL を追加で staging root へコピーする:

```powershell
.\scripts\package-windows.ps1 -DllDir C:\vcpkg\installed\x64-windows\bin
```

複数指定する場合:

```powershell
.\scripts\package-windows.ps1 -DllDir C:\vcpkg\installed\x64-windows\bin,C:\extra\dlls
```

環境変数でも指定できる:

```powershell
$env:BMZ_WINDOWS_DLL_DIRS = "C:\vcpkg\installed\x64-windows\bin;C:\extra\dlls"
.\scripts\package-windows.ps1
```

短い packaged smoke を実行する:

```powershell
.\scripts\package-windows.ps1 -Smoke
```

開発者の Windows PC では、このコマンドで生成

```powershell
.\scripts\package-windows.ps1 -DllDir C:\vcpkg\installed\x64-windows\bin -Installer -Iscc "C:\Users\hyrorre\AppData\Local\Programs\Inno Setup 6\ISCC.exe"
```


### Windows FFmpeg / DLL bundling

Windows で FFmpeg を dynamic link する build では、実行環境に必要な DLL が存在する
必要がある。配布用 artifact では `-DllDir` で必要な DLL を staging root へコピーする。

FFmpeg と codec library を配布物へ含める場合は、公開前に必ず `docs/licenses.md` を
確認し、FFmpeg の version / configure flags / source provenance /
`--enable-nonfree` 不使用を記録する。

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
      bmz-player.icns
      skins/
        default/
        Rmz-skin/
        mz-select/
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
Finder / Dock 上の icon は `Contents/Resources/bmz-player.icns` を
`Info.plist` の `CFBundleIconFile` で参照する。

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
`install_name_tool` は Mach-O の既存署名を無効化するため、署名指定が無い場合でも
script は ad-hoc 署名を自動で行う。これを行わないと Finder / Dock 起動時に
`Code Signature Invalid` / `Invalid Page` で dyld が落ちることがある。
ad-hoc 署名では bundled dylib が hardened runtime の library validation に弾かれるため、
script は Developer ID 署名時だけ hardened runtime option を付ける。

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

## GitHub Actions release build

`.github/workflows/release-apps.yml` は GitHub Release が `published` になったときに
release artifact を自動生成する。手動 dry run には `workflow_dispatch` を使う。

生成物:

```text
bmz-player-v<version>-windows-x64-setup.exe
bmz-player-v<version>-windows-x64-portable.zip
bmz-player-v<version>-windows-x64-provenance.txt
bmz-player-v<version>-macos-arm64.app.zip
bmz-player-v<version>-macos-x64.app.zip
bmz-player-v<version>-macos-<arch>-brew-ffmpeg.json
bmz-player-v<version>-macos-<arch>-ffmpeg-version.txt
SHA256SUMS.txt
```

## App update checks

BMZ Player は GitHub Releases を更新確認先として使う。Stable channel は
GitHub API の `releases/latest` を参照するため、draft / prerelease は対象外。
Prerelease channel は releases 一覧から最新の非 draft release を対象にする。

アプリ側の設定は `data/config.toml` の `[updates]` に保存する。

```toml
[updates]
enabled = true
channel = "Stable"
check_on_startup = true
skipped_version = ""
```

起動時チェックは release build の既定では有効、debug build の既定では無効。設定画面の
「アップデート」から手動確認できる。

更新が見つかった場合は Select 画面または設定画面で dialog を出し、ユーザーが
`アップデート` / `今回はアップデートしない` / `このリリースをスキップ` を選ぶ。
`今回はアップデートしない` はその起動中だけ抑止し、`このリリースをスキップ` は
`skipped_version` に保存して次の別 version まで通知しない。

自動適用 v1 は Windows installer artifact のみを対象にする。対象 asset は
`bmz-player-v<version>-windows-x64-setup.exe` を優先し、download 後に GitHub asset
`digest` または `SHA256SUMS.txt` の SHA256 と照合する。検証後に installer を起動し、
BMZ Player は通常の終了処理へ進む。

macOS `.app.zip` と Windows portable zip は、現時点では release page を開く手動更新に
留める。macOS の自動置換は Developer ID 署名 / notarization / helper の方針が固まってから
追加する。

release tag は `v0.1.0` のように `v` prefix 付きでもよいが、数値部分は
`Cargo.toml` の workspace version と一致する必要がある。手動実行では `tag` input
を指定する。`upload_to_release=false` なら Actions artifact の生成だけを行い、
GitHub Release には添付しない。

Windows job は public runner に ASIO SDK を用意しないため、既定で
`scripts/package-windows.ps1 -NoDefaultFeatures` を使う。自動 release build で
`asio` feature を有効化する場合は、SDK の取得・配置方法を別途用意してから拡張する。

macOS job は arm64 / x64 の app zip を別々に作る。現状は `--ad-hoc-sign` のため、
署名済み release を公開する前に Developer ID 署名と notarization 用の protected
GitHub secrets を追加する。

workflow は FFmpeg の package/version provenance を artifact と一緒に残し、
`ffmpeg -version` に `--enable-nonfree` が含まれる場合は失敗する。FFmpeg library を
bundle した artifact を公開する前に `docs/licenses.md` を確認する。
