# 詳細オプションの見た目の追加調整

本体の基点: `main` / `e6184575`。スキンの基点: ADFX02 `bmz` / `aec0a8f`、mz-select `main` / `08fef6a`、
Luxez-Flat `main` / `20752d1`。[前回の作業](2026-10-19-detail-option-normal-panel-style.md)へのユーザーの指摘に対応した。

## 指摘と対応

| スキン | 指摘 | 対応 |
|---|---|---|
| Luxez-Flat | 枠の色が急に変わる部分があり、線に見える | 原因は左右の縁の内側の発光が見出しの下端（y=226）から急に始まっていたこと。角の帯・見出し・本体の各行を、中央の伸ばした色と、カードの同じ位置から切り出した左右の発光に分けて重ね、発光を上から下までつなげた |
| Luxez-Flat | 選択肢のテキストが若干下寄り | 2px上げた。GPU画像で文字の中心とボタンの中心が1px以内になることを確認 |
| Luxez-Flat / mz-select | 項目名に黒縁 | text定義に`outlineColor = "000000FF"`、`outlineWidth = 2`を指定 |
| ECFN | 項目名と小見出しが近い | 項目名を5px上げ、小見出しを3px下げた（間隔約10px） |
| ECFN | 選択肢の枠の幅が広い | 通常パネルの余白（幅209pxの枠に175pxのボタンで左18px・右16px）に合わせ、左右17px・幅224pxにした |
| 共通 | 緑数字・判定タイミングをオレンジではなく未選択の枠で | 3スキンとも数値欄から選択素材（オレンジ・発光）を外した |
| 共通 | 判定タイミングが正のとき`+`を表示 | 本体の詳細オプションの値ラベルで、判定表示オフセットが正のときだけ`+12 ms`のように符号を付けた。設定画面の表示は変更していない |

## 検証

- 追加テスト:
  - `positive_visual_offset_shows_its_sign`（+12 ms / 0 ms / -7 ms）。
  - 3スキンの`*_number_values_use_an_unselected_button`。緑数字・判定表示オフセットの中央列に選択素材が描かれず、
    選択式の項目では描かれることを確認する。mz-select・Luxez-Flatは修正前のスキンで失敗することも確認した。
- `cargo test -p bmz-player --locked`: 2,579成功・35 ignored。ECFNの外部素材を使うテスト（`--ignored`）は7成功。
- `cargo fmt --check`、`cargo clippy -p bmz-player --all-targets --locked -- -D warnings`が成功。
- WindowsのGPU offscreenで、Luxez-Flatの列の上端・下端の拡大、3スキンの数値欄、ECFNの通常パネルとの余白比較を目視した。
  画像は一時ディレクトリに出力し、確認後に削除した。
- Luxez-Flatの描画要素は増えたが、Luaのテーブル量の上限内で読み込めることをテストで確認した。
- 通常ウィンドウでの操作と、判定表示オフセットを正にした状態の画面表示は未確認（値ラベルは単体テストで確認）。
