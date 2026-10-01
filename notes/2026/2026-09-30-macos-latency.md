# macOS入力・音声診断の実装・検証

手順と仕様は [macOS遅延検証](../../docs/macos-latency.md)。2026-09-30に実施。
既存差分なしで開始。既定の入力Auto、Fixed 256を維持。

## 変更

1. `4885e371`: 任意有効化のcallback/入力キュー分布、JSONと比較script。
2. `cd9e3c19`: IOHID専用CFRunLoop、Mach変換、デバイス別保持、route世代。
3. `28d83f7f`: 設定・6言語UI・権限要求・fallback・privacy manifest。
4. `9d0f0412`: stream要求/採用情報、warm-up/epoch、音声コマンド部分計測。
5. 停滞試験、保存/IR抑止、画面表示、フォーカス確認、回帰テスト、検証文書。

gilrs-core 0.5.15のmacOS blocking APIは内部channelのrecv_timeout。
公開APIでroute変更・終了を待機解除できないため、コントローラーの1ms待機は維持した。
macOSでゲームパッド無効の場合だけ、不要な取得スレッドの待機期限を250msへ延ばす。
IOHIDは別スレッドなので影響しない。Linux/Windowsの待機仕様は変更していない。

## 実行環境・条件

- macOS 26.6.2 (25G83)、aarch64-apple-darwin、Rust 1.98.1。
- デバイス表示名「MacBook Proのスピーカー」、CoreAudio、採用48,000 Hz。
- 報告バッファ範囲15–4096 frames。
- debug build、同梱sample-playable、自動演奏、既定1280×720/Native/Vsync、FPS設定240。
- 一時データ `/tmp/bmz-latency-ab-20260930`。通常のprofile/DB/configは変更しない。
- サンプルは低負荷であり、高密度譜面/手動キー音の検証ではない。
- 最初の2秒を音声分布から除き、最後の累積JSONを採用。各設定1回、約20秒の起動試験。
- 初回系列は並行するbuild/checkの影響を排除していない。性能差を断定する条件ではない。

## 観測値

quantileはbucket上端（maxで制限）。callback処理時間はµs、API予測はms。
これは総入力遅延でも物理出力遅延でもない。

| 要求 | 実frames (p50/p95/p99/max) | 件数 | callback p95 / p99 / max (µs) | CPAL予測 (ms) |
|---|---|---:|---|---:|
| Auto | 512 / 512 / 512 / 512 | 1230 | 425.983 / 458.751 / 474.000 | 12.917 |
| Fixed 256（再測定） | 256 / 256 / 256 / 256 | 2454 | 81.919 / 90.111 / 102.625 | 7.583 |
| Fixed 128 | 128 / 128 / 128 / 128 | 4920 | 57.343 / 61.439 / 75.000 | 4.917 |
| Fixed 64 | 64 / 64 / 64 / 64 | 9841 | 36.863 / 45.055 / 61.333 | 3.583 |
| Fixed 256 + 100ms停滞 | 256 / 256 / 256 / 256 | 2456 | 73.727 / 90.111 / 172.500 | 7.583 |

上の各実行でstream error、engine lock miss、queue drop、timeline catch-up、invalid predictionは0。
callback間隔maxはAuto 10.724ms、256 5.384ms、128 2.732ms、64 1.383ms、停滞256 5.489ms。
OSのunderrun/processor overloadは直接監視しておらず、聴感検証も未実施。
短時間のエラー0を安定性保証や音切れなしの証明にはしない。

コマンド投入→適用p99はAuto 10.627ms、256 5.206ms、128 2.634ms、64 1.310ms、
停滞256 5.303ms。各28コマンドで、自動演奏/BGM等を含む。手動キー音遅延とは扱わない。

`/usr/bin/time -l` のプロセス全体（起動・ロードを含む）の記録:

| 条件 | real秒 | user秒 | sys秒 | (user+sys)/real |
|---|---:|---:|---:|---:|
| 256 診断OFF | 23.31 | 10.37 | 0.98 | 48.7% |
| 256 診断ON | 20.05 | 10.25 | 0.94 | 55.8% |
| 256 診断ON + 停滞 | 20.16 | 9.29 | 0.78 | 50.0% |
| 128 診断ON | 20.02 | 10.31 | 0.99 | 56.4% |
| 64 診断ON | 19.96 | 10.40 | 1.06 | 57.4% |
| Auto 診断ON | 20.12 | 10.14 | 0.91 | 54.9% |

実時間が揃っていないため、診断ONのCPU overheadをこの差から推定できない。
定常プレイの同じ測定窓で複数回再測定する必要がある。

## ネイティブ入力・配布形態

- CLIでMacOsHidを要求し、ゲームパッド無効で起動。権限確認後のmanager初期化が失敗し、
  winitへfallbackしてサンプル自動演奏・正常終了を確認。
  エラー値は `-536870203` (`0xe00002c5` / SDKの`kIOReturnExclusiveAccess`)。
  他の所有者やドライバによる排他状態の原因は特定しておらず、他プロセスの停止はしていない。
- ローカルdebug `.app` をpackage scriptで作成し、LaunchServices (`open -n -W`)で起動。
  こちらはInput Monitoring未許可を検出し、winitへfallbackして正常終了した。
- 許可要求ボタンを押さず、OS権限の付与・取消し・TCC編集は行っていない。
- IOHID実キー受信、A/B入力p95/p99、BMSコントローラー、JIS実機、スリープ、
  停滞中の実フォーカス/キー解放は未検証。自動演奏のinput_queue count=0は未測定。
- `.app` は `/tmp/bmz-latency-package-20260930/BMZ Player.app`。
  dylib同梱・配布署名・notarizationをしないローカル検証物で、Rustライセンスレポートは省略。
  正式配布物の権限同一性は未検証。
- Intel実機は未検証。C bridgeはx86_64 macOS 10.13指定の構文検査に成功。
  Apple Silicon bundleの11.0 minimum target検査は成功。

## 自動確認

- workspace check / all-targets Clippy (`-D warnings`) / test成功。
- 最初のsandbox内テストではローカルsocketのPermissionDeniedが17件。
  制限外で再実行して成功。機能の不具合とは区別した。
- 最終workspace実行でbmz-player全体2,073件成功、6件ignored。
  保存抑止の子プロセステストも成功し、手動結果のスコア・リプレイ・IR jobが残らないことを確認。
- bmz-audio 114件成功、live-device test 1件ignored。
- 入力関連60件、Mach/IOHID状態テスト6件成功。
- 既存の `dedicated_runtime_matches_original_session_advance` と
  `gameplay_results_replay_and_audio_are_invariant_to_render_stalls` が成功。
  同じ入力列・時刻で元のsession advanceと判定/リプレイが一致し、16/33/100/250msの
  render観測停止でも一致する。注入位置はSharedInputBackendで、IOHID取得性能の証明ではない。
- 設定round-trip、全localeのキー・placeholder一致、診断/判定時刻の分離を確認。
- package script構文、privacy manifestのplutil検査に成功。
- スリープ検出にはuptime/continuous両Mach時計の差を使い、時刻読み取り自体の長い
  descheduleを除外する。実スリープ操作は未実施。純粋なoffset変化の回帰テストは成功。
- 外部スキンがない場合に早期returnする既存テストを、スキン互換の実機確認とは数えていない。

生ログは `/tmp/bmz-latency-*.log`、`/tmp/bmz-workspace-final.log`、
`/tmp/bmz-player-complete.log`。一時ファイルなので将来の存在は保証しない。

## 次に行う操作

CLIの排他アクセスエラーは、キーボードを排他利用する常駐ソフト/ドライバの有無をユーザー操作で確認する。
設定からInput Monitoring許可を明示要求し、CLI/.appそれぞれで再起動・再選択を試す。
まず音声256固定・自動キー音OFFでwinit/IOHIDの実キーA/Bを行い、その後に音声バッファだけ変更する。
通常/高密度譜面を各条件複数回・10分以上試し、専用機材があれば同一収録器でトリガーと
アナログ出力を記録する。**物理総遅延、対象手動音別C/D、長時間安定性は未測定。**
