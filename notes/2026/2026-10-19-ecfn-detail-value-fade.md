# ECFN詳細オプションのフェード中に焼き込みOFFが透ける問題

本体の基点: `main` / `4354e5f9`。スキンの基点: `data/skins/ADFX02` の `bmz` / `8b32f86`。
本体には今回と無関係な未コミット差分（CHANGELOG・Cargo・installer）があり、変更・コミットしていない。

## 症状と原因

通常→詳細の切替フェード中、詳細パネルの全値欄に「OFF」が見えた。
値欄は`option.png`の灰色ボタン(1581,290,175,37)を236×44へ拡大し、その内側へ不透明の黒を重ねていた。
ボタン素材には「OFF」が焼き込まれており、両方が同じ不透明度aでフェードすると、
黒が下をaの割合しか覆わず、OFFがa×(1−a)の強さで見える（a=0.5で最大）。
登場(timer 22)・退出(timer 32)の両方、選択式の候補と数値欄のすべてで発生していた。

## 修正

- ボタン素材全体を描かず、文字のない上縁(1590,290,20,2)だけを`value_edge`として切り出し、
  値欄の外周へ3pxの4辺を描く。内側の黒(230×38)は4辺と重ならない。
- 色は(124,124,124, α245)で一様。rendererはUVを半texel内側へ寄せるため切り出し境界のにじみはない。
- 太さは従来の見え方（2pxの拡大と黒の3px inset）に合わせた。オレンジの選択表示は文字のない素材なので変更しない。
- `data/skins/ECFN/select/`の同一コピーはユーザー指示により更新していない。

## 検証

- 追加した`ecfn_detail_options_value_boxes_never_draw_baked_off_text`は、Lua修正前で失敗、修正後で成功。
  0↔2・1↔2の切替の各時点で、236×44の描画がボタンの文字中心を含むUVを使わないこと、外周の辺があることを確認する。
- `cargo test -p bmz-player --locked ecfn_detail_options -- --ignored --nocapture --test-threads=1`で6成功。
- `cargo fmt --check`、`cargo clippy -p bmz-player --all-targets --locked -- -D warnings`が成功。
- Windowsのwgpu offscreenで1→2の100ms時点と静止時を目視し、OFFが出ないこと、
  枠と黒欄が従来と同じ見え方であることを確認した。画像は一時ディレクトリで削除済み。
- ログは`.local/ecfn-detail-value-fade-tests.log`。通常ウィンドウでのE1→E2切替は未実施。
