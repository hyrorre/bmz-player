# `text.numberRef` の正符号表示

[Issue #150](https://github.com/hyrorre/bmz-player/issues/150) の対応。
LR2の数値描画は画像セルを選ぶ方式で、24セル形式では正数とゼロに正側の符号セルを使う。
BMZのフォントによる数値表示では、曲リストと同じ `showPlus` を各 `text` 要素に指定できるようにした。
[スキン仕様と使用例](../../docs/skin.md#bmz-default-json-skin)

beatorajaの `108` / `128` / `153` は同じターゲット差を参照するため、BMZの描画時とResult Luaの読み込み時でも値を揃えた。
LR2 CSVのPlayとResultは従来の内部refで評価し、その他の画面では未変換の `108` / `128` を非表示とした。
[ターゲット参照仕様](../../docs/skin.md#play-target-score-refs)

## 検証

- macOSで `cargo fmt --check`、workspaceの `check` と `clippy --all-targets -- -D warnings` が通過。
- `BMZ_REQUIRE_MZ_SELECT=1` でworkspaceのテストを実行し、Lua / JSONの符号整形、ターゲット差、LR2 CSVの回帰テストが通過。
  ループバックを許可した再実行では、`bmz-video` の既存テスト1件がタイムアウトし、同じバイナリでの単独再実行で通過。
- 同梱のmz-selectは配置済み。WMIX_HD・LITONE5など未配置の外部素材を使う項目はスキップ。
