# Windows 8.1向けの検証

Windowsリリースのx64バイナリはWindows 8.1互換を目標にビルドします。
Windows 8.1自体は検証中です。ビルドや新しいWindowsでの起動成功だけでは正式サポートとしません。
VMでの起動、GPUドライバ、音声、USBパススルーした実際の専コンを確認してから判断します。
作業結果は[2026-11-19の記録](../notes/2026/2026-11-19-windows-81-compat.md)へ残します。

## ビルド

64bit WindowsのMSVC / Windows SDK / FFmpeg / ASIOの準備は[README](../README.md)と共通です。
`x86_64-win7-windows-msvc`はWindows 7以降向けのTier 3ターゲットです。
標準ライブラリが配布されないため、nightlyの`build-std`でビルドします。
通常リリースもこのRustターゲットを使います。自動更新の配布先IDは`windows-x64`を維持します。

```powershell
rustup toolchain install nightly --profile minimal --component rust-src
.\scripts\build-windows-compat.ps1
.\scripts\build-windows-compat.ps1 -Target x86_64-pc-windows-msvc
```

スクリプトはplayerとupdaterを同じターゲットでビルドします。
updaterはCRTを静的リンクし、`updater/`だけから起動してもVC/UCRTの別途インストールを必要としない構成にします。
出力先は`target/<target>/release/`です。`-Profile Debug`も指定できます。
vcpkg-rs 0.2.15がwin7ターゲットからMSVC ABIを判別できないため、
未指定時は`VCPKGRS_TRIPLET=x64-windows`をコマンド実行中だけ設定します。
独自のFFmpegは`FFMPEG_DIR`、独自tripletは`VCPKGRS_TRIPLET`で指定します。
Rustターゲットを変更してもFFmpegやVCランタイムなどのDLLの最低OSは変わりません。
VMへ持ち込むDLLも8.1でロードできる必要があります。

## リリースとCI

以下の互換パッケージ作成とCI検証スクリプトはPowerShell 7で実行します。
`.github/workflows/release-apps.yml`のWindows jobは`windows-2022`上で、
`nightly-2026-11-19`と`rust-src`を使ってwin7ターゲットをビルドします。
MSVCはv143、Inno Setupは6.7.1に固定し、installerの最低OSは6.3（Windows 8.1）です。
ASIO SDKは公式の2.3.4（2025-10-15）をURLとSHA-256で固定し、
CIのPowerShellで展開して`CPAL_ASIO_DIR`を渡します。
`asio-sys`の自動取得時にWindows PowerShellとPowerShell 7のArchive moduleが混在する問題を避けます。
`scripts/package-windows.ps1 -Target x86_64-win7-windows-msvc -SkipBuild`は
VS 2022の`vcruntime140.dll`とWindows SDKのapp-local UCRT一式を追加します。
UCRTは`Windows Kits/10/Redist/ucrt/DLLs/x64`の旧OS向けセットを使い、
build番号が14393を超えるものやVC runtime 14.44を超えるものは包装を失敗させます。
使用したDLLのversionとSHA-256を`resources/licenses/windows-runtime.json`へ記録します。

CIの確認は次の2段階です。

- `scripts/verify-windows-compat-imports.ps1`: 同梱DLLのimport/export、updaterのCRT非依存、
  既知のWindows 10以降の直接importを検査します。すべてのOS APIを網羅するものではありません。
- `scripts/test-windows-compat.ps1`: 配布物の一時コピーを作り、開発用DLLのPATHと保存先overrideを外して、
  player/updaterのCLI、AutoとRaw Inputでそれぞれ120 Result framesの描画・正常終了を確認します。
  音声デバイスのないホストでも検証できるよう、サンプルのResult画面を使います。
  配布用stagingへ実行時dataを作らず、診断ログを`windows-compat-startup` artifactへ残します。

GitHub提供のWindowsランナーには8.1がないため、起動試験はWindows Server 2022上で行います。
同ランナーのsoftware adapterも利用できますが、確認できるGPU/backendはその試行のログを基準にします。
8.1のAPI、同梱UCRTの実際のロード、専コン、プレイ・音声の実機確認を代替しません。

Release Appsの手動実行では`windows_only=true`、`tag`にブランチ等のGit refを指定すると、
Windowsだけをビルド・起動確認できます。このモードでは`upload_to_release`の値に関係なく公開せず、
通常のrelease asset名でActions artifactだけを生成します。

## 実行経路

- 描画のAutoはDirectX 12 → Vulkan → OpenGLの順です。
  WindowsでVulkanを明示した場合も、初期化に失敗したらOpenGLを試します。
  ログの`selected renderer adapter for backend candidate`と
  `window and renderer surface ready`で実効バックエンドを確認できます。
- 入力のAutoはWGIのRawGameController APIを使える場合にgilrsを選びます。
  利用できない環境ではRaw Inputを選び、保存済みgilrs設定もRaw Inputへ移行します。
  WGIの利用可否はAPIを実際に呼び出して判定し、接続台数には依存しません。
  このWGIは実験用のGameInput.dllとは別です。
- gilrsの初期化に失敗した場合も、WindowsではRaw Inputを試します。
  HID登録は実際に生成できたバックエンドに合わせます。
  入力設定の「動作中」とログの`input backend: dedicated capture thread`を確認します。
- WASAPI低遅延共有はIAudioClient3を使えない場合に通常の共有出力へ戻ります。
  8.1での実際のデバイス再生は別途確認します。

## 通常ターゲットとの比較

同じnightly、ソース、release設定、DLLで両ターゲットをビルドしてください。
`-Toolchain`で日付付きnightlyを指定することもできます。
このスクリプトは比較側の通常ターゲットも標準ライブラリからビルドし、
コンパイラの違いとターゲットの違いを混ぜないようにします。

Rust標準ライブラリのMutexは通常ターゲットではfutex、win7ではSRWLOCKを使います。
Mutexの単体計測だけではアプリのフレーム時間への影響を判断できません。
両方を同じWindows環境で交互に実行し、次の条件を固定して比較します。

- 譜面・スキン・解像度・GPU・ドライバ・描画バックエンド
- Vsync・FPS上限・frame latency・音声デバイス・バッファ・入力バックエンド
- ログ設定・ウォームアップ・計測フレーム数。ビルドやテストは計測中に動かさない

個人の設定やスコアに影響しないよう、`BMZ_DATA_DIR`を検証専用ディレクトリへ設定し、
`BMZ_RESOURCE_DIR`にはリポジトリの`data`を指定します。
同じ初期設定から始めるときも、各試行のデータディレクトリは分けてください。

```powershell
$env:BMZ_RESOURCE_DIR = (Resolve-Path .\data).Path
$env:BMZ_DATA_DIR = Join-Path (Get-Location) '.local\windows-compat\pc-run1'
$env:RUST_LOG = 'info,bmz_player::play_profile=debug'
$env:BMZ_LATENCY_DIAGNOSTICS = '1'
& .\target\x86_64-pc-windows-msvc\release\bmz-player.exe --boot-play-sample --autoplay-on-start --smoke-exit-after-play-frames 1800
```

win7版では実行ファイルと`BMZ_DATA_DIR`を変更します。
フレームログは検証データの`logs`へ保存されます。
`play frame profile`のCPU処理、`total_redraw_ms` / p95 / p99、
`surface_ms` / `present_ms`とそのp95 / p99を分けて比較します。
表示FPSも確認し、起動成功だけを性能改善の証拠にしません。
計測項目の意味は[フレームペーシング](frame-pacing.md)を参照してください。

## Windows 8.1 VMでの確認

1. 実行ファイルと全DLLを配置し、CLIの`--help`、通常起動を確認する。
2. OpenGLを明示して起動する。Vulkanを使えない状態でVulkan指定からOpenGLへ移ることも確認する。
3. Raw Inputを明示し、専コンの全ボタン・スクラッチの両方向・停止・抜き差しを確認する。
4. Autoと保存済みgilrs設定で起動し、「動作中」がRaw Input、gilrsが候補にないことを確認する。
5. バックエンド変更・無効化・再有効化でHID登録と実際の取得が一致することを確認する。
6. WASAPI低遅延共有から通常共有へのフォールバックと実際の再生を確認する。
7. サンプル・BGA付き譜面をプレイし、同じVMでターゲット比較を行う。

VMの仮想GPU・音声とUSBパススルーの結果は、物理Windows 8.1環境の結果と区別します。

## 参照

- [Rustのwin7ターゲット](https://doc.rust-lang.org/rustc/platform-support/win7-windows-msvc.html)
- [RawGameControllerの最低OS](https://learn.microsoft.com/en-us/uwp/api/windows.gaming.input.rawgamecontroller)
- [gilrs-core 0.5.15の既定feature](https://docs.rs/crate/gilrs-core/0.5.15/source/Cargo.toml)
