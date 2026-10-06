# プレイ中の画面停止調査と通常ログでの診断

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

## 追加した診断

環境変数を設定し忘れても次の再発を調べられるよう、250ms以上の長い描画処理と
描画間の空白をWARNへ記録する。既存のphase計測を再利用し、通常フレームの詳細ログや
DEBUG profilerの集計は有効化しない。仕様は [frame-pacing.md](../../docs/frame-pacing.md) を参照。

この変更は原因特定のための診断追加であり、画面停止の修正・解消確認ではない。
実機で再発した際のログから、event loop間の空白と描画処理内の停滞を切り分ける。

## 検証

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
