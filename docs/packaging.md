# Packaging

## App icons

BMZ Player の desktop app icon は `scripts/generate-app-icons.sh` で生成する。
web 用の元 icon は `bmz-ir-web/public/icon.svg` に置き、desktop 配布用は
Apple / Windows それぞれの template に合わせた source SVG を `assets/app-icon/`
配下に置く。

```sh
scripts/generate-app-icons.sh
```

生成先:

```text
assets/app-icon/
  bmz-player-apple.svg
  bmz-player-windows.svg
  bmz-player.png
  bmz-player-window.png
  bmz-player-window-windows.png
  bmz-player.ico
  bmz-player.icns
```

`bmz-player-window.png` は winit の実行時ウィンドウ icon として `bmz-player`
binary に埋め込む。Windows build では `bmz-player-window-windows.png` を
埋め込む。`bmz-player.ico` は Windows installer / shortcut 用、
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
      third-party-notices.txt
```

`bmz-player.exe` の隣の `resources` が runtime の `resource_dir` になる。
`config.toml`, `library.db`, `profiles`, `score.db`, `replay` などのユーザー状態は
installer に含めず、既存の Windows path 解決で `data_dir` 側へ作成する。

Inno Setup installer まで作る:

```powershell
.\scripts\package-windows.ps1 -Installer
```

package script は `Cargo.toml` の workspace version を読み取り、
`installer/inno/bmz-player.iss` の `AppVersion` fallback と Inno Setup へ渡す
`/DAppVersion` を同期する。

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

`-DllDir` と `BMZ_WINDOWS_DLL_DIRS` を省略した場合は、`repo\vcpkg_installed`,
`VCPKG_ROOT`, PATH 上の `vcpkg`, Scoop の `~/scoop/apps/vcpkg/current`,
`C:\vcpkg` の順で vcpkg の `installed\<triplet>\bin` を探し、見つかった DLL を
staging root へコピーする。Scoop で入れた vcpkg も通常は自動検出される。

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

既定の `CFBundleIdentifier` は、所有ドメイン `hyrorre.net` に合わせて
`net.hyrorre.bmz-player` とする。必要な場合は `--bundle-id` または
`BMZ_MACOS_BUNDLE_ID` で上書きする。

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
        third-party-notices.txt
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

Developer ID 署名時は hardened runtime と secure timestamp を付ける。GitHub
Actions などで作った `.app.zip` はダウンロード時に quarantine が付くため、ad-hoc
署名だけの `.app` は Gatekeeper により「壊れている」と表示されることがある。
通常のダブルクリック起動で配布する release artifact は Developer ID 署名後に
notarization と stapling を行う。
また、macOS の code signing は resource file path も sealed resource として記録する。
`mz-select/customize/advanced` には説明用の 0 byte 日本語名ファイルが含まれるが、
zip / artifact 展開時の Unicode 正規化差分で resource seal が壊れることがあるため、
app bundle へコピーした後、署名前にこれらの空マーカーファイルを除外する。

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

## Linux Flatpak

Linux の Flatpak manifest と desktop integration file は `installer/flatpak/` に置く。
Flatpak app id は、所有ドメイン `hyrorre.net` に合わせて
`net.hyrorre.BMZPlayer` とする。

必要な runtime / SDK:

```sh
flatpak remote-add --if-not-exists --user flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user flathub org.freedesktop.Platform//25.08 org.freedesktop.Sdk//25.08 org.freedesktop.Sdk.Extension.rust-stable//25.08 org.freedesktop.Sdk.Extension.llvm22//25.08
```

submodule skin が空の場合は先に初期化する:

```sh
git submodule update --init --recursive data/skins/Rmz-skin data/skins/mz-select
```

Flatpak bundle を作る:

```sh
scripts/package-flatpak.sh
```

package script は `Cargo.toml` の workspace version を読み取り、
`installer/flatpak/net.hyrorre.BMZPlayer.metainfo.xml` の先頭 `<release>` version を
ビルド前に同期する。同期だけを行う場合は次を使う:

```sh
scripts/package-flatpak.sh --sync-metadata-only
```

既定の出力先:

```text
dist/flatpak/bmz-player-<version>-linux.flatpak
```

インストールと smoke test まで行う:

```sh
scripts/package-flatpak.sh --install --smoke
```

.flatpak bundle を入れ直す:

```sh
flatpak install --user --reinstall ./bmz-player-v0.1.3-linux-x64.flatpak
```

古いローカル origin remote が残り、GUI で古い version への更新通知が出る場合は、
app と bundle 由来 remote を消してから入れ直す:

```sh
flatpak uninstall --user -y net.hyrorre.BMZPlayer
flatpak remote-delete --user bmzplayer1-origin 2>/dev/null || true
flatpak remote-delete --user debug-origin 2>/dev/null || true
flatpak install --user -y ./bmz-player-v0.1.3-linux-x64.flatpak
```

手動で実行する:

```sh
flatpak run net.hyrorre.BMZPlayer
flatpak run net.hyrorre.BMZPlayer --boot-play-sample --smoke-exit-after-frames 3
```

Flatpak layout:

```text
/app/bin/
  bmz-player
  bmz-player-flatpak
/app/share/bmz-player/
  skins/
    default/
    Rmz-skin/
    mz-select/
  songs/
    sample-playable/
  licenses/
    BMZ-GPL-3.0-only.txt
    license-notes.md
    third-party-notices.txt
/app/share/applications/net.hyrorre.BMZPlayer.desktop
/app/share/metainfo/net.hyrorre.BMZPlayer.metainfo.xml
/app/share/icons/hicolor/256x256/apps/net.hyrorre.BMZPlayer.png
```

`bmz-player-flatpak` wrapper が `BMZ_RESOURCE_DIR=/app/share/bmz-player` を設定する。
`config.toml`, `library.db`, `profiles`, `score.db`, `replay` などのユーザー状態は
Flatpak sandbox の XDG path に作られる。通常は host 側の
`~/.var/app/net.hyrorre.BMZPlayer/` 配下になる。
`finish-args` は Wayland / fallback X11 / DRI / PulseAudio / network / input device を
許可する。`--device=input` はゲームパッド入力用。

現在の manifest はローカル配布 bundle を作りやすくするため、build 時に Cargo が
crate を取得できるよう `build-args: --share=network` を使う。Flathub へ提出する場合は
`flatpak-cargo-generator.py` などで `Cargo.lock` から cargo source manifest を生成し、
network build を外す。

FFmpeg は `ffmpeg-next` 経由で音声/動画 decode に使う。Flatpak artifact を公開する前に
実際に含まれる FFmpeg library の version / configure flags / license を確認し、
`docs/licenses.md` の release checklist に従う。`--enable-nonfree` を含む FFmpeg build は
配布物に含めない。

## GitHub Actions release build

`.github/workflows/release-apps.yml` は GitHub Release が `published` になったときに
release artifact を自動生成する。手動 dry run には `workflow_dispatch` を使う。

CI 内で生成される package / provenance artifact:

```text
bmz-player-v<version>-windows-x64-setup.exe
bmz-player-v<version>-windows-x64-portable.zip
bmz-player-v<version>-windows-x64-provenance.txt
bmz-player-v<version>-macos-arm64.app.zip
bmz-player-v<version>-macos-x64.app.zip
bmz-player-v<version>-macos-<arch>-brew-ffmpeg.json
bmz-player-v<version>-macos-<arch>-ffmpeg-version.txt
bmz-player-v<version>-linux-x64.flatpak
bmz-player-v<version>-linux-x64-flatpak-provenance.txt
SHA256SUMS.txt
```

GitHub Release に添付するのは、ユーザーが選ぶ配布物と `SHA256SUMS.txt` のみ。
`*-provenance.txt` / `*-ffmpeg-version.txt` / `*-brew-ffmpeg.json` は Actions
artifact 側に残し、Release asset には登録しない。

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

Windows job は `scripts/package-windows.ps1` を default features で実行するため、
release artifact は ASIO 対応を含む。`cpal/asio` が使う `asio-sys` build script は
ASIO SDK をビルド時に取得し、bindings 生成用に runner の LLVM `libclang` path を
`LIBCLANG_PATH` で明示する。

macOS job は arm64 / x64 の app zip を別々に作る。現状は `--ad-hoc-sign` のため、
Developer ID 署名と notarization 用の protected GitHub secrets が無い場合、Actions
artifact は quarantine 付き環境で通常起動できないことがある。署名済み release を
公開する場合は次の secrets を設定する。

- `BMZ_MACOS_CODESIGN_IDENTITY`
- `BMZ_MACOS_CERTIFICATE_P12_BASE64`
- `BMZ_MACOS_CERTIFICATE_PASSWORD`
- `BMZ_MACOS_KEYCHAIN_PASSWORD`
- `BMZ_MACOS_NOTARY_APPLE_ID`
- `BMZ_MACOS_NOTARY_PASSWORD`
- `BMZ_MACOS_NOTARY_TEAM_ID`

secrets が揃っている場合、macOS job は Developer ID 署名、notarization、stapling、
`spctl` 検証を行ってから `.app.zip` を作る。無い場合は従来通り ad-hoc 署名で
artifact を作る。

Linux job は Flatpak 用 container
`ghcr.io/flathub-infra/flatpak-github-actions:freedesktop-25.08` で
`scripts/package-flatpak.sh` を実行し、`net.hyrorre.BMZPlayer` として
`flatpak run ... --help` まで確認する。Actions cache は `.flatpak-builder` のみに
限定し、`dist/flatpak/repo` は cache しない。release bundle へ古い Flatpak repo /
AppStream / commit 履歴を混ぜないため、package script は build dir と repo dir を
ビルド前に作り直す。

CI では package script で Flatpak metainfo version を同期してから、
`Cargo.toml` / release tag / `BMZ_VERSION` と
`installer/flatpak/net.hyrorre.BMZPlayer.metainfo.xml` の先頭 `<release>` version が
一致することも検証する。現状の manifest は build 時 network access を使うため、
Flathub 提出前に `flatpak-cargo-generator.py` などで Cargo source を固定する。

workflow は FFmpeg の package/version provenance を artifact と一緒に残し、
`ffmpeg -version` に `--enable-nonfree` が含まれる場合は失敗する。FFmpeg library を
bundle した artifact を公開する前に `docs/licenses.md` を確認する。
