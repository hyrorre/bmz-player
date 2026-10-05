# LR2 Select / Resultレビューの修正

対象ブランチ: `codex/lr2-select-result-compatibility`（レビュー時点 `5f9749e2`）。
現在の契約は [スキン仕様](../../docs/skin.md#lr2のselect--result) を参照。
Windowsで、外部スキンは変更せずBMZ側を修正した。

## 曲名の予約列

`DST_BAR_TITLE` のop1..3相当は予約列であり、OpenLR2の
`AddDrawingBuffer_TextXY`も表示条件として評価しない。
通常destinationへの変換前に予約列を無効化し、外側の`#IF`条件を残した。
回帰テストは予約列に1/2/3が入った場合と複数DSTの結合を確認する。

検証:

- `cargo fmt --check`、`bmz-skin`のcheck / all-targets Clippy / testが成功（245件）。
- 実アセットをappのdecodeとSelect描画評価へ渡して確認。
  WMIX_HD / ECBE / 3R / EndlessCirculation / PLATINUMのサンプル曲名は
  各1件から8件へ復帰。LR2 / Seraphic / RED_BELTは従来の件数を維持。
- 自動テストの外部アセット確認は作業領域に素材が無くスキップされるため、
  上記の実アセット確認はmain側の素材を読み込む別プローブで実施。
  ローカルログはmain側の`.local/lr2-fix1-repros.log`（共有されない）。
- GPU上の画面表示・実操作は未確認。
