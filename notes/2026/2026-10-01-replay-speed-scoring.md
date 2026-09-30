# リプレイ視聴速度による採点差の修正

- 作業日: 2026-10-01
- Issue: https://github.com/hyrorre/bmz-player/issues/24
- 調査開始時のHEAD: `a4c85e0f`、作業開始時の差分なし。
- 実装・回帰テストcommit: `732db42c`。
- 現在の仕様: [rule.md](../../docs/rule.md)

## 原因と変更

`session/frame.rs::sync_judge_windows` は、記録済み入力を譜面時間のまま
判定するリプレイにも、実入力用の速度倍率を掛けていた。低速では判定窓が
狭く、高速では広くなる。独立した対戦相手のリプレイにも同じ補正があった。

本人・対戦相手のリプレイではこの補正を外し、Practiceの実入力には残した。
判定窓だけを固定してフレーム単位の進行を残す実験では、200%再生のLNを
含む比較テストが失敗した。見逃し判定の発生時刻とHCNの状態変更・継続ゲージも
外側のフレーム間隔に依存するため、リプレイ専用の譜面時間1ms間隔で進める。
記録入力の時刻は変更せず、判定にそのまま渡す。ゲージによる途中終了も
この採点ステップ内で判定し、終了後の入力を採点しない。

`ReplayPlayer.next_scoring_time` は実行中だけのcursor。ファイルに保存される
`ReplayFile` / `ReplayEvent`、DB schema、既存スコアは変更しない。
skip/seekではcursorを開始時刻に移し、過去の採点を繰り返さない。
本人の通常リプレイと独立対戦相手が対象で、現在のappでは
`replay_lane_mask` は常に `None`。Practiceと非リプレイの進行経路は維持する。

等速再生も共通の採点ステップを使用する。従来のフレーム依存のHCN更新や
見逃し判定の発生時刻まで再現するものではなく、修正後の等速結果を基準に
各速度の一致を保証する。保存済みスコアの書き換え・再集計は行わない。

IssueのnumberRef正差分の「＋」表示は変更していない。
Issue本文の取得はこの環境では失敗したため、ユーザー提示の調査・条件を基に作業した。

## 自動検証

- `cargo fmt --check`: 成功。
- `cargo check --workspace --locked`: 成功。
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: 成功。
- `cargo test -p bmz-gameplay --locked`: 232件成功。
- `cargo test -p bmz-player --locked gameplay_runtime::tests`: 6件成功。
- `cargo test --workspace --locked --no-fail-fast`: bmz-playerの下記2件が失敗。
  他の対象は成功した。bmz-playerの6件のignoredテストや、外部素材なしで早期returnする
  テストを実機・外部スキン互換性の確認には数えていない。

追加回帰テストは同じ記録入力を25/50/100/200/300%および200msごとの
途中速度変更で比較する。144 FPS相当の実時間間隔に速度倍率を掛けて
譜面時間のフレーム間隔も変え、共通の譜面時刻で採点状態を比較した。
通常ノーツのPG/GREAT/BAD境界付近、見逃し、LN/CN/HCNの早離し・押し直し、
終端の±16ms境界とその前後を含む。本人のノーツ別判定詳細・全判定列、
本人/対戦相手の判定集計、EX、BP、現在/最大コンボ、ゲージが等速と一致する。
表示オフセット0/-20msも比較。Practice実入力の15ms差に対する判定が
速度によって意図どおり変わること、採点cursorの重複処理防止とseek、Hardゲージでの
途中終了後に採点を継続しないことも確認した。

失敗した変更範囲外のテスト:

- `skin_loader::tests::cache::result_refresh_pins_resolved_wildcard_source`:
  decode後のsourcesが空で、index 0参照に失敗。単独実行でも再現した。
- `skin_loader::tests::paths::wildcard_source_with_context_falls_back_to_default_file_stem`:
  通常Windowsパスと `\\?\` 付きパスの比較不一致。

両方ともGameSessionの生成やリプレイ採点を呼ばないスキンパスのテストで、
該当実装・テストは変更していない。変更前HEADでの再実行は未実施。
生ログはGit管理外の `.local/replay-speed-*.log` に保存した。

## 未実施の実機確認

Windows v0.4.3、7K AUTO(LN)、BGA OFF、Vulkan、VSync/144 FPS、
表示オフセット-20msおよび0での画面・音声を伴う再生比較は未実施。
自動テストは採点ロジックとruntime連携の検証であり、VulkanやVSyncの
実動作・実際の表示FPS・音声・性能改善を確認したものではない。
