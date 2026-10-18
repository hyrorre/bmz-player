# 詳細オプションの大分類の再編と枠の色分け

本体の基点: `main` / `fabc6e41`。スキンの基点: ADFX02 `bmz` / `99875b2`、
mz-select `main` / `809babd`、Luxez-Flat `origin/main` / `e08c40f`（ローカル`main`は`d01a305`で、HEADの祖先）。

## ユーザーの指定

- 表示順: JUDGE → LANE / HI-SPEED → GAUGE → LONG NOTE → BGA → ASSIST / MODIFIER。
- LANEとHI-SPEEDは1つの大分類にまとめる。
- 色: JUDGE・LANE / HI-SPEEDは同じ青系、GAUGEは緑系、LONG NOTEは橙系、BGAは赤系、
  ASSIST / MODIFIERは紫系かピンク系。ECFN・mz-select・Luxez-Flatの3スキンに実装する。

## 本体

- カタログを上記の順へ並べ替えた。安定ID・保存先・入力は変更していない。
- CATEGORY_IDは既存の値を維持し、LANE / HI-SPEEDを1にまとめ、旧HI-SPEEDの3は欠番とした。
  翻訳は`detail-options-category-1`をLANE / HI-SPEEDへ変更し、`-3`を削除した。
- カーソル位置を数値で指定していたテストは、`cursor_of(安定ID)` / `DetailOptionDef::by_id`で参照するよう変更した。
  先頭付近に数値行（判定表示オフセット）が来たため、列ごとのクリック先は値の型に応じて確認する。

## スキン

最初はdraw式（`number(19401+10*s)==N`）で色ごとにdestinationを複製した。
ECFN・mz-selectは動作したが、Luxez-Flatは角丸の帯が多いため、Luaのテーブル量の上限（20万）を超えた。
上限はsandboxの安全策なので緩めず、beatoraja互換の`imageset`の`value`に列の`category_id`を渡す方式へ変更した。
画像の並びをcategory_id順（0と欠番3はシアン）にし、destination数は変更前と同じにした。

- ECFN: `option.png`の通常パネルの枠5色を使用。いずれも幅209px・上部16px・縁が同じ構造で、下端だけ異なる。
  シアンは2P「譜面の配置」(1563)、緑はグルーヴゲージ(1143)、橙は1P「譜面の配置」(933)、
  赤はDPオプション(1773)、ピンクはスクロール固定(1353)。
- mz-select: シアンは`panel2.png`、ほかは`panel3.png`のカード（3pxの縁と単色の見出し）。
  選択時の塗りは見出しの色を不透明度96で重ね、以前の`07373760`とほぼ同じ見え方にした。
- Luxez-Flat: 紫のカードと角の不透明マスクが一致するカードだけを使用。
  シアンはBGA、橙はGAS、赤は判定自動調整（`option3_panel_ver1.3.0.png`）、緑はグルーヴゲージ（`option1_panel_bmz.png`）。
  1P「譜面の配置」・判定タイミング・その他は角の形状が異なるため使っていない。
  見出し・選択時の塗りは各カードの内側の縁を1px切り出し、不透明度160 / 128で描く。

## 検証

- 3スキンに`*_frames_follow_row_categories`を追加した。複数のカーソル位置と退出中のsnapshotで、
  各列の左辺が大分類に対応する素材から描かれ、全大分類（1,2,4,5,6,7）が現れることを確認する。
- `cargo test -p bmz-player --locked`で2,576成功・34 ignored。`cargo fmt --check`、
  `cargo clippy -p bmz-player --all-targets --locked -- -D warnings`も成功。
- ECFNの外部素材を使うテストは`--ignored`で明示実行し、7成功（GPU previewを含む）。
- WindowsのGPU offscreenで3スキンの日本語・2つのカーソル位置を目視した。
  色の割り当て、上端・角の切り出しに焼き込み文字や背景が混ざらないこと、見出し・選択時の塗りを確認した。
  画像は一時ディレクトリに出力し、確認後に削除した。
- 通常ウィンドウでの操作、英語以外の長いラベル、他の解像度の目視は未実施。
