# LR2選曲スキンのパネル2以降と埋め込みスキン設定

## 背景

LR2選曲スキンの多く（ECBE、EndlessCirculation SE、PLATINUM、3R、W-mix HD）は、
パネル2（システム）にスキン設定を埋め込んでいる。BMZはLR2スキンでパネル2以上を
スキンへ「パネルなし」として渡し、代わりにBMZのASSIST / DETAILの文字パネルを重ねていたため、
これらのパネルとスキン設定は表示も操作もできなかった。

ユーザーと相談し、LR2スキンではE2 / E1+E2でスキン自身のパネルを表示し、
ASSIST / DETAILはF1の設定画面で変更する方針（案a）にした。

## 変更

### パネル

- LR2スキンへ実際のパネル番号（1..6）を渡す。BMZのASSIST / DETAILの文字パネルは重ねない。
  画面下部の案内は「F1: SETTINGS」だけにした。
- パネルボタン1..6でそれぞれのパネルを開閉する（LR2のtimer 21..26 / 31..36が対応する範囲）。
- LR2スキンのパネル2以降では鍵盤・ゲームパッドでASSIST / DETAILを変更しない。

### スキン設定

- 対象（button 170..185）をBMZのスキン区分へ対応付けた。KEYCONFIG / SKINSELECT / SOUNDSET / THEME / BATTLE9は対象外。
- スキン変更（190）はBMZの既定と導入済みスキンを順に切り替える。区分ごとの設定履歴を保存・復元する。
- カスタマイズ（220..229、slider 7でスクロール）は、F1の設定画面と同じ選択肢・ファイル候補・既定値の規則で値を切り替える。
- 変更はプロフィールのスキン設定へ書き込み、対象のスキンをその場で再読み込みする。
- 表示用の一覧は、対象区分のスキンパス・選択肢・ファイル選択が変わったときだけ作り直す。
  F1の設定画面での変更も反映する。スキン名・作者はスキンヘッダーから読む。
- `WinitApp` のサイズ上限を超えないよう、状態はBoxで保持する。

## 検証

- UIのAPIテスト: 対象の対応付け、スキン変更の折り返しと設定履歴、選択肢・ファイル選択の既定値と切り替え。
- 描画のテスト: スキン名・作者・表示行の文字、対象ボタンの点灯、slider 7の位置。
- 変換のテスト: text 50 / 51 / 100..119のLR2専用参照、button 1..6 / 170..229の動作対応。
- 一時テストで同梱外のECBE選曲スキンのヘッダーを読み、名前「EC:BE」・作者「ric」と、
  Banner / LR2 Resolution (BMZ) / BGA Size / BackGroundImage / Character / Shutterの6行が既定値付きで並ぶことを確認した。
- `cargo fmt --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、
  `cargo test --workspace --locked --no-fail-fast` は成功。
- 実画面でのパネル表示・クリック操作・スキン再読み込みの確認は未実施。

## 追記: 設定ラベルと14K / 10Kの入れ替わり

- ECBEのシステムパネルでBGA・AUTO ADJUST等の値欄が空だった。LR2のtext 60..85は `LR2files/Config/optionstr.csv` の文言で、
  BMZで変更できる設定（60..65 / 70 / 74 / 76 / 80 / 84）だけ、BMZの表記で現在値を表示する。
  ユーザー環境のLR2（`G:\LR2`）の `optionstr.csv` と比較したうえで、ユーザーの判断によりBMZの表記（NORMAL / HARD / HAZARD、H-RANDOM / ALL-SCR、SCORE等）を採用した。
  キーフィルターだけは逆にBMZ側をLR2の表記へそろえ、JSON / Luaスキンの `bmz_select_mode` を含めて `ALL KEYS` / `7KEYS` / … / `4KEYS` / `6KEYS` / `8KEYS` と表示する。プロフィールへの保存値（`ALL` / `7K` 等）は変更しない。
- ECBEのスキン種別タブで14Kと10Kが入れ替わる件は、ECBE側の割り当てと判断した。
  - OpenLR2はbutton 170..188を `op - 170` でスキン種別へ変換し、種別は2が14KEYS、3が10KEYS。
  - LR2同梱の `skinselect.csv` はコメントが逆（173を14KEYS）だが、`skinselect.tga` の該当領域を描画すると
    173のボタン画像は「10KEYS」、172は「14KEYS」だった。RED BELTとW-mix HDも172を14Kに割り当てている。
  - ECBEは画面上の「14K」タブに173、「10K」タブに172を割り当てている（EndlessCirculationのskinselect.csvも同じコメント）。
  - 第三者スキンは書き換えず、BMZもLR2本体の割り当てを維持する。