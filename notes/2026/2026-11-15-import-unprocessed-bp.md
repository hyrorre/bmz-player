# 途中終了した外部スコアのBP・CB（#105）

対象はmain `8a5b701c`。[Issue #105](https://github.com/hyrorre/bmz-player/issues/105)の
GREAT 1 / POOR 2 / 総ノーツ2006で、BEST BPが2005から2へ過小化する不具合を修正した。
現行仕様は[スコア保存](../../docs/score-persistence.md#外部スコアdbの最小bp)を参照。

## 原因と変更

外部スコアの組み立てで`past_notes = expected_notes`としていたため、保存処理の
`bp_with_unprocessed_notes` / `cb_with_unprocessed_notes`に残ノーツが渡らなかった。
元のminbpを保持しても、判定由来の過小なBPとのMINで消えていた。

- beatoraja / LR2oraja / DXのscoreとscoredatalog: PGREAT〜POORを処理済みとして数え、
  Empty Poorは除外する。独立ランプがクリア済みでも最高EXの判定内訳は途中終了の
  可能性があるため、ランプにかかわらず残ノーツを補う。
- LR2: PoorにEmpty Poorが合算される。残ノーツを超えるPoorだけ分離して、BPの合計を
  維持しつつCBへの空POOR混入を除く。途中終了時の正確な履歴BPは識別不能だが、
  既知のsource minbpを推定値で下回らせない。未知から既知への補完時は、他履歴も含めて
  BEST BPを再集計する。削除後の再集計にも同じ規則を使う。
- migration v37: 修正前の単曲外部DB履歴だけを判定内訳から修復する。Local、IR由来、
  course、既に未処理補完された履歴は対象外。影響キーの独立BP・CBを再計算し、
  代表ID・ランプ・回数・IR識別情報・リプレイ所有権を保全する。LR2の確定できる
  Poor分離だけは履歴と代表BESTの判定内訳にも反映する。

## 確認した関連経路

| 経路 | 確認内容 |
| --- | --- |
| 全4種別のDB import / scoredatalog fallback | 途中終了、完走、空POOR、0判定、不明minbp、独立ランプ |
| 保存・代表比較・独立ベスト | 履歴BPとsource minbpの分離、ローカル実績とのMIN、既存代表比較 |
| 再import | 判定・BP/CBがmigration後も一致し、履歴・回数を増やさない |
| cleanup / rebuild | 残存履歴から再計算し、削除した実績を残さない |
| 選曲・songlist・リザルト | `best_scores_for_charts`の独立BP/CBを使う。表示側の再計算は不要 |
| リプレイ・IR更新判定 | `play_finish`は`record_bp/record_cb`と独立BESTを比較する |
| IR download / backfill | pass_notesと判定の整合を使用。IR取得の出所を持つ履歴はmigrationから除外 |
| LR2コース | source minbpを直接保存するため同じ過小化はない。beatorajaコースは従来どおり非対応 |
| 通常プレイ | 既存の未処理BP/CB加算を維持。import専用の処理済み数と補完条件だけ変更 |

LR2の途中終了時の空POOR内訳、外部DBにない独立最小CB、旧Local由来と区別できない
履歴は推測で復元しない。過去時点のprevious-best snapshotと代表履歴の選択は保全する。

## 検証

Debian 13の隔離コンテナ、Rust 1.100.0、FFmpeg 7の開発依存を使用。
実ユーザーのDB、Windowsでの実機UI、外部IRへの送信は未実施。
ログは作業環境の`/workspace/bmz-105-*.log`に保持し、Gitへ含めない。

- `cargo fmt --check`: 成功。
- `cargo check -p bmz-player --locked`: 成功。
- `cargo clippy -p bmz-player --all-targets --locked -- -D warnings`: 成功。
- `cargo test -p bmz-player --locked --lib storage::score`: 137件成功。
- `cargo test -p bmz-player --locked --lib storage::migration`: 12件成功。
- `cargo test -p bmz-player --locked --no-fail-fast`: libは3232件成功、31件失敗、67件ignore。
  binary / integration / doc-testターゲットは成功（0件）。失敗内訳は日本語既定値を
  前提とする4件、未取得の同梱スキンを要求する25件、変更していないscanテスト2件。
  日本語ロケールで表示関連10件と設定復元1件を再実行し、全件成功した。
  scanの`overlapping_external_links_read_every_folder_once`と
  `scan_song_roots_skips_unreadable_subdirectory_and_keeps_accessible_files`は
  非rootの単独実行でも失敗したため、未解決の既存テストとして分けて報告する。

追加9テストは4インポート種別、score/scoredatalog、未処理ノーツ、Empty Poor、独立ランプ、
未知minbp、ローカルとの混在、再import、cleanup、旧DB修復、移行の再実行とrollbackを確認する。
全体実行には既存のIR download/backfill、通常保存、リプレイ更新判定、選曲snapshotテストも含む。
