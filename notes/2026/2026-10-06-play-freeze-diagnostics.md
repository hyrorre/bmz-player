# プレイ中の画面停止調査とGNOME / Plasma比較

2026-10-06の調査記録。短い停止はGNOMEメインスレッドのCPU処理と一致し、
Plasmaへの移行後は長めの停止が減った。ただし通常のばらつきは残り、
当初申告された約5秒停止と、GJSのGCが直接の原因かどうかは未確定。
調査用に追加したWARN診断は、後述の比較完了後に撤去した。

## 症状と調査

対象は `feat/linux-latency`、調査開始時のHEADは `a6ff7902`。
ユーザー申告では、プレイ中に画面が約5秒止まる一方、曲とキー音は続く。
発生時期・条件は不明。

Linux / Wayland / GNOME / NVIDIA RTX 5090 / Vulkan / Fifo、antique Play skinで確認中。
12:25 JST前後の再発報告に対応する起動は通常の `filter=info` で、
DEBUG限定の描画計測値が残っていなかった。先に診断用環境変数で起動した2回は
Selectのみで、プレイ中の再発を記録したものではない。

別の45秒間のmain thread待機状態サンプリングでは5秒停止を捉えられなかった。
その期間のrunqueue待機の少なさは、申告された停止の原因を特定する証拠にはならない。
プロセスへのstrace attachはOSのptrace制限で失敗した。制限の変更は行っていない。
生ログは `.local/performance/play-freeze-2026-10-06/` と `data/logs/` にあり、共有しない。

## 初期対応で追加した診断（後に撤去）

`f0b26e44` で、環境変数を設定し忘れても再発を調べられるよう、250ms以上の長い描画処理と
描画間の空白をWARNへ記録する診断を追加した。既存のphase計測を再利用し、通常フレームの
詳細ログやDEBUG profilerの集計は有効化しなかった。
現在も利用する既存の計測手順は [frame-pacing.md](../../docs/frame-pacing.md) を参照。

この変更は原因特定のための診断追加であり、画面停止の修正・解消確認ではない。
実機で再発した際のログから、event loop間の空白と描画処理内の停滞を切り分ける。

## 初期診断追加時の検証

- `cargo fmt --check`、`cargo check -p bmz-player --locked`、
  `cargo clippy -p bmz-player --all-targets --locked -- -D warnings` は成功。
- `cargo test -p bmz-player --locked --no-fail-fast` は2219成功・6失敗・19 ignored。
  失敗6件はいずれもLuxez-Flat submoduleが未初期化で素材が存在しないため。
  スキン互換全体の検証完了を示す結果ではない。
- 追加した診断テスト6件は成功。5秒のgapをINFO filterでWARNへ出力すること、
  前フレームの長い処理との区別、低FPS、scene/generation変更、描画を挟まないfocus変更、
  pacing変更時のリセットを実時間のsleepなしで検証した。
- `cargo build -p bmz-player --release --locked` は成功。
- 分離したdata/cache/logs領域、同梱default skinとサンプル曲、Wayland / Vulkan / Fifo /
  PulseAudioで360 Playフレームのsmokeが正常終了した。`RUST_LOG=info`、ERRORなし。
  window decorationの既存WARNは発生した。これは起動確認であり、停止の解消を示さない。

実際の5秒停止は未再現で、解消は未確認。

## 追加調査：長い停止と短い引っかかりの切り分け

ユーザーはVsync / Unlimited / 安定で改善したが、画面全体の引っかかりが残ると報告した。
mainでも同様との申告があり、BMZの処理時間とcompositorからの表示通知を分けて調べた。

- 実ログの0.918秒redrawはE2+E3でプレイを中断した際の`consume_active_play`に集中していた。
  通常演奏中の短い停止や、当初の約5秒停止と同一の事象とは判断しない。
- GNOME上のBMZでは、実表示間隔の最大20.807msを取得した。
  停止直前にsurfaceはcommit済みで、frame callback受信が遅れていた。
  該当120フレームの集計ではCPU redraw最大2.741ms、surface取得最大0.197msだった。
- BMZを含まないwgpu 29.0.3 / winit 0.30.13の背景色clearでも実表示間隔最大24.540ms、
  GTK 4.22.4 / OpenGLの背景色描画でも最大21.467msを記録した。
  音声・スキン・譜面・BGA・wgpu・winit・Vulkanに固有の問題ではない。
- 計測曲のBGAは静止画であり、動画decode負荷は再現に不要だった。

BMZとwgpuの上記表示時刻はWayland詳細ログから取得した。
GTKは詳細ログを使わず、`Gdk.FrameTimings.get_presentation_time()`のcomplete済みの値を
8フレーム後に読み、メモリへ蓄積して終了時にCSVへ保存した。未取得値はゼロ遅延として扱わない。

## GNOMEのモニター構成とメインスレッド

環境はNVIDIA RTX 5090 / driver 610.57.04、GNOME Shell 50.1、Mutter 50.1、GJS 1.88.0、
native Wayland。DP-2はLG ULTRAGEAR+ 3840×2160 / 240.084Hz、
DP-1はDELL G3223Q 3840×2160 / 143.963Hz。両方とも倍率125%、固定リフレッシュレート。

GTKプローブをDP-2の全画面で各40秒実行し、起動後2〜38秒の実表示間隔を集計した。

| 構成・実行順 | p99 (ms) | 最大 (ms) | 12ms超 |
|---|---:|---:|---:|
| 2画面・変更前 | 5.359 | 13.513 | 2回 |
| LGのみ | 4.288 | 22.414 | 1回 |
| 2画面・復元後 | 5.577 | 21.359 | 3回 |

LG単独では通常のばらつきが小さくなったが、長い停止は残った。
各構成1回の短い比較なので、停止頻度の優劣は断定しない。
一時的なモニター変更は独立した復元用プロセスとfinallyで保護し、元の設定への復元を確認した。

SysprofのMutter marksと、約1ms周期で読み取ったGNOME main threadの
`/proc/<pid>/task/<tid>/schedstat` / `stat`をCLOCK_MONOTONICで照合した。
GTK callbackが18.076ms空いた事象を囲む18.954msの観測区間で、
GNOME main threadのCPU実行は16.632ms、runqueue待機は0.025msだった。
ユーザー空間の実行時間が増え、両モニターのframe dispatchが同時に遅れていた。
その間、観測側は最大約1.14ms間隔で動作していた。
別の停止でも14.014ms / 10.888msのCPU実行を確認した。

この結果はGNOME main threadのCPU処理が画面更新を遅らせていることを支持する。
記録されたMutterの描画処理だけではCPU時間の大半を説明できなかった。
[GJS 1.88.0のschedule_gc_internal](https://raw.githubusercontent.com/GNOME/gjs/1.88.0/gjs/context.cpp)
には10秒後にGCを予約する実装があり、観測した約10秒周期と合うため候補となる。
ただし今回のSysprof記録にはGJSのGC marksや実行スタックがなく、GC自体は未確認。
NVIDIA driverの全関与を除外する結果でもない。

### 中止・除外した比較

- GNOME拡張の全停止比較は、Web検索拡張の再有効化時に`GLib.Error.matches`の引数エラーが
  出たため中止した。復旧の再読み込みでTiling Assistantのindicator重複エラーも生じた。
  保存設定と他の6拡張は復元したが、そのセッションのTiling AssistantはERRORのままで、
  ユーザーへ再ログインが必要と通知した。エージェントによるログアウトは行っていない。
  後にユーザーがPlasmaへ移行したが、GNOMEでの拡張復旧の再検証はしていない。
  この失敗をフレーム停止の原因や「拡張を停止すると改善する」という証拠には使わない。
- XWaylandの試行はmonitor / surfaceサイズがnative Waylandと異なり、厳密な速度比較から除外。
- Mesa overlay layer併用は起動時SIGSEGV、wgpu OpenGLプローブはcompatible adapterなしで、
  性能結果に含めていない。

## Plasma移行後の比較

ユーザーがUbuntuへPlasmaを追加し、体感が改善したとの報告後に再計測した。
Plasma / KWin 6.6.6、Ubuntu 26.04.1、kernel 7.0.0-38-generic、native Wayland。
GPU・driver・両モニターの解像度とHzは同じで、VRRはNeverだった。

BMZはGNOME時と同じ`f0b26e44`のrelease binaryを利用し、SHA256も一致した。
Rechronize another、antique skin、DP-2の4K borderless、Vulkan / Fifo、Unlimited、
安定（maximum frame latency 2）、同じ音声設定で自動演奏した。
専用data/cache/logsを使い、config/profileはGNOME試行のものをコピーした。
保存時刻を除く設定値の一致を確認している。

ただしGNOMEは倍率125%、Plasmaは150%で、別のログインセッションである。
surface formatもGNOMEのRgba8UnormからPlasmaのBgra8Unormへ自動選択が変わった。
これらを含む環境比較であり、compositorだけを変えた厳密な比較ではない。

### BMZ：CPUの描画開始間隔

gameplay runtime開始後2〜18秒のfocusedな16秒間を比較した。
両環境とも`play_profile=debug` / `frame_pacing=trace`で、Wayland詳細ログは無効。

| 環境 | 件数 | 平均 (ms) | p99 (ms) | 最大 (ms) | 12ms超 |
|---|---:|---:|---:|---:|---:|
| GNOME | 3,822 | 4.186 | 4.838 | 19.599 | 2回 |
| Plasma 1回目 | 3,834 | 4.172 | 5.343 | 8.004 | 0回 |
| Plasma 2回目 | 3,837 | 4.170 | 5.257 | 6.237 | 0回 |

CPU redraw処理の平均はGNOME約1.253ms、Plasma約1.463 / 1.470ms。
surface取得は約0.066msから約0.080 / 0.082ms、present呼び出しは約0.117msから
約0.118 / 0.117msであり、CPU描画処理が高速化したという結果ではない。
これらはCPU側のphase計測で、GPU execution時間でも実表示間隔でもない。

PlasmaのWayland詳細ログありの別試行では、frame callback受信間隔最大7.975msだった。
この環境のBMZログ経路には`wp_presentation_feedback.presented`が出ず、BMZの実表示時刻は
取得できなかった。callback / CPU間隔を実表示間隔の代用として扱わない。

最初のPlasma試行は`--monitor primary`がDP-1を選択したため除外し、以降は
`monitors list`に出るDP-2の完全なIDを指定した。
Waylandログのoutput名と`set_fullscreen`の対応でもDP-2を確認した。

### GTK：compositorから報告された実表示間隔

同じGTK / GLRendererプローブで各40秒実行し、2〜38秒の36秒間を比較した。
GNOMEは前節の2画面の2回と拡張比較前の1回、Plasmaは2回を集計した。

| 環境・試行 | 件数 | p99 (ms) | 最大 (ms) | 12ms超 |
|---|---:|---:|---:|---:|
| GNOME・2画面変更前 | 8,636 | 5.359 | 13.513 | 2回 |
| GNOME・2画面復元後 | 8,631 | 5.577 | 21.359 | 3回 |
| GNOME・拡張比較前 | 8,624 | 5.504 | 24.686 | 5回 |
| Plasma 1回目 | 8,617 | 5.462 | 8.751 | 0回 |
| Plasma 2回目 | 8,615 | 5.408 | 9.460 | 0回 |

未取得値と非連続frame counter間の差は分位点から除外した。
Plasma 2回目は未取得1行で、その前後の有効値の差は2フレーム分で8.148msだった。
GNOMEの拡張比較前にも未取得1行がある。

### 判断と計測の限界

- Plasmaでは長めの停止が減ったことを確認でき、ユーザーの体感報告と整合する。
- BMZのp99は少し悪化し、GTKのp99も同程度である。通常のばらつきやフレーム落ちの
  完全解消、長時間の安定動作を証明する結果ではない。
- 約5秒停止の再現・解消、GJS GCの直接計測は未完了。
- 表示時刻はcompositor / GTK feedbackによるもの。光学センサー計測や、
  物理入力から表示・発音までの遅延測定は行っていない。
- Plasma計測ではモニター設定を変更せず、終了後も設定の一致を確認した。
  起動した計測用BMZは全て正常終了し、普段のconfig / DBは利用していない。

## 診断用変更の撤去（2026-10-06）

ユーザーの依頼に従い、今回の画面停止調査で`f0b26e44`に追加した部分を戻した。

- `frame_runtime.rs`の`RedrawStallTracker`、250ms閾値、専用テスト6件。
- `lifecycle.rs`の描画開始・終了追跡、focus時の追跡リセット、`slow play redraw` WARN。
- `frame_flow/render.rs`の追加時計読み取りと`slow play scene render` WARN。
- 診断のためだけに広げた`current_frame_pacing_state`の可視性と、現行仕様書のWARN説明。

既存のframe profilerや、今回の調査以前からあるLinux遅延診断は維持する。
Wayland FIFO待機、focus通知、入力overflow、build identityの修正も維持する。
撤去した4つのRustファイルは、診断追加前の`a6ff7902`との一致を確認した。
これは診断処理の撤去であり、今回のPlasma比較結果を撤去後の性能測定とは扱わない。

撤去後の検証結果:

- `cargo fmt --check`、`cargo check -p bmz-player --locked`、
  `cargo clippy -p bmz-player --all-targets --locked -- -D warnings` は成功。
- `cargo test -p bmz-player --locked --no-fail-fast` は2213成功・6失敗・19 ignored。
  失敗集合は初期診断追加時のログと完全に一致し、Luxez-Flat素材未配置による6件だった。
  成功数の6件減少は、撤去した診断専用テスト6件によるもの。
- `cargo build -p bmz-player --release --locked` は成功し、実行ファイルも診断撤去版へ更新した。
- Plasma上で、分離データ・同梱default skin・サンプル曲・Vulkan / Fifoを使う
  360 Playフレームの自動演奏smokeが正常終了した。WARN / ERRORはなく、ログdropは0。
  この起動確認を性能改善の証拠には使わない。
- `git diff --check`、ローカル文書リンク、Rustソースの診断追加前との差分なしを確認した。
  Windows / macOS、追加feature、workspace全体の再検証は行っていない。

## ローカル計測素材

以下は`.local/performance/play-freeze-2026-10-06/`以下に保存したローカル専用データで、
Git管理・共有・同梱はしない。第三者製スキン・曲・profile / DBも同梱しない。

- `investigation-result.json`: GNOME上の切り分け、モニター比較、拡張比較の中止・復旧状態。
- `monitor-comparison/`: GTK CSV、Mutter Sysprof、GNOME main threadのCPU照合結果。
- `plasma-comparison/comparison.json`: Plasma比較の集計値、条件差、未取得値、除外試行。
- `cadence-feature-current-settings-dp2/`、`cadence-plasma-dp2-cpu-{1,2}/`: BMZ CPU比較。
- `cadence-wayland-feedback-dp2/`、`cadence-plasma-dp2-feedback-1/`: Wayland詳細ログ。
- `minimal_gtk.py`、`measure_plasma.py`、`summarize_gtk.py`、`summarize_plasma.py`: 計測・集計用スクリプト。
- `cleanup/`: 診断撤去後のtest / release build / smokeのログ。

関連する実装・計測契約は [frame-pacing.md](../../docs/frame-pacing.md)、
[gameplay-runtime.md](../../docs/gameplay-runtime.md)、[linux-latency.md](../../docs/linux-latency.md)、
[Linux入力経路の比較記録](2026-10-06-linux-input-alternatives.md)を参照。
