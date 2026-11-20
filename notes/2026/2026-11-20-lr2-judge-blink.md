# REMI-S の GREAT／GOOD 点滅と LR2 の画像分割

REMI-S (hachimi) v1.1 FHD の 7KEY で「GREAT以下の判定点滅」が ON のとき、GREAT の判定文字が上下半分ずつ引き伸ばされ、GREAT／GOOD のコンボ数字は消灯時に黒い縦筋になった。

判定文字の GREAT と両判定のコンボ数字は、高さ 140 px の `DEFAULT.PNG` に対し、y=105 から高さ 70 px を 2 コマに分割する。先に全体を切り詰めると GREAT の 35 px の文字が二分割される。コンボ数字は消灯コマの y=140 以降で最下段を繰り返しサンプリングしていた。GOOD の判定文字には範囲内に透明な消灯コマがある。

LR2 の画像と数字は、セルを割り出してから画像の範囲で切り詰める。範囲外のセルはそのコマの間だけ描画を省く。判定文字とコンボ数字のセルは個別に評価する。beatoraja 互換の画像と数字は従来の UV 計算を通る。詳細は [LR2 の Play 描画](../../docs/skin.md#lr2のplay描画)を参照。

Windows／DX12 のオフスクリーン描画で、実際の 7KEY の CSV と `DEFAULT.PNG` に GREAT／GOOD を与えて確認した。点滅 ON の 0、37、75 ms では、文字と数字のピクセルが点滅 OFF と一致した。38、74 ms では、両方の領域が背景と一致した。実プレイと他の GPU バックエンドは未確認。

回帰テストには同じアトラス配置を使い、LR2 の範囲外セルの非表示と、beatoraja 互換経路が従来の UV を返すことを確認した。描画ログと画像は Git 管理外の `.local/remi-s-judge-fixed*` に保存した。

Windows で `cargo fmt --all --check`、`bmz-render` の check・all-targets Clippy・test、`bmz-player` ライブラリの release build を確認した。`bmz-render` のテストはサンドボックスの一時パスの大文字小文字が一致せずフォントの 3 件が失敗したため、`TEMP` と `TMP` をリポジトリ内に指定して再実行し、1071 件成功、29 件の既定の無視を確認した。
