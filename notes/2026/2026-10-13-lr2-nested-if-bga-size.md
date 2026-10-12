# LR2の入れ子 #IF とPlay以外のBGA Size

## 調査

3R Resultの `result.lr2skin` は、BGA Sizeのop（30 = Normal / 31 = Extend）で
左右レイアウトのCSVを選び、2つ目の `#IF` を閉じていない1つ目の内側に置いている。

```
#IF,30
#INCLUDE,...\R-L.csv
#IF,31
#INCLUDE,...\R-R.csv
#ENDIF
#IF,11
#INCLUDE,...\BT.csv
#ENDIF
```

- OpenLR2の `ReadSkin`（`LR2/LR2_skinload.cpp`）は各 `#IF` 階層を自分の条件だけで判定し、
  行のスキップも最も内側の階層だけを見る。外側が偽でも、31が真ならR-R.csvを読む。
- OpenLR2はop 0〜899を `GetOptionFlag_dst` から全画面で設定し、op 30/31は
  `config.play.bgasize` に従う。
- beatorajaは入れ子を持たず `#IF` ごとに判定し直す。この例ではLR2と同じ結果になる。
  BGA SizeはPlayスキンにだけ追加するため、beatorajaの3R Resultは30/31が両方偽になる。
- BMZは親の条件もすべて満たすことを要求していた（2026-06のWMII FHD対応で導入）。
  appはResultのロード時に30=true / 31=falseを渡していたため、実画面ではR-L.csvが読まれていた。
  3R Resultの見た目の崩れは解像度の宣言漏れが主因で、
  [解像度推定](2026-10-13-lr2-resolution-inference.md) で解消した。
  同記録の「destinationが0件」は、appのruntime optionを渡さずに読み込んだ結果だった。

## 変更

- `Processor` の `#IF` 判定をOpenLR2に合わせ、各階層の条件だけで判定する。
  描画時に評価するopの条件も、最も内側の階層の分だけを付ける。
- Play以外のLR2スキンにもスキンオプション `BGA Size`（Normal / Extend、既定Normal）を追加した。
  Result選択時のapp側の30/31は、オプションを非表示にしたスキン向けのfallbackとして残す。

手元スキンで入れ子の `#IF` を持つもののうち、Seraphic ResultとWMII_FHD_LR2 WIDEは
内側にも親と同じ条件を書いているため結果は変わらない。
KCOOL 10key（`#IF,32` 内の `#IF,994..998`）とRED BELT Result（`#ELSE` 内の `#IF,948/949`）は、
LR2と同じく内側の条件だけで評価されるようになる。

## 検証

- 3R型の閉じていない入れ子で、BGA SizeのNormal / ExtendによりR-L / R-Rが選ばれる回帰テストを追加した。
  既存の入れ子テストはLR2の判定に合わせて更新した。
- 一時テストで手元スキンを読み込み、3R Result（runtime optionなし）が0件から66件のdestinationになり、
  解像度が1280×720と推定されることを確認した。3R Course Result、KCOOL 10key、RED BELT Result、
  WMII_FHD_LR2 WIDE、Seraphic Resultのdestination数は変更前と同じ。
- `cargo fmt --check`、`cargo clippy -p bmz-skin --all-targets --locked -- -D warnings` は成功。
- `cargo test -p bmz-skin --locked`: 264 passed。
- `cargo test -p bmz-player --locked skin_loader`: 198 passed / 18 ignored、
  `wmii` を含むテスト41件（ignored含む）も成功。外部アセットがない場合に早期returnするテストを含む。
- GPUでの実画面確認は未実施。
