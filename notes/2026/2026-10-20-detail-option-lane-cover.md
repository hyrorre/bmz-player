# 詳細オプションのSUDDEN+ / HIDDEN+ / LIFTを1項目にまとめる試行

本体の基点: `main` / `473fcdf5`以降。スキンの基点: mz-select `main` / `1dce687`。

## ユーザーの依頼

SUDDEN+・HIDDEN+・LIFTの3項目を1つにまとめた版を試したい。選択肢は
OFF、SUDDEN+、HIDDEN+、SUD+ & HID+、LIFT、LIFT & SUD+、LIFT & HID+、ALLの8つ。

## 実装

- 3項目（安定ID 101〜103）を、新しい項目「LANE COVER」（安定ID 104、内部キー`lane-cover`）に置き換えた。
  101〜103は欠番。大分類はLANE / HI-SPEED（1）、キーモードごとの設定。
- 選択肢の番号は1=SUDDEN+、2=HIDDEN+、4=LIFTのビット和。指定の表示順がそのまま0〜7になる。
- 保存先は従来どおり`play.lane_effect`（SUDDEN+ / HIDDEN+）と`lane.lift_enabled`。量は変更しない。
  補助情報に3つの保存量（`SUDDEN+ 300 / HIDDEN+ 0 / LIFT 120 (/ 1000)`）を表示する。
- 翻訳は項目名`LANE COVER`と説明を全言語に追加し、旧3項目の名前を削除した（説明は日英以外は英語）。
- mz-selectの選択肢用ビットマップフォント`choices.fnt`に`&`がなかったため、括弧と同じく
  既存の`m_select_profile_24`の字形を同じ縦位置の補正（yoffset −9）で追加した。字形の書体は周りと少し異なる。
- 8択は3スキンとも全8候補の描画枠を確保済みで、GPU画像で列に収まることを確認した。

## 検証

- `lane_cover_combines_toggles_and_preserves_amounts_and_other_modes`を追加。8通りの直接選択、
  前後への循環、保存量・他キーモードの保持、保存と再読み込みを確認する。
- 既存テストの101〜103・`"SUDDEN+"`（項目名）の参照を104・`"LANE COVER"`へ置き換えた。
- `cargo test -p bmz-player --locked`: 2,582成功・38 ignored。ECFNのテスト（`--ignored`）7成功。
- `cargo clippy -p bmz-player --all-targets --locked -- -D warnings`、`cargo fmt --check`が成功。
- WindowsのGPU offscreenで3スキンの中央列（LANE COVER）を目視した。画像は確認後に削除した。
- 通常ウィンドウでの操作は未確認。試行のため、元の3項目へ戻す場合はこの変更を戻す。
