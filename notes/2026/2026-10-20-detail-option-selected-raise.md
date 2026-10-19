# 詳細オプションの選択列を上へずらす

本体の基点: `main` / `8cf46d21`。スキンの基点: ADFX02 `bmz` / `b1eb7a6`、mz-select `main` / `ffe599e`、
Luxez-Flat `main`（別作業のコミットを含むHEAD）。

## ユーザーの依頼

同じ形式のパネルを採用している別のゲームの画像のように、選択された項目を上にずらして分かりやすくしたい。

## 実装

- 選択項目は常に中央のslot 3にあるため、3スキンとも描画関数でslot 3の要素だけyを24px上げた。
  枠・塗り・文字・選択肢・クリック領域・退出中の表示が同じ関数を通るので、まとめて動く。
- 本体の変更はない。列の高さ（550px）は変えず、列全体を上へ移す。
- 項目を移動すると、新しい選択列はその時点で上がった状態になり、横移動の補間（`bmzDetailScroll`）の間も
  上がったまま滑り込む。前の選択列はその時点で下がる。移動量に応じて上下を補間するには、
  列ごとの位置に応じた変位を本体が計算する必要があり、今回は行っていない。

## 検証

- 3スキン共通の枠色テスト（`assert_frames_follow_categories`）に、中央列の枠が他の列より24px上にあることの確認を追加した。
- 中央列の選択肢・見出しを固定のy座標でクリックしていたLuxez-Flat・ECFNのテスト（別作業で追加されたGPUテストを含む）を、
  持ち上げ分を引いた座標へ更新した。
- `cargo test -p bmz-player --locked -- --include-ignored detail_option`: 71成功（ECFN・GPU previewを含む）。
- WindowsのGPU offscreenで3スキンの静止時と、mz-selectのスクロール途中（0.5列）を目視した。画像は確認後に削除した。
- 通常ウィンドウでの操作は未確認。

## 追補: 上下も移動に合わせて補間する

ユーザーの依頼で、項目の移動に合わせて選択列の上下も滑らかにした。

- 本体に、destinationの`"bmzDetailRaise": [column, dy]`を追加した（`bmz-skin-document`の`SkinDestinationDef`、
  `bmz-render`の`apply_skin_offset_to_frame_inner`）。見かけの列位置`column + scroll`と中央（3）との距離dから
  `dy * max(0, 1 - d)`だけ上げる。`bmzDetailScroll`と同じ場所で計算するので、描画とクリック判定の両方に効く。
- 3スキンは、slot 3だけyを上げていた処理をやめ、列のdestinationすべてに`bmzDetailRaise = {描画列, 24}`を付けた。
  描画列は枠外のslot 7が-1、slot 8が7。
- Luxez-Flatは各destinationに表が増えたが、Luaのテーブル量の上限内で読み込めることをテストで確認した。
- `bmz-render`に、列とscrollの組み合わせ（中央で24、0.5列ずれで12、1列ずれで0など）の単体テストを追加した。
  ECFN・Luxez-Flatのスクロール途中のクリック位置のテストは、補間した持ち上げ量で座標を計算するように更新した。
- GPU offscreenで、mz-selectのスクロール途中（0.5列）に中央を通る2列がどちらも12px上がることを目視した。
