# LR2専用のnumber / option参照

## 背景

LR2 CSVスキンのnumber / option IDをbeatorajaの参照として評価していたため、
同じ番号で意味が違う値がずれていた。報告された例は次の2件。

- リザルトのPOOR（number 114）: LR2は見逃しPOOR＋空POOR（`LR2_skinobject.cpp` の `GetNumber` case 114）。
  BMZはbeatorajaの `NUMBER_POOR`（見逃しのみ）を表示していた。2P側の134だけは合計になっていた。
- 選曲のINTERNET RANKING（number 92..95）: LR2は92=IR順位、93=参加人数、94=クリア率、95=ライバルとのEX差。
  BMZはbeatorajaの `NUMBER_MAINBPM`（92）として評価し、順位の位置にBPMが出ていた。93..95は空欄だった。

サブエージェントで同梱・手元のLR2スキンとOpenLR2の定義を照合し、他の不一致も一覧化した。

## 変更

- `bmz-skin-document` にLR2専用の番号領域 `LR2_NUMBER_BASE`（93_000）と `LR2_OPTION_BASE`（94_000）を追加した。
- `bmz-skin` の `lr2/refs.rs` に、意味が違うIDの表を集約した。変換時にnumber refとdestinationのopをLR2専用領域へ移す。
  - number: Playの100/107/108/109/114、Select / Decideの92..95、全画面の290/291→90/91。
  - option: 52..57・620..623を固定値（BMZに該当設定が無いため）としてロード時と描画時の両方で扱う。
    Select / Decideの100..105・110..117は曲ごとに変わるため描画時に評価する。Resultの340..347も移す。
- `bmz-render` に `number/lr2.rs` と `condition/lr2.rs` を新設した。2P側の `lr2_opponent_number` も移し、
  スコア・ゲージ・次ランク差の計算を1P / 2Pで共有した。
  ResultのLR2 1P番号（100..116、反転後の自分側）のうち100/107/108/109/114もLR2の定義で評価する。

## 未対応

値の供給が必要なため、今回は扱っていない。

- レベルバー（option 500..565）: 同じフォルダ内の他難易度の情報が描画状態に無い。
- ゴーストの進捗値（number 150/151、155..158）、SP時の2P番号（120..136）。
- IRのクリア人数の5分類（number 210..219）、ライバル記録の追加項目（272..279、292/293）。
- number 37/84/89の空POOR、option 600..616のIR状態、text 16/17等。

## 検証

- `bmz-skin` に変換の回帰テスト（`lr2/tests/refs.rs`）を追加した。
  Play / Select / Resultのnumber ref、destinationのop、`#IF` 内のruntime化、固定値のロード時評価を確認する。
- `bmz-render` に評価の回帰テストを追加した。POOR＋空POOR、LR2式のスコア・ゲージ・ターゲット差・次ランク差、
  Result反転後の自分側、選曲のIR順位・人数・クリア率・ライバル差、クリアランプ・ランク・新ランク・固定値を確認する。
- 手元のLR2スキン（LR2標準 / Seraphic / ECBE / RED BELTのSelect、Seraphic / RED BELT / ECのResult、
  LR2標準 / 3RのPlay）を変更前後で読み込み、destination数が変わらず、LR2専用numberに置き換わることを確認した。
- `cargo fmt --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、
  `cargo test --workspace --locked --no-fail-fast` は成功。
- GPUでの実画面確認とIR接続時の選曲画面の確認は未実施。
