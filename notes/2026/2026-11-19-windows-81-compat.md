# Windows 8.1向けの互換ビルドとフォールバック検証

## 対象と判断

- 開始時のHEAD: `9455b88bbd9d169cf600e58bfc89b11182d0d9e0`（main）。
- 作業ブランチ: `codex/windows-8.1-compat`。
- Windows 8.1 VMは準備中。正式サポートの可否は今回の結果だけでは確定しない。
- ビルドと再検証の手順: [Windows互換ビルド](../../docs/windows-compat.md)。
- 関連仕様: [入力の実行経路](../../docs/gameplay-runtime.md)、
  [設定](../../docs/settings.md)、[フレーム計測](../../docs/frame-pacing.md)。

## 変更

- WindowsでVulkan指定の初期化に失敗したらOpenGLを試す。
  AutoのDirectX 12 → Vulkan → OpenGLの順序は維持する。
- WGIのRawGameController APIを実際に呼び、利用可否をプロセス単位で保持する。
  未接続とAPIなしを区別し、Auto・保存済み設定の補正・UIの候補・再初期化で同じ判断を使う。
  WGIが使えない環境ではAutoと保存済みgilrsをRaw Inputへ解決する。
- Windowsでgilrs初期化に失敗した場合もRaw Inputを試す。
  専用取得スレッドのHID登録は、要求値ではなく生成したバックエンドに合わせる。
  Raw Inputに必要なbridgeがなければ、暗黙にgilrsを開かない。
- 入力設定に実際の「動作中」バックエンドを表示し、6言語のカタログを更新する。
  ログにも要求値と実効値を記録する。
- `scripts/build-windows-compat.ps1`でplayer/updaterのwin7ビルドと比較用の通常ビルドを追加する。
  同じnightlyで両ターゲットのstdを再ビルドする。
  vcpkg-rs 0.2.15のターゲット名推論の制限は`VCPKGRS_TRIPLET`の明示で回避する。
- WASAPIのIAudioClient3失敗時の通常共有出力へのフォールバックは、既存コードを確認した。
  `crates/bmz-audio/src/backend/cpal/device.rs`の`WasapiSharedPeriodGuard::open`失敗時に
  `None`を返して通常共有出力へ戻るため、変更しない。

## 環境

- Windows 11 Pro 10.0.26300 / Intel Core i9-12900K。
- NVIDIA GeForce RTX 5090、ドライバ621.14、OpenGL 4.6。
- 通常の検証: stable rustc 1.99.1 (`58cf8f9c3`, 2026-10-13)。
- 比較ビルド: rustc 1.102.0-nightly (`65ce8611f`, 2026-11-18)、nightly配布日2026-11-19。
- target: `x86_64-pc-windows-msvc` / `x86_64-win7-windows-msvc`。
- 依存DLLはローカルvcpkgの`installed/x64-windows/bin`を共通で使用する。

## ビルド・動作確認

- 両ターゲットのrelease player/updaterビルドに成功。
- win7版の`--version` / `--help`がWindows 11で動作。
- win7版でOpenGLを明示し、同梱サンプルのAutoplayを120 Play framesで正常終了。
- VulkanのICDパスを当該プロセス内で存在しないファイルへ向け、Vulkan surface作成失敗 →
  OpenGL選択 → 120 Play frames → 正常終了をログで確認。
- 上記のフォールバック試行では入力をRaw Inputに明示し、
  `backend="Raw Input"`を確認。接続中のHID機器を16ボタン・2軸として認識した。
  ボタン押下・軸の実操作は実施していない。
- `dumpbin /imports`でwin7版exeと主要FFmpeg DLL 4本の一部の新API直接importを確認。
  `SetThreadDescription`、`GetDpiForWindow`、`GetTempPath2`、`IsWow64Process2`は見つからない。
  exeのOS/subsystem versionは6.00。これは全依存DLLの8.1互換性の証明ではない。
- nightlyではwgpu型に関する`recursion_depth_exceeding_limit`警告が両ターゲットで出る。
  ビルドには成功するが、このnightlyを将来更新する際は再確認が必要。

## 自動検証

制限環境での初回workspace testはローカル通信・パス参照で失敗し、通信待ちのテストを中断した。
再実行では専用TEMP/TMPを`.local/windows-compat/test-temp`へ設定し、必要なローカル通信を許可する。

- `cargo fmt --all --check`: 成功。
- `cargo check --workspace --locked`: 成功。
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: 成功。
- `cargo test --workspace --locked --no-fail-fast`: 再実行は終了コード0。
  全出力の集計で5,806 passed / 0 failed / 144 ignored。
  player本体は3,342 passed / 94 ignored、renderは1,071 passed / 29 ignored。
  除外された実機・手動テストは成功数に含めない。
- WGIあり/なしの選択、保存済み設定の補正、Raw Inputのbridgeなし、
  Vulkan失敗とDX12無効時のAuto経路からのOpenGL再試行を含む。
- PowerShellビルドスクリプトの構文確認と両ターゲットでの実行に成功。

通常check/Clippyでは、制限環境のincremental cacheのhard link作成に関する環境警告が出る。
RustコードのClippy警告を抑制して通したものではない。

## ターゲット比較

Mutexの実装は、Rustのstdソースで通常ターゲットのfutexとwin7のSRWLOCKを確認した。
同じソース・nightly・DLL・OpenGL・Raw Inputで比較し、CPU処理とsurface/present待ちを分ける。
Windows 11上での比較であり、8.1の性能を推測する根拠にはしない。

2026-11-19の18:38〜18:39 JSTに、ビルド・テスト終了後に実行した。
同梱サンプル、標準スキン、1280×720、VsyncOff（実効Immediate）、frame latency=1、
240 FPS上限、背景上限も240、Raw Input、WASAPI通常共有を固定した。
音声はRME Babyface Pro、48 kHz、実効480 frames。
診断は`RUST_LOG=info,bmz_player::play_profile=debug`と`BMZ_LATENCY_DIAGNOSTICS=1`で共通。
個人データをコピーせず、各試行に同じ生成済みconfig/profileを配置した。

実行順はpc1 → win7-1 → win7-2 → pc2 → pc3 → win7-3。
各2,400 Play framesで終了し、最初の240 framesを除いた2,160 framesを集計した。
FPSは120 framesごとのログ間隔から算出したもので、モニターの実表示回数の測定ではない。

| ターゲット | 試行 | ログ間隔のFPS | plan平均 ms | 再描画平均 ms | 区間p99の中央値 ms | surface平均 ms | present平均 ms |
|---|---:|---:|---:|---:|---:|---:|---:|
| pc | 1 | 240.00 | 0.0735 | 1.1566 | 1.6980 | 0.0032 | 0.3480 |
| pc | 2 | 240.00 | 0.0737 | 1.1648 | 1.8115 | 0.0032 | 0.3680 |
| pc | 3 | 239.99 | 0.0732 | 1.1344 | 1.7395 | 0.0033 | 0.3557 |
| win7 | 1 | 240.01 | 0.0736 | 1.1668 | 1.7325 | 0.0033 | 0.3657 |
| win7 | 2 | 240.00 | 0.0748 | 1.1552 | 1.6885 | 0.0033 | 0.3659 |
| win7 | 3 | 240.01 | 0.0734 | 1.1477 | 1.7200 | 0.0035 | 0.3536 |

planはCPUのdraw-plan作成、再描画は`total_redraw_ms`で待ち時間を含む。
区間p99は120 framesごとのp99の中央値で、全試行を統合したp99ではない。
全再描画の区間p99最大値はpcで5.177 / 2.723 / 3.154 ms、win7で4.156 / 2.255 / 3.580 ms。

3試行平均の再描画時間はpc 1.1519 ms、win7 1.1566 ms（約+0.0047 ms / +0.4%）。
CPUのplanは0.0735 / 0.0739 ms、renderer外のapp処理は0.0675 / 0.0705 ms。
surfaceは0.0032 / 0.0034 ms、presentは0.3572 / 0.3617 ms、queueは0.2765 / 0.2786 ms。
この短いサンプル条件では、大きな悪化や240 FPS維持の差は確認できなかった。
Mutexだけの差を測定したものではなく、実操作・高密度譜面・動画BGA・8.1では別途比較が必要。

比較exeのSHA-256（dirty build、Git管理外）:

- pc: `85653FFD7A5D817801276FB219CA1E626BFAF16628929260EE18CC09CCEE778A`
- win7: `DECDBF7855DF53711175300EE27422DEC304BE5D36352A978C20EE0A7C188307`

## 未確認

- Windows 8.1 VMでのexe/全DLLロード、描画、音声、BGA、専コン実操作。
- WGIが実際に存在しないOSでのAuto選択・設定候補・保存済みgilrs設定の移行。
  今回の自動テストは利用可否を模擬した選択・補正・再生成経路を確認する。
- 専コンの押下、スクラッチ両方向・停止・抜き差し、バックエンド変更後の実入力。
- Windows 8.1のIAudioClient3なしでの音声再生とフォールバック。
- 物理Windows 8.1、macOS、Linuxでの動作。

生ログと試行データは`.local/windows-compat*`へ保存する。これらはローカルのみでGitへ含めない。
