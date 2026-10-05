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

## 3R Resultのロード条件

レビューでは30/31をプレイ側と記述したが、参照実装の条件はBGA SizeのNormal/Extend。
3Rはこの条件をResultの左右レイアウト選択へ流用していた。
BMZのResultにはPlayスキンと共有するBGA Size設定がないため、ロード状態に
30=true / 31=falseを渡してNormal側を選ぶ。通常・コースResultの先読みも同じ状態を使う。

検証:

- 先読み・確定スコアの両状態、7K / 14Kでincludeが読み込まれる回帰テストを追加。
- `BMZ_TEST_LR2_SKIN_ROOT`でmain側の実素材を指定したappテストでは、
  3R Resultが66 destinations / 5 decoded sourcesとなった。
- fmt、bmz-player check / all-targets Clippy / test成功（2116件、ignore 6件）。
  最初の全テストは同梱素材の参照先不一致で3件失敗し、`BMZ_RESOURCE_DIR`を
  作業領域の`data`へ指定して解消。`BMZ_DATA_DIR`は隔離した`.local/lr2-test-data`を使用。
- ログはmain側の`.local/lr2-fix2-test-resources.log`。GPU画面・実操作は未確認。
