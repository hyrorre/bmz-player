# PLAYターゲットスコアの最終値表示修正

- 対象: [Issue #25](https://github.com/hyrorre/bmz-player/issues/25)
- 調査元: main `674ff19212c678f2c487bec1e31b63209771b1bc`
- 環境: Windows、作業ブランチ `codex/fix-target-score-25`
- 現在の仕様: [Play Target Score Refs](../../docs/skin.md#play-target-score-refs)

## 原因と修正

`bmz-render` の `skin/state_values/number/state.rs` で、数値ref `121/151` に
`projected_score_at_progress()` を適用していた。最終ターゲット1600、総ノーツ1000、
通過250では400になり、最終値を表示するbeatoraja互換refの用途と異なっていた。
ローカル参照 `.local/beatoraja` の `721856fbb4311241ea81711e0574e5cc094c13c2` でも、
`IntegerPropertyFactory` の両refは `getRivalScore()` を直接取得する。

両refを現在の `SkinDrawState.target_ex_score` の直接取得へ変更した。
IR取得後の値の更新もそのまま反映し、開始時の値を保持するキャッシュは追加しない。
数値 `153` の進捗差分、グラフ `114` の進行値と `115` の最終値の割合は維持する。
自己ベスト・ターゲット率・LR2の2P用番号変換と取得処理は変更しない。

`value.ref` は `skin_value_number_for_destination()` → `skin_value_number()`、
`text.numberRef` は `skin_state_text_with_draw_state()`、Luaは `RenderLuaMainState::number()` →
`lua_main_state_number()` から同じ `skin_state_number()` に接続する。
デフォルトスキンの `_play_text.json` は既存の `numberRef: 121` のまま対応する。

## 回帰テストと検証

`skin/tests/graphs_more.rs` の既存テストを
`best_and_target_scores_show_final_values_while_differences_follow_note_progress` に改名し、
`121/151 = 1600`、`150/170 = 1800`、`152/172 = 0`、`153 = 50` を確認する。
グラフ `114 = 0.2`、`115 = 0.8` は既存の近似比較helperを使う。

追加テストは次を確認する。外部スキンやGPUに依存しない。

- 通過ノーツ0・250・1000のすべてで `121/151 = 1600`。
- 0点・未設定・総ノーツ0の数値ref。
- 進捗25%の代表ケースで、value / text / Lua経路の未設定 → 1600 → 1800 → 0 → 未設定。
  未設定の扱いはそれぞれ `None` / 空文字 / `0` を維持する。

修正前の実装で対象26テストを実行し、23成功・3失敗を確認した。
最終値1600に対して開始時0・進捗25%で400を返す不具合を再現できた。

修正後の検証結果:

- `cargo fmt --check`: 成功。
- `cargo check -p bmz-render --locked`: 成功。
- `cargo clippy -p bmz-render --all-targets --locked -- -D warnings`: 成功。
- `cargo test -p bmz-render --locked`: 670成功・0失敗・4 ignored、doc-test 0件。
- `git diff --check`、追加文書のリンク先・見出し・コード参照を確認。

既存のResult数値ref、自己ベスト進行値、数値画像描画・キャッシュ、text拡張のテストも成功。
`lr2_battle_state_projects_live_opponent_without_a_target_score` では、
ターゲット未設定でも2P専用内部refが対戦相手EX SCORE 789を返すことを確認した。
今回の回帰テストとこれらの関連テストは外部スキンの有無による早期returnを使わない。

4件のignoredは、GPUを要する既存の減算合成・texture upload・Ambientのテストで、
今回の数値ref変更では明示実行しない。実機での表示確認、第三者スキンの見た目の確認、
beatorajaとの同条件の画面比較、macOS / Linuxでの実行は未実施。
