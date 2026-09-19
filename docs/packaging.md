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
  bmz-updater.exe
  bmz-package.json
  resources/
    bmz-player.ico
    skins/
      default/
      Rmz-skin/
      mz-select/
      Luxez-Flat/
    songs/
      sample-playable/
    licenses/
      BMZ-GPL-3.0-only.txt
      license-notes.md
      third-party-notices.txt
      rust-dependency-licenses.txt
```

`bmz-player.exe` の隣の `resources` が runtime の `resource_dir` になる。
`config.toml`, `library.db`, `profiles`, `score.db`, `replay` などのユーザー状態は
installer に含めず、既存の Windows path 解決で `data_dir` 側へ作成する。

Inno Setup installer まで作る:

```powershell
.\scripts\package-windows.ps1 -Installer
```

通常releaseはGameInput backendを無効化しており、GameInput redistributableを同梱・実行しない。
GameInput実装は開発検証用の`experimental-gameinput` featureでのみコンパイルできる。

package script は `Cargo.toml` の workspace version を読み取り、
`installer/inno/bmz-player.iss` の `AppVersion` fallback と Inno Setup へ渡す
`/DAppVersion` を同期する。

Inno Setup の script は `installer/inno/bmz-player.iss`。既定では将来の自動更新を
入れやすいよう、per-user install として
`%LOCALAPPDATA%\Programs\BMZ Player` へインストールする。`Program Files` へ入れる
per-machine installer は UAC が必要になりやすいため、現時点では既定にしない。
installer 本体と shortcut の icon は `assets/app-icon/bmz-player.ico` を使う。

Rust crate の license report は `cargo-about` で
`resources/licenses/rust-dependency-licenses.txt` へ生成する。release artifact を作る
環境では先に入れておく:

```powershell
cargo install --locked --features cli cargo-about
```

egui のライセンス表記は `resources/licenses/third-party-notices.txt` と
`resources/licenses/rust-dependency-licenses.txt` を連結して表示する。

既定の installer 出力先:

```text
dist/windows/installer/bmz-player-<version>-windows-<arch>-setup.exe
```

### Windows options

GitHub Actions の Windows release job は `triplets/x64-windows-release.cmake` を
overlay triplet として使い、vcpkg の FFmpeg を Release-only でビルドする。

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

ローカルの動作確認で一時的に license report 生成を飛ばす:

```powershell
.\scripts\package-windows.ps1 -SkipRustLicenseReport
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

配布用 bundle の最低対応バージョンは Intel/x64 版が macOS 10.13、Apple Silicon
版が macOS 11.0 とする。Apple Silicon Mac と arm64 macOS 自体が 11.0 以降のため、
arm64 Mach-O を 10.13 target にはできない。package script は
`MACOSX_DEPLOYMENT_TARGET` と `LSMinimumSystemVersion` を同じ値に揃え、同梱した
すべての Mach-O が指定値より新しい deployment target を要求していないことを
`vtool` で検証する。検証値は `--minimum-system-version` または
`BMZ_MACOS_DEPLOYMENT_TARGET` で上書きできる。

CPAL 0.17 以降の CoreAudio backend は、出力専用アプリでも macOS 14.2 の process
tap 破棄シンボルを強参照する。BMZ は loopback 入力を使わないため、`bmz-audio` で
未到達の破棄処理を unsupported error へ解決し、古い OS での dyld load を妨げない。
package script は process tap と Continuity Camera のシンボルが配布物へ再混入して
いないことも検証する。

HTTP / WebSocket の TLS は Rustls と WebPKI roots を使う。Security.framework の
`SecTrustEvaluateWithError` は macOS 10.14 で導入されたため、10.13 向け配布物では
native-tls / SecureTransport の証明書検証経路をリンクしない。Keychain に認証情報を
保存する `keyring` の Apple native backend は引き続き使用する。

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
        Luxez-Flat/
      songs/
        sample-playable/
      licenses/
        BMZ-GPL-3.0-only.txt
        license-notes.md
        third-party-notices.txt
        rust-dependency-licenses.txt
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
Rust crate の license report は `cargo-about` で
`Contents/Resources/licenses/rust-dependency-licenses.txt` へ生成する。release
artifact を作る環境では先に入れておく:

```sh
cargo install --locked --features cli cargo-about
```

egui のライセンス表記は `Contents/Resources/licenses/third-party-notices.txt` と
`Contents/Resources/licenses/rust-dependency-licenses.txt` を連結して表示する。

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
Apple Developer Program未加入の場合も、このad-hoc署名とSparkleの更新署名で公開できる。
初回起動などで利用者による許可操作が必要になる場合があるため、
[Appleの案内](https://support.apple.com/ja-jp/102445)を参照する。
Developer ID署名・notarization・staplingは、証明書と認証情報を設定した場合に利用する。
また、macOS の code signing は resource file path も sealed resource として記録する。
`mz-select/customize/advanced` には説明用の 0 byte 日本語名ファイルが含まれるが、
zip / artifact 展開時の Unicode 正規化差分で resource seal が壊れることがあるため、
app bundle へコピーした後、署名前にこれらの空マーカーファイルを除外する。

短い packaged smoke を実行する:

```sh
scripts/package-macos-app.sh --smoke
```

ローカルの動作確認で一時的に license report 生成を飛ばす:

```sh
scripts/package-macos-app.sh --skip-rust-license-report
```

### FFmpeg / dylib bundling

既定では Homebrew など、実行環境に存在する dynamic libraries を使う。

公開用 macOS artifact では Homebrew の FFmpeg bottle を同梱しない。bottle の
deployment target は runner / Homebrew 更新に追従して変わるため、
`scripts/build-ffmpeg-macos.sh` で FFmpeg 9.0.1 の公式 source archive を検証して
artifact の最低対応 OS 向け shared library を作る。BMZ が使用しない `libavdevice` /
`libavfilter`、encoder / muxer、外部 codec library、network protocol、CLI program は
無効化する。

```sh
brew install nasm pkg-config
BMZ_MACOS_DEPLOYMENT_TARGET=10.13 \
scripts/build-ffmpeg-macos.sh \
  --target x86_64-apple-darwin \
  --prefix /tmp/bmz-ffmpeg-x64

FFMPEG_DIR=/tmp/bmz-ffmpeg-x64 \
PKG_CONFIG_PATH=/tmp/bmz-ffmpeg-x64/lib/pkgconfig \
scripts/package-macos-app.sh \
  --target x86_64-apple-darwin \
  --minimum-system-version 10.13 \
  --bundle-dylibs
```

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
git submodule update --init --recursive data/skins/Rmz-skin data/skins/mz-select data/skins/Luxez-Flat
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
    Luxez-Flat/
  songs/
    sample-playable/
  licenses/
    BMZ-GPL-3.0-only.txt
    license-notes.md
    third-party-notices.txt
    rust-dependency-licenses.txt  # pre-generated when available
/app/share/applications/net.hyrorre.BMZPlayer.desktop
/app/share/metainfo/net.hyrorre.BMZPlayer.metainfo.xml
/app/share/icons/hicolor/256x256/apps/net.hyrorre.BMZPlayer.png
```

`bmz-player-flatpak` wrapper が `BMZ_RESOURCE_DIR=/app/share/bmz-player` を設定する。
PulseAudio 接続用の `PULSE_COOKIE` が未指定なら、sandbox 内の
`$XDG_CONFIG_HOME/pulse/cookie` を指定し、初回起動時には 256 バイトの cookie を作成する。
これは `pulseaudio-rs 0.3.1` が XDG 設定先を参照せず、cookie が見つからない場合に
不正な長さの認証要求を送って ALSA へフォールバックする問題への対処。
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

## Linux tar.gz

正式ReleaseではUbuntu 22.04 / glibc 2.35+ x86_64用の実行用・対応ソースを別々に配布する。
`scripts/package-linux-tar.sh` と手動 **Optional Linux tar.gz** workflowも利用できる。
通常利用者は実行用だけを展開し、直下の `./bmz-player` を起動する。
ホスト要件、保存先、手動更新、対応ソースの再ビルドは [Linux tar.gz](linux-tar.md) を参照。
Flatpakの配布も継続する。

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
bmz-player-v<version>-macos-<arch>-ffmpeg-build.txt
bmz-player-v<version>-linux-x64.flatpak
bmz-player-v<version>-linux-x64-flatpak-provenance.txt
bmz-player-v<version>-linux-x64.tar.gz
bmz-player-v<version>-linux-x64-sources.tar.gz
SHA256SUMS.txt
```

GitHub Release には配布物、`SHA256SUMS.txt`、client manifest、署名付き `updates.json` を添付する。
Linux tarジョブは実行用単独の起動と対応ソースからのオフライン再ビルドを検証する。
全ジョブはmetadataで解決した同じcommitをcheckoutし、manifestにもそのcommitを記録する。
手動実行でもworkflow起動元の `GITHUB_SHA` を配布物のcommitとして使わない。
Linux tarを含む全ビルド成功後に集約する。dry runでは集約済みmanifestとチェックサムを
`verified-release-files` artifactとして保存し、Release・更新フィードへは公開しない。
`*-provenance.txt` / `*-ffmpeg-build.txt` は Actions artifact 側に残し、
Release asset には登録しない。

## App update checks

Windows は GitHub Releases を更新確認先として使い、SemVerで最新版を比較する。
Stable は draft / prerelease を除外し、Prerelease は非 draft release を対象にする。
macOS の更新対応bundleはSparkleのチャンネル・CPU別フィードを使う。

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

更新が見つかった場合は Select 画面で dialog を出し、ユーザーが
`アップデート` / `今回はアップデートしない` / `このリリースをスキップ` を選ぶ。
`今回はアップデートしない` はその起動中だけ抑止し、`このリリースをスキップ` は
`skipped_version` に保存して次の別 version まで通知しない。

Windows は `bmz-package.json` の形式に従ってinstaller/portableを選択する。
portableは署名付き更新情報とファイル一覧を検証し、専用helperで置換・復旧する。
macOS はSparkleで署名済み `.app.zip` を適用する。いずれも検証後にユーザーが
「更新して再起動」を選ぶ。旧版・開発ビルドなど形式不明の環境は手動更新を案内する。
署名キー、helper自身の更新、橋渡し版、復旧手順は [自動更新](auto-update.md) を参照。

release tag は `v0.1.0` のように `v` prefix 付きでもよいが、数値部分は
`Cargo.toml` の workspace version と一致する必要がある。手動実行では `tag` input
を指定する。`upload_to_release=false` なら Actions artifact の生成だけを行い、
GitHub Release には添付しない。

Windows job は `scripts/package-windows.ps1` を default features で実行するため、
release artifact は ASIO 対応を含む。`cpal/asio` が使う `asio-sys` build script は
ASIO SDK をビルド時に取得し、bindings 生成用に runner の LLVM `libclang` path を
`LIBCLANG_PATH` で明示する。

macOS job は arm64 / x64 の app zip を別々に作る。Apple Developer Program未加入でも
ad-hoc署名で公開できる。[自動更新](auto-update.md) に記載したWindows / Sparkleの
更新署名キーは公開時に必須とする。

Developer ID署名・公証を利用する場合だけ、次の任意secretsを設定する。

- `BMZ_MACOS_CODESIGN_IDENTITY`
- `BMZ_MACOS_CERTIFICATE_P12_BASE64`
- `BMZ_MACOS_CERTIFICATE_PASSWORD`
- `BMZ_MACOS_KEYCHAIN_PASSWORD`
- `BMZ_MACOS_NOTARY_APPLE_ID`
- `BMZ_MACOS_NOTARY_PASSWORD`
- `BMZ_MACOS_NOTARY_TEAM_ID`

署名用secretsが揃っている場合はDeveloper ID署名を使い、公証用secretsも揃っている場合は
notarization、stapling、`spctl` 検証を行ってから `.app.zip` を作る。
未設定ならad-hoc署名で作成し、`upload_to_release=true` でも公開できる。
どちらの場合も最終ZIPにはSparkleのEdDSA署名を付け、埋込み公開鍵との照合に成功してから公開する。

macOS の arm64 runner は `macos-15`、x64 runner は `macos-15-intel` を使う。
workflow matrix から arm64 には `MACOSX_DEPLOYMENT_TARGET=11.0`、x64 には
`MACOSX_DEPLOYMENT_TARGET=10.13` を Rust と制御ビルドした FFmpeg の双方へ渡す。
package 後には本体と全同梱 dylib の deployment target が指定値以下であることを
検証する。

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
