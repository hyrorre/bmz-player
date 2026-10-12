# LR2スキンの解像度推定と全画面種別の上書き設定

## 原因

ECBEのSelect、EndlessCirculationのResult / Play、W-mix HD等は `#RESOLUTION` を宣言せず、
1280×720の座標で書かれている。[既定解像度の修正](2026-10-03-lr2-default-resolution.md)以降は
640×480として読むため、左上の一部だけが表示されていた。
Play / Decideには、解像度を手動で選ぶ設定もなかった。

## 変更

- `#RESOLUTION` がなく、`LR2 Resolution (BMZ)` が `Auto` の場合は基準解像度を推定する。
  ロード時のoptionで有効な分岐とinclude先から、`0,0,W,H` の `#DST_*` を
  640×480 / 1280×720 / 1920×1080 / 3840×2160ごとに数え、最も多い解像度を使う。
  該当がなければ640×480のままとする。走査したinclude先はdecode cacheの依存に加える。
- `LR2 Resolution (BMZ)` をすべてのLR2スキン種別に追加した。
  先頭項目を `Skin default` から `Auto` に変更し、旧保存値は先頭項目として扱う。
- 上書き設定は `#RESOLUTION,幅,高さ` の明示指定扱いにしない。
  Playで上書きしても、LIFTに追従するボム・キービームの扱いは変わらない。

仕様は [skin.md](../../docs/skin.md#lr2の基準解像度) を参照。

## 手元スキンでの推定結果

一時テストで、`data/skins` の各スキンを既定optionで読み込んだ結果。

| 推定結果 | スキン |
|---|---|
| 1280×720 | ECBE Select、EndlessCirculation SE-Select / SE-Result / SE-Decide / SE-CourseResult / Play SESP、W-mix HD Select / Result / Decide / Play、PLATINUM Select / Play、3R Select / Play / Decide |
| 640×480 | LR2標準 Select / Result / Play / Decide、KCOOL Play / Decide、Seraphic、RED BELT、OA_DX+ |
| 1920×1080 | WMII_FHD_LR2 Play（`#RESOLUTION,1920,1080` を宣言） |

3R Resultは640×480になった。本体の `#INCLUDE` が `#IF,30` / `#IF,31` の内側にあり、
Resultではこのoptionが設定されないため、destinationが0件になる。
解像度推定とは別の既存の問題で、今回は修正していない。

## 検証

- 追加した回帰テストで、Play / Select / Decide / Result / Course Resultのinclude先からの推定、
  分岐ごとの推定、宣言と上書き設定の優先、旧保存値、上書き時に明示指定扱いにならないことを確認した。
- `cargo fmt --check`、`cargo clippy -p bmz-skin --all-targets --locked -- -D warnings` は成功。
- `cargo test -p bmz-skin --locked`: 263 passed。
- `cargo test -p bmz-player --locked skin_loader`: 198 passed / 18 ignored。
  外部アセットがない場合に早期returnするテストを含む。
- GPUでの実画面確認とmacOS / Linuxでの実機確認は未実施。
