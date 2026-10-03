# LR2スキンの既定解像度修正

## 原因と変更

KCOOL SKIN 1.72の7keyは `#RESOLUTION` を省略し、640×480の座標を使う。
BMZのLR2ヘッダー既定値が1280×720だったため、画面の横1/2・縦2/3に描画されていた。
参照beatorajaの `SkinHeader` は `Resolution.SD`（640×480）を既定値としている。

`bmz-skin` のLR2既定値を640×480へ修正した。プリセットと幅・高さの明示指定は維持する。
外部スキン・設定・画像は変更せず、Key4やフォント等の別件は修正対象に含めない。
仕様は [skin.md](../../docs/skin.md#lr2の基準解像度)、対応状況は
[skin-compatibility.md](../../docs/skin-compatibility.md) を参照。

## 検証

- 外部アセットに依存しないCSVを使い、ヘッダーからinclude先まで実際に読み込む回帰テストを追加。
  解像度省略と明示的なSD指定の両方で、640×480の背景がキャンバス全体を占め、
  ノーツ領域の上下座標・高さ・ノーツサイズが維持されることを確認する。
- 修正前は省略時の幅が1280となり、追加回帰テストが失敗することを確認した。
- 既存のchart destinationテスト1件は、解像度未指定の高さ720を前提にしていたため、
  高さ480での期待座標へ更新した。プリセットと幅・高さの明示指定の既存テストも成功。
- `cargo fmt --check`、`cargo check -p bmz-skin --locked`、
  `cargo clippy -p bmz-skin --all-targets --locked -- -D warnings` は成功。
- `cargo test -p bmz-skin --locked --quiet`: 233 passed / 0 failed。
- `cargo test -p bmz-player --locked skin_loader::tests::lr2 --quiet`: 43 passed / 0 failed。
  外部アセットがない場合に早期returnするテストを含む。
  手元には `WMII_FHD_LR2/play/FHDPLAY_AC.lr2skin` があるが、
  `WMII_FHD/play/FHDPLAY_AC.lr2skin` と同DP版はないため、これらの素材依存部分は未検証。
  KCOOLの素材は存在するが、追加回帰テストは上記の独立したCSVを使う。
- `git diff --check` と変更文書のローカルリンク確認は成功。
- GPUを使うKCOOL実画面確認、macOS/Linux実機確認は未実施。
