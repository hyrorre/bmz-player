# 配布パッケージ

## アプリのアイコン

BMZ Playerのdesktop app iconは`scripts/generate-app-icons.sh`で生成します。
web用の元iconは`bmz-ir-web/public/icon.svg`に置き、desktop配布用は
Apple / Windowsそれぞれのtemplateに合わせたsource SVGを`assets/app-icon/`
配下に置きます。

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

`bmz-player-window.png`はwinitの実行時ウィンドウiconとして`bmz-player`
binaryに埋め込みます。Windows buildでは`bmz-player-window-windows.png`を
埋め込み、`bmz-player.ico`を実行ファイルのリソースiconとして登録します。
同じ`.ico`をWindows installer / shortcutにも使い、
`bmz-player.icns`はmacOS `.app` bundle用です。

## Windowsインストーラー（Inno Setup）

Windowsのstaging directoryとInno Setup installerは`scripts/package-windows.ps1`
で作ります。

PowerShell 5.1以降で実行する:

```powershell
.\scripts\package-windows.ps1
```

既定のstaging出力先:

```text
dist/windows/BMZ Player/
```

staging layout:

```text
BMZ Player/
  bmz-player.exe
  updater/
    bmz-updater.exe
    bmz-package.json
    instance.lock
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

`bmz-player.exe`の隣の`resources`がruntimeの`resource_dir`になります。
`config.toml`, `library.db`, `profiles`, `score.db`, `replay`などのユーザー状態は
installer / ZIPに含めず、起動時に`data_dir`側へ作成します。

### Windows の保存先と優先順位

空でない`BMZ_DATA_DIR` / `BMZ_CACHE_DIR` / `BMZ_LOGS_DIR` / `BMZ_RESOURCE_DIR`は、
配布形式にかかわらず、それぞれの保存先を最優先で上書きします。相対指定は起動時の
作業フォルダを基準とするため、固定したい場合は絶対パスを指定します。

| 実行形態 | `BMZ_DATA_DIR`未指定時の`data_dir` |
| --- | --- |
| portable | exe横の`data`です。未作成でも必ずこの場所を選び、起動時に作成する |
| installer | exe横に`data`が既にあれば使用し、なければ`%APPDATA%\BMZ Player` |
| Cargo開発実行 | exe横に`data`が既にあれば使用し、なければワークスペースの`data` |

portable / installerはexe横の`updater/bmz-package.json`（旧配置は`bmz-package.json`）
の`kind`で識別します。exeのバージョン差で保存先を切り替えません。新配置を優先し、
存在するメタデータが壊れていれば起動をエラーにします。メタデータのない旧配布は従来の
探索順を維持します。開発実行は実際のワークスペースのCargo出力であることを確認します。

正式なWindowsパッケージは作業フォルダの`data`を探索しません。
portableの`data`作成失敗時はエラーとし、AppDataへ切り替えません。
既存のAppDataの設定・DBを自動移動・コピー・削除しないため、exe横の`data`が
空なら新規データとして起動します。利用者が明示した曲ルートやスクリーンショットの
絶対パスは書き換えません。

`BMZ_DATA_DIR`指定時、exe横の`data`使用時、開発実行時は、キャッシュとログの
既定値も選択した`data_dir/cache`と`data_dir/logs`にまとめます。
installerがAppDataを使用する場合のみ`%LOCALAPPDATA%\BMZ Player\cache`と
`%LOCALAPPDATA%\BMZ Player\logs`を既定値にします。それぞれの環境変数で個別に上書き可能です。
`resource_dir`は配布版ではexe横の`resources`、開発時はexe横の`resources`が
あればそれを使用し、なければワークスペースの`data`を使用します。
macOS / Linuxの配布パス解決は変更しません。

### Windows updater の配置

通常は`-UpdaterLayout grouped`（既定値）で、updater本体・メタデータ・ロック・
更新作業を`updater/`へまとめます。`data`の上書き先が別ドライブでも、更新作業領域は
exeと同じボリュームに置きます。`updater/update.lock`、`active.json`、`job-*`は実行時の
状態であり、配布物には含めません。`-Smoke`はパッケージの一時コピーで起動し、
検証用データをstagingへ生成しません。

旧updaterから自動更新可能な橋渡し版を作る場合のみ`-UpdaterLayout legacy`を指定します。
この場合は旧配置・package protocol 1のまま、新旧配置対応のprotocol 2バイナリを同梱します。
公開順序と`BMZ_UPDATE_BRIDGE_TAG`の設定は[自動更新](auto-update.md)を参照してください。

Inno Setup installerまで作る:

```powershell
.\scripts\package-windows.ps1 -Installer
```

通常releaseはGameInputバックエンドを無効化しており、GameInput redistributableを同梱・実行しません。
GameInput実装は開発検証用の`experimental-gameinput` featureでのみコンパイルできます。

package scriptは`Cargo.toml`のworkspace versionを読み取り、
`installer/inno/bmz-player.iss`の`AppVersion` fallbackとInno Setupへ渡す
`/DAppVersion`を同期します。

Inno Setupのscriptは`installer/inno/bmz-player.iss`です。既定では将来の自動更新を
入れやすいよう、per-user installとして
`%LOCALAPPDATA%\Programs\BMZ Player`へインストールします。`Program Files`へ入れる
per-machine installerはUACが必要になりやすいため、現時点では既定にしません。
installer本体とshortcutのiconは`assets/app-icon/bmz-player.ico`を使います。

Rust crateのlicense reportは`cargo-about`で
`resources/licenses/rust-dependency-licenses.txt`へ生成します。release artifactを作る
環境では先に入れておく:

```powershell
cargo install --locked --features cli cargo-about
```

eguiのライセンス表記は`resources/licenses/third-party-notices.txt`と
`resources/licenses/rust-dependency-licenses.txt`を連結して表示します。

既定のinstaller出力先:

```text
dist/windows/installer/bmz-player-<version>-windows-<arch>-setup.exe
```

### Windowsのオプション

GitHub ActionsのWindows release jobは`triplets/x64-windows-release.cmake`を
overlay tripletとして使い、vcpkgのFFmpegをRelease-onlyでビルドします。

debug buildで作る:

```powershell
.\scripts\package-windows.ps1 -Profile Debug
```

ASIO SDK / LLVM周りでWindows buildが失敗する場合はdefault featureを外す:

```powershell
.\scripts\package-windows.ps1 -NoDefaultFeatures
```

出力先を変える:

```powershell
.\scripts\package-windows.ps1 -OutDir C:\tmp\bmz-package
```

Inno Setup compilerのpathを指定する:

```powershell
.\scripts\package-windows.ps1 -Installer -IsccPath "C:\Program Files (x86)\Inno Setup 6\ISCC.exe"
```

DLLを追加でstaging rootへコピーする:

```powershell
.\scripts\package-windows.ps1 -DllDir C:\vcpkg\installed\x64-windows\bin
```

`-DllDir`と`BMZ_WINDOWS_DLL_DIRS`を省略した場合は、`repo\vcpkg_installed`,
`VCPKG_ROOT`, PATH上の`vcpkg`, Scoopの`~/scoop/apps/vcpkg/current`,
`C:\vcpkg`の順でvcpkgの`installed\<triplet>\bin`を探し、見つかったDLLを
staging rootへコピーします。Scoopで入れたvcpkgも通常は自動検出されます。

複数指定する場合:

```powershell
.\scripts\package-windows.ps1 -DllDir C:\vcpkg\installed\x64-windows\bin,C:\extra\dlls
```

環境変数でも指定できる:

```powershell
$env:BMZ_WINDOWS_DLL_DIRS = "C:\vcpkg\installed\x64-windows\bin;C:\extra\dlls"
.\scripts\package-windows.ps1
```

短いpackaged smokeを実行する:

```powershell
.\scripts\package-windows.ps1 -Smoke
```

ローカルの動作確認で一時的にlicense report生成を飛ばす:

```powershell
.\scripts\package-windows.ps1 -SkipRustLicenseReport
```

### WindowsでのFFmpeg / DLLの同梱

WindowsでFFmpegをdynamic linkするbuildでは、実行環境に必要なDLLが存在する
必要があります。配布用artifactでは`-DllDir`で必要なDLLをstaging rootへコピーします。

FFmpegとcodec libraryを配布物へ含める場合は、公開前に必ず`docs/licenses.md`を
確認し、FFmpegのversion / configure flags / source provenance /
`--enable-nonfree`不使用を記録します。

## macOS `.app`

BMZ PlayerのmacOS app bundleは`scripts/package-macos-app.sh`で作ります。

```sh
scripts/package-macos-app.sh
```

配布用bundleの最低対応バージョンはIntel/x64版がmacOS 10.13、Apple Silicon
版がmacOS 11.0とします。Apple Silicon Macとarm64 macOS自体が11.0以降のため、
arm64 Mach-Oを10.13 targetにはできません。package scriptは
`MACOSX_DEPLOYMENT_TARGET`と`LSMinimumSystemVersion`を同じ値に揃え、同梱した
すべてのMach-Oが指定値より新しいdeployment targetを要求していないことを
`vtool`で検証します。検証値は`--minimum-system-version`または
`BMZ_MACOS_DEPLOYMENT_TARGET`で上書きできます。

CPAL 0.17以降のCoreAudioバックエンド は、出力専用アプリでもmacOS 14.2のprocess
tap破棄シンボルを強参照します。BMZはloopback入力を使わないため、`bmz-audio`で
未到達の破棄処理をunsupported errorへ解決し、古いOSでのdyld loadを妨げません。
package scriptはprocess tapとContinuity Cameraのシンボルが配布物へ再混入して
いないことも検証します。

HTTP / WebSocketのTLSはRustlsとWebPKI rootsを使います。Security.frameworkの
`SecTrustEvaluateWithError`はmacOS 10.14で導入されたため、10.13向け配布物では
native-tls / SecureTransportの証明書検証経路をリンクしません。Keychainに認証情報を
保存する`keyring`のApple nativeバックエンド は引き続き使用します。

既定の`CFBundleIdentifier`は、所有ドメイン`hyrorre.net`に合わせて
`net.hyrorre.bmz-player`とします。必要な場合は`--bundle-id`または
`BMZ_MACOS_BUNDLE_ID`で上書きします。

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

`Contents/Resources`がruntimeの`resource_dir`になります。`config.toml`,
`library.db`, `profiles`, `score.db`, `replay`などのユーザー状態はbundle
に含めず、`data_dir`側へ作成します。
Finder / Dock上のiconは`Contents/Resources/bmz-player.icns`を
`Info.plist`の`CFBundleIconFile`で参照します。

同梱スキンは`resource_dir/skins`に置きます。編集したい場合はbundle内を直接
変更せず、ユーザーが`data_dir/skins`へコピーしてユーザースキンとして選びます。
Rust crateのlicense reportは`cargo-about`で
`Contents/Resources/licenses/rust-dependency-licenses.txt`へ生成します。release
artifactを作る環境では先に入れておく:

```sh
cargo install --locked --features cli cargo-about
```

eguiのライセンス表記は`Contents/Resources/licenses/third-party-notices.txt`と
`Contents/Resources/licenses/rust-dependency-licenses.txt`を連結して表示します。

### オプション

debug buildで作る:

```sh
scripts/package-macos-app.sh --debug
```

出力先を変える:

```sh
scripts/package-macos-app.sh --out-dir /tmp/bmz-package
```

ad-hoc署名する:

```sh
scripts/package-macos-app.sh --ad-hoc-sign
```

Developer IDで署名する:

```sh
scripts/package-macos-app.sh --sign "Developer ID Application: ..."
```

Developer ID署名時はhardened runtimeとsecure timestampを付けます。GitHub
Actionsなどで作った`.app.zip`はダウンロード時にquarantineが付くため、ad-hoc
署名だけの`.app`はGatekeeperにより「壊れている」と表示されることがあります。
Apple Developer Program未加入の場合も、このad-hoc署名とSparkleの更新署名で公開できます。
初回起動などで利用者による許可操作が必要になる場合があるため、
[Appleの案内](https://support.apple.com/ja-jp/102445)を参照します。
Developer ID署名・notarization・staplingは、証明書と認証情報を設定した場合に利用します。
また、macOSのcode signingはresource file pathもsealed resourceとして記録します。
`mz-select/customize/advanced`には説明用の0 byte日本語名ファイルが含まれるが、
zip / artifact展開時のUnicode正規化差分でresource sealが壊れることがあるため、
app bundleへコピーした後、署名前にこれらの空マーカーファイルを除外します。

短いpackaged smokeを実行する:

```sh
scripts/package-macos-app.sh --smoke
```

ローカルの動作確認で一時的にlicense report生成を飛ばす:

```sh
scripts/package-macos-app.sh --skip-rust-license-report
```

### FFmpeg / dylibの同梱

既定ではHomebrewなど、実行環境に存在するdynamic librariesを使います。

公開用macOS artifactではHomebrewのFFmpeg bottleを同梱しません。bottleの
deployment targetはrunner / Homebrew更新に追従して変わるため、
`scripts/build-ffmpeg-macos.sh`でFFmpeg 9.0.1の公式source archiveを検証して
artifactの最低対応OS向けshared libraryを作ります。BMZが使用しない`libavdevice` /
`libavfilter`、encoder / muxer、外部codec library、network protocol、CLI programは
無効化します。

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

`--bundle-dylibs`を付けると、`otool`で見える非system dylib依存を
`Contents/Frameworks`へコピーし、`install_name_tool`で参照を書き換えます。
`install_name_tool`はMach-Oの既存署名を無効化するため、署名指定が無い場合でも
scriptはad-hoc署名を自動で行います。これを行わないとFinder / Dock起動時に
`Code Signature Invalid` / `Invalid Page`でdyldが落ちることがあります。
ad-hoc署名ではbundled dylibがhardened runtimeのlibrary validationに弾かれるため、
scriptはDeveloper ID署名時だけhardened runtime optionを付けます。

```sh
scripts/package-macos-app.sh --bundle-dylibs --ad-hoc-sign
```

このoptionはFFmpegとcodec libraryを配布物へ含める可能性があります。
公開用artifactを作る前に必ず`docs/licenses.md`を確認し、FFmpegの
version / configure flags / source provenance / `--enable-nonfree`不使用を記録します。

### 手動のsmoke確認

既に作った`.app`を直接起動してsmokeする場合:

```sh
BMZ_DATA_DIR=/tmp/bmz-player-package-data \
  "dist/macos/BMZ Player.app/Contents/MacOS/bmz-player" \
  --boot-play-sample \
  --smoke-exit-after-frames 3
```

## Linux Flatpak

LinuxのFlatpak manifestとdesktop integration fileは`installer/flatpak/`に置きます。
Flatpak app idは、所有ドメイン`hyrorre.net`に合わせて
`net.hyrorre.BMZPlayer`とします。

必要なruntime / SDK:

```sh
flatpak remote-add --if-not-exists --user flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user flathub org.freedesktop.Platform//25.08 org.freedesktop.Sdk//25.08 org.freedesktop.Sdk.Extension.rust-stable//25.08 org.freedesktop.Sdk.Extension.llvm22//25.08
```

submodule skinが空の場合は先に初期化する:

```sh
git submodule update --init --recursive data/skins/Rmz-skin data/skins/mz-select data/skins/Luxez-Flat
```

Flatpak bundleを作る:

```sh
scripts/package-flatpak.sh
```

package scriptは`Cargo.toml`のworkspace versionを読み取り、
`installer/flatpak/net.hyrorre.BMZPlayer.metainfo.xml`の先頭`<release>` versionを
ビルド前に同期します。同期だけを行う場合は次を使う:

```sh
scripts/package-flatpak.sh --sync-metadata-only
```

既定の出力先:

```text
dist/flatpak/bmz-player-<version>-linux.flatpak
```

インストールとsmoke testまで行う:

```sh
scripts/package-flatpak.sh --install --smoke
```

.flatpak bundleを入れ直す:

```sh
flatpak install --user --reinstall ./bmz-player-v0.1.3-linux-x64.flatpak
```

古いローカルorigin remoteが残り、GUIで古いversionへの更新通知が出る場合は、
appとbundle由来remoteを消してから入れ直す:

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

`bmz-player-flatpak` wrapperが`BMZ_RESOURCE_DIR=/app/share/bmz-player`を設定します。
PulseAudio接続用の`PULSE_COOKIE`が未指定なら、sandbox内の
`$XDG_CONFIG_HOME/pulse/cookie`を指定し、初回起動時には256バイトのcookieを作成します。
これは`pulseaudio-rs 0.3.1`がXDG設定先を参照せず、cookieが見つからない場合に
不正な長さの認証要求を送ってALSAへフォールバックする問題への対処です。
`config.toml`, `library.db`, `profiles`, `score.db`, `replay`などのユーザー状態は
Flatpak sandboxのXDG pathに作られます。通常はhost側の
`~/.var/app/net.hyrorre.BMZPlayer/`配下になります。
`finish-args`はWayland / fallback X11 / DRI / PulseAudio / network / input deviceを
許可します。`--device=input`はゲームパッド入力用（最低Flatpak 1.15.6）です。
`pulseaudio,pipewire`をビルドし、native PipeWireには`xdg-run/pipewire-0`だけを公開します。
SDK/runtime 25.08のクライアントライブラリ・moduleを利用します。サーバー側のFlatpak分類と
アクセス制限も必要で、manager socketや全bus/deviceは公開しません。
ビルド・権限確認・Pulseへの復旧は[Linux遅延検証](linux-latency.md)を参照します。

現在のmanifestはローカル配布bundleを作りやすくするため、build時にCargoが
crateを取得できるよう`build-args: --share=network`を使います。Flathubへ提出する場合は
`flatpak-cargo-generator.py`などで`Cargo.lock`からcargo source manifestを生成し、
network buildを外します。

FFmpegは`ffmpeg-next`経由で音声/動画decodeに使います。Flatpak artifactを公開する前に
実際に含まれるFFmpeg libraryのversion / configure flags / licenseを確認し、
`docs/licenses.md`のrelease checklistに従います。`--enable-nonfree`を含むFFmpeg buildは
配布物に含めません。

## Linux tar.gz

正式ReleaseではUbuntu 22.04 / glibc 2.35+ x86_64用の実行用・対応ソースを別々に配布します。
`scripts/package-linux-tar.sh`と手動**Optional Linux tar.gz** workflowも利用できます。
通常利用者は実行用だけを展開し、直下の`./bmz-player`を起動します。
ホスト要件、保存先、手動更新、対応ソースの再ビルドは[Linux tar.gz](linux-tar.md)を参照してください。
Flatpakの配布も継続します。

## GitHub Actionsのリリースビルド

`.github/workflows/release-apps.yml`はGitHub Releaseが`published`になったときに
release artifactを自動生成します。手動dry runには`workflow_dispatch`を使います。

CI内で生成されるpackage / provenance artifact:

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

GitHub Releaseには配布物、`SHA256SUMS.txt`、署名付き`release.json`、旧版互換用の
`updates.json`を添付します。独立したclient manifestは添付せず、rianIRは`release.json`の
`builds`を取り込みます。形式と橋渡し版の公開手順は[Releaseメタデータ](release-metadata.md)を参照してください。
Linux tarジョブは実行用単独の起動と対応ソースからのオフライン再ビルドを検証します。
全ジョブはmetadataで解決した同じcommitをcheckoutし、manifestにもそのcommitを記録します。
手動実行でもworkflow起動元の`GITHUB_SHA`を配布物のcommitとして使いません。
Linux tarを含む全ビルド成功後に集約します。dry runでは集約済みmanifestとチェックサムを
`verified-release-files` artifactとして保存し、Release・更新フィードへは公開しません。
dry runの`release.json`は`signature: null`を持つ未署名の確認用データで、自動更新では拒否します。
公開用署名キーを使わず、`updates.json`も生成しません。
`*-provenance.txt` / `*-ffmpeg-build.txt`はActions artifact側に残し、
Release assetには登録しません。

## アプリの更新確認

WindowsはGitHub Releasesを更新確認先として使い、SemVerで最新版を比較します。
Stableはdraft / prereleaseを除外し、Prereleaseは非draft releaseを対象にします。
macOSの更新対応bundleはSparkleのチャンネル・CPU別フィードを使います。

アプリ側の設定は`data/config.toml`の`[updates]`に保存します。

```toml
[updates]
enabled = true
channel = "Stable"
check_on_startup = true
skipped_version = ""
```

起動時チェックはrelease buildの既定では有効、debug buildの既定では無効です。設定画面の
「アップデート」から手動確認できます。

更新が見つかった場合は選曲画面でdialogを出し、ユーザーが
`アップデート` / `今回はアップデートしない` / `このリリースをスキップ`を選びます。
`今回はアップデートしない`はその起動中だけ抑止し、`このリリースをスキップ`は
`skipped_version`に保存して次の別versionまで通知しません。

Windowsは`updater/bmz-package.json`（旧配置は直下）の形式に従ってinstaller/portableを選択します。
portableは署名付き更新情報とファイル一覧を検証し、専用helperで置換・復旧します。
macOSはSparkleで署名済み`.app.zip`を適用します。いずれも検証後にユーザーが
「更新して再起動」を選びます。旧版・開発ビルドなど形式不明の環境は手動更新を案内します。
署名キー、helper自身の更新、橋渡し版、復旧手順は[自動更新](auto-update.md)を参照してください。

release tagは`v0.1.0`のように`v` prefix付きでもよいが、数値部分は
`Cargo.toml`のworkspace versionと一致する必要があります。手動実行では`tag` input
を指定します。`upload_to_release=false`ならActions artifactの生成だけを行い、
GitHub Releaseには添付しません。

Windows jobは`scripts/package-windows.ps1`をdefault featuresで実行するため、
release artifactはASIO対応を含みます。`cpal/asio`が使う`asio-sys` build scriptは
ASIO SDKをビルド時に取得し、bindings生成用にrunnerのLLVM `libclang` pathを
`LIBCLANG_PATH`で明示します。

macOS jobはarm64 / x64のapp zipを別々に作ります。Apple Developer Program未加入でも
ad-hoc署名で公開できます。[自動更新](auto-update.md)に記載したWindows / Sparkleの
更新署名キーは公開時に必須とします。

Developer ID署名・公証を利用する場合だけ、次の任意secretsを設定します。

- `BMZ_MACOS_CODESIGN_IDENTITY`
- `BMZ_MACOS_CERTIFICATE_P12_BASE64`
- `BMZ_MACOS_CERTIFICATE_PASSWORD`
- `BMZ_MACOS_KEYCHAIN_PASSWORD`
- `BMZ_MACOS_NOTARY_APPLE_ID`
- `BMZ_MACOS_NOTARY_PASSWORD`
- `BMZ_MACOS_NOTARY_TEAM_ID`

署名用secretsが揃っている場合はDeveloper ID署名を使い、公証用secretsも揃っている場合は
notarization、stapling、`spctl`検証を行ってから`.app.zip`を作ります。
未設定ならad-hoc署名で作成し、`upload_to_release=true`でも公開できます。
どちらの場合も最終ZIPにはSparkleのEdDSA署名を付け、埋込み公開鍵との照合に成功してから公開します。

macOSのarm64 runnerは`macos-15`、x64 runnerは`macos-15-intel`を使います。
workflow matrixからarm64には`MACOSX_DEPLOYMENT_TARGET=11.0`、x64には
`MACOSX_DEPLOYMENT_TARGET=10.13`をRustと制御ビルドしたFFmpegの双方へ渡します。
package後には本体と全同梱dylibのdeployment targetが指定値以下であることを
検証します。

Linux jobはFlatpak用container
`ghcr.io/flathub-infra/flatpak-github-actions:freedesktop-25.08`で
`scripts/package-flatpak.sh`を実行し、`net.hyrorre.BMZPlayer`として
`flatpak run ... --help`まで確認します。Actions cacheは`.flatpak-builder`のみに
限定し、`dist/flatpak/repo`はcacheしません。release bundleへ古いFlatpak repo /
AppStream / commit履歴を混ぜないため、package scriptはbuild dirとrepo dirを
ビルド前に作り直します。

CIではpackage scriptでFlatpak metainfo versionを同期してから、
`Cargo.toml` / release tag / `BMZ_VERSION`と
`installer/flatpak/net.hyrorre.BMZPlayer.metainfo.xml`の先頭`<release>` versionが
一致することも検証します。現状のmanifestはbuild時network accessを使うため、
Flathub提出前に`flatpak-cargo-generator.py`などでCargo sourceを固定します。

workflowはFFmpegのpackage/version provenanceをartifactと一緒に残し、
`ffmpeg -version`に`--enable-nonfree`が含まれる場合は失敗します。FFmpeg libraryを
bundleしたartifactを公開する前に`docs/licenses.md`を確認します。
## macOSの入力・時計API

Core Audio IOProc音声は`experimental-coreaudio-ioproc` featureで有効にする実験用backendです。
default featureに含まれ、通常のmacOSビルド・配布で選択できます。
`Auto`と`Core Audio`はCPAL / Audio Unit経路を使い、設定でIOProcを明示選択した場合だけ使います。
IOProc経路は入力streamのない機器だけにHAL IOProcを登録し、入力取得やprocess tapは使いません。
非macOSまたはfeature無効のビルドでは候補を非表示にし、保存済みIOProc設定をAutoへ戻します。

macOSのIOHID入力は`macos-iohid` featureでのみコンパイルし、default featureと通常配布には含めません。
検証用に含める場合は`cargo build -p bmz-player --release --features macos-iohid`を先に実行し、
`scripts/package-macos-app.sh --skip-build`でその実行ファイルをパッケージします。
IOHIDを含めた場合も、権限要求は設定画面の明示操作だけで行います。
package scriptはMach時計の経過時間計算に対応する`assets/PrivacyInfo.xcprivacy`を
Resourcesへ含めます。最低対応macOS版は変更していません。
CLI/.app別の権限・署名と実機検証は[macOS遅延検証](macos-latency.md)を参照してください。
