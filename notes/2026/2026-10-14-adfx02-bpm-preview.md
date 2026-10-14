# ADFX02 (EC:FN / Starseeker) 7KのBPM変化予告

## 依頼と範囲

[BPM変化予告の拡張ref](2026-10-14-bpm-change-preview.md)を、別途導入するADFX02のEC:FNと
Starseekerの7Kプレイスキンへ実装した。ユーザー指定により、ADFX02の`bmz`ブランチへcommitし、
DP・5K・EC:FNの`play7wide`は対象外とした。ADFX02は本体Git管理外の別repoであり、
本体のsubmodule・配布物には追加しない。

EC:FNのreadmeは二次配布を禁じ、改変Luaは「オリジナルに上書きする形式」での配布を認める。
Starseekerはrepo内に規約がない（作者は同じ）。ユーザーの方針は「基本的に自分用、許可が取れたら配布」。
変更はLuaと説明文書だけとし、画像・フォントの追加や加工は行わない。

作業はセッションのworktree内にADFX02の`bmz`ブランチをcloneして行った（`data/skins/ADFX02`、Git管理外）。

## 実装

- 両スキンの`play/bmz_bpm_preview.lua`（同一内容）が部品を組み立て、`play7main.lua`は
  `bmz`がある場合だけオプションを追加し、`pcall`で構築に成功した部品だけを閉店・フェードより手前に追加する。
- 数字はPGREAT時の±ms表示と同じ画像（EC:FN `main.png` / Starseeker `system.png`の`x=207`、
  青`y=3214`、赤`y=3230`、16×16）を等倍・最大3桁で使う。ユーザー指定で濃い色の行を採用した。
- 「»」は同梱の`GenEiMGothic2-Heavy.ttf`（数字画像の元フォント）で描き、黒の縁取り1pxを付ける。
  GPU画像で数字と縦の中心を測り、サイズ30・2pxの持ち上げを4pxに変更して中心差を1px以内にした。
- 高さはTYPE Aが判定文字の上の表示（EC:FN `y=558+18`、Starseeker `y=563+18`）の8px上、
  TYPE Bが判定文字（`y=470`）の下、TYPE Cが判定文字の中央、TYPE Dが判定線（`y=357`）の中央。
  Starseekerの判定文字はテーマごとに高さが異なるため、標準テーマ（84px）を基準にした。
- TYPE C/Dは1Pならレーンの右（`x=532`から左詰め）、2Pなら左（`x=1396`まで右詰め）。
  当初は黒い下地を敷いたが、ユーザー指定によりFAST/SLOW・スコア差分と同じく下地なしにした。
- Starseekerのレーン外にあるfast/slow（TYPE-B `y=524..542`、TYPE-C `y=393..411`）と
  スコア差分（TYPE-B/C）とは、重なるか6px未満に近づく場合、ユーザー指定によりその上へずらす。
  余白なしの判定ではTYPE Cがスコア差分TYPE-Bとfast/slow TYPE-Bの間に2〜4pxで挟まるため、6pxとした。
  EC:FNのスコア差分（`y=558`）も同じ規則で避けるが、現在の配置では重ならない。

## 検証

検証画像は `.local/performance/bpm-change-preview/adfx02/` に保存（Git管理外）。

- `crates/bmz-player/src/skin_loader/tests/adfx02_bpm_preview.rs`の4テスト（素材が必要なためignored）を
  明示実行し成功。2スキン×1P/2P×TYPE A〜Dの宣言・数字画像・配置、OFFとBMZオプションの追加、
  Starseekerのfast/slow TYPE-B・スコア差分TYPE-Cの上への移動を確認した。
- GPUテストで2スキン×4条件を描画し、予告前の非表示、青・赤の数字、レーン側の数字が枠から8px以内、
  TYPE A/Bの数字と「»」の縦の中心差1px以内を確認。画像も目視確認した。
- 実プレイ確認、DP・5K・`play7wide`、標準以外の判定テーマでの見た目は未確認。

```sh
cargo test -p bmz-player --locked skin_loader::tests::adfx02_bpm_preview -- --ignored --test-threads=1
```

## 2026-10-14 追記: 判定詳細との重なりのスクリーンショット

`adfx02_bpm_preview_judge_detail_screenshots`（ignored、`BMZ_BPM_PREVIEW_DIR`へ保存）で、
判定文字・FAST・スコア差分を表示した状態の全組み合わせを撮影した。
Starseekerは1P/2P × fast/slow位置TYPE-A〜C × スコア差分位置TYPE-A〜C × 予告TYPE A〜Dの72枚、
EC:FNは1P/2P × 予告TYPE A〜Dの8枚。画像と一覧は `.local/performance/bpm-change-preview/overlap/`。

撮影条件で分かったこと:

- Starseekerのスコア差分の数値定義はスコアグラフのモジュールにあり、「スコアグラフ」がOnの場合だけ表示される。
- 判定文字は30msごとに文字と空白のフレームを交互に表示するため、判定から130ms後で撮影した。
- 1つのrendererへ別スキンを続けて導入すると、同じtexture IDが残るため、撮影ごとにrendererを作成した。

確認結果: Starseekerでレーン外のfast/slow・スコア差分と重なる組み合わせでは、予告がその上へ移動し、
複数ある場合は「予告、FAST、スコア差分」の順に積み重なる。TYPE A/Bはレーン内の表示と重ならない。
EC:FNはスコア差分（判定詳細の高さ）と重ならない。

## 2026-10-15 追記: LIFT・オフセットへの追従と表示位置の調整

TYPE Aをfast/slowと同じ `{3, 33}` に追従させ、オフセット「BPM変化予告の表示位置」（id 48、x/y/w/h/a）を追加した。
詳細は[antique側の記録](2026-10-14-bpm-change-preview.md)と同じ。両スキンで読込・描画計画の明示実行テスト
（`adfx02_bpm_preview_offset_ids_follow_neighbors`、`..._custom_offset_adjusts_position_size_and_alpha`、
`..._moves_with_lift_and_judge_offsets`）を実行し成功。