# Luxe Flatリザルトのランク差分とLua数値評価順序

## 症状と原因

- Luxe Flat Resultで1805ノーツ・EX SCORE 3610の満点時に `MAX+` のみ表示された。
- 実スキンの `resultmain.lua` は満点で `rank_diff_count` に0を返し、`MAX+` と数字を同条件で表示する。
- このvalue callbackは `rank_plus` / `scorerate` 等の共有状態も更新する。Autoのnearest-rank推論が戻り値だけを組み込み式へ置き換えたため、描画用の永続VMでは `rank_plus=false` が残っていた。
- さらにBMZはdraw判定後に数値を取得していた。参照beatorajaの `SkinNumber.prepare` は数値取得後に `SkinObject.prepare` のdraw/timer判定を行う。Compatで関数を残すだけでは、満点時に全ての数値destinationが非表示となり更新関数へ到達しない。

## 変更範囲

- Resultの既知の `rank_diff_count` / `rank_diff_*` 部品のうち共有 `rank_plus` upvalueを持つvalue / drawはAutoでも永続Lua VMに残す。Selectのランク推論・スコア有無判定、通常のref/式推論、他のランク計算やBMZ拡張refの意味は変更しない。
- static（Play / Result等）とSelectの数字runtime callbackをdestinationの表示判定前に評価し、取得済みの値を描画処理へ渡す。destination単位で評価し、二重評価やフレームをまたぐLua結果のcacheは行わない。
- 失敗値・整数sentinelでは表示を止める。Lua VMの命令数上限・エラー診断は既存処理を利用する。
- 第三者スキン、画像、設定、DBは変更しない。

## 検証

- 最小fixtureで、数値の戻り値が単純なrefと同じでも共有状態更新がAuto / Compatで維持されることを検証。
- rendererのPlay / Result / Select経路で、非表示destination・timer OFF・失敗値・sentinel・cache hitを含む評価順序と呼出回数を検証。
- 実際のLuxe Flat Result / Course Resultの定義を読み込み、gradeのdestination順序・条件・数字atlas定義を保持した描画テストを実施。GPU texture handleのみテスト用に差し替える。
- 1805ノーツでMAX+0、MAX-1、17/18境界の前後、AAA+91、AAA+0、AAA-1、F+0、MAXへの復帰をAuto / Compatおよび繰り返しフレームで確認する。
- `cargo fmt --check`、`cargo check --workspace --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo test --workspace --locked --no-fail-fast` は全て成功。
- 主な対象crateはbmz-player 2112 passed / 6 ignored、bmz-render 667 passed / 4 ignored、bmz-skin 239 passed。Luxe Flatの選曲側の既存推論・スコア有無ガードのテストも成功。
- Luxe Flat、WMII_FHD、ADFX02/Starseeker、MILLIONDOLLARの対象素材が存在する環境で実行し、Resultパネル・ランク差分等の既存テストも成功。GPU画面での見た目およびbeatoraja実機との比較は未実施。
- 生ログはGit管理外の `.local/luxe-grade-workspace-{check,clippy,tests}.log` に保存。

現在の契約は [skin.md](../../docs/skin.md#lua-runtime-compatibility-mode)、対応範囲は [skin-compatibility.md](../../docs/skin-compatibility.md) を参照。
