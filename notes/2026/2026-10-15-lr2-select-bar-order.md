# LR2選曲カーソルの発光とBAR部品の描画順

## 症状と根拠

ECBEのSelectスキンで、選択カーソル周囲の発光の下部が直下の選曲バーに覆われる。
`parts_songbar.csv`ではバーが高さ32px、`DST_BAR_FLASH`が高さ52px・相対y=-10で、
発光はバーから上下10pxずつはみ出す。BMZは行ごとに本体・発光・装飾を出してから
次の行へ進んでいたため、下の本体が後から発光を覆っていた。

参照実装の`ReadDST`はDSTへ宣言順の`sortID`を設定し、`LRDraw`は描画bufferを
その順に並べ替える。ECBEでは本体・タイトルの後にFLASHが定義されている。
実素材でのGPU描画と描画項目の位置・順序で再現し、clipで発光領域を切っていないことも確認した。

## 実装

Claudeの実装を引き継ぎ、次の変更を完成させた。

- `bmz-skin`は各BAR destinationの最初のDST位置へsonglistの目印を置く。
  include先も同じdestination列へ追加し、通常の画像・パネル・マスクとの宣言順を保つ。
- `bmz-skin-document`の`lr2BarPart`は部品・スロットと`order`を保持する。
  目印と対応する子destinationへ共通の`order`を付ける。
- `bmz-render`は行ごとに有効な子destinationを選び、その描画項目を対応する目印の位置で出す。
  発光の中央スロットへの固定、スクロール補間、従来のclip適用範囲を維持する。
- 同じ部品でもSRCや`#IF`条件が異なる定義は別の目印を持つ。同じ条件とsourceのDSTだけを
  アニメーションのkeyframeとしてまとめる。クリック領域は最初の目印で一度だけ評価する。
- JSON / Luaは従来の行ごとの描画順を維持する。第三者製スキンと素材は変更しない。

引き継ぎ時の実装では、同じ部品の最初の目印へ全条件分岐を集めていた。
パネル1閉鎖時だけマスクより後に発光を定義する回帰テストが、修正前は
`B B B F M40`、期待値は`B B B M40 F`となって失敗した。
有効なdestinationと目印を`order`で対応付け、別SRCの場合と同じSRCの別DST条件の場合の両方を修正した。

## 検証

Windowsで合成CSVの5テストを実行し、宣言順の入れ替え、通常画像との前後関係、include、
上下スクロール、パネル条件を確認した。converterには最初のDSTとランプのclear-index対応のテストもある。
`cargo fmt --check`、`cargo check --workspace --locked`、
`cargo clippy --workspace --all-targets --locked -- -D warnings`が通過。
通常のWindows環境で`cargo test --workspace --locked --no-fail-fast`を実行し、
合計4,157 passed・0 failed・45 ignoredだった。
最初の制限付き環境では既存の通信・ファイル操作テストが失敗し、一部が待機した。
updaterの失敗には`os error 5`があり、待機した今回のplayerテストプロセスだけを停止した。
通常環境で全体を再実行すると失敗は解消した。環境の制限を避けるために製品コードやテストを変更していない。
Cargoのincremental cacheでhard linkからcopyへのfallback警告があったが、検証は完了している。

実素材のあるECBE、LR2、RED_BELT、Seraphic、WMIX HD、PLATINUM、3R、EndlessCirculation SEで、
停止・下スクロール・上スクロール・パネル1・パネル2の計40画面をoffscreen GPUで生成した。
ECBEの停止・上下スクロール・パネル1、WMIX HDのパネル2、RED_BELT・PLATINUM・EndlessCirculation SEの停止、
LR2の上スクロール、Seraphic・3Rのパネル1を目視し、
ECBEの下側の発光が次のバーに覆われないことを確認した。
曲名とスコア等は合成snapshotであり、実ライブラリ・実入力による操作確認ではない。

詳細ログと画像はローカル限定の`.local/lr2-play-audit/bar-order-*`へ保存した。
共有される記録・配布物へ第三者製素材や生成画像は含めない。

## 対応範囲と未確認事項

各destinationの最初のDST位置を使う。別オブジェクトのDSTをアニメーションkeyframeの間へ挟み、
`sortID`自体を補間して前後関係を変えるLR2の特殊な定義は今回の対象外。
LR2実機との画面比較、実入力での連続スクロール、macOS / Linuxは未実施。

仕様は[skin.md](../../docs/skin.md#lr2のselect--result)、
対応範囲は[skin-compatibility.md](../../docs/skin-compatibility.md)を参照する。
