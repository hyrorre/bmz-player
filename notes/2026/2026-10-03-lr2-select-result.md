# LR2 Select / Resultの互換対応

推奨したハイブリッド案を `codex/lr2-select-result-compatibility` で実装した。
第三者スキンのCSV・画像は編集せず、decode、描画、app側操作を接続する。
現在の契約は [skin.md](../../docs/skin.md#lr2のselect--result)、
操作は [controls.md](../../docs/controls.md#lr2選曲スキン) に記載する。

## 実装

- BAR系の固定スロット、バー左下原点、ランプの表示変換、アニメーション、クリック範囲。
- LR2ボタン番号をbeatorajaと分離し、共通設定を意味で変換。未対応の現在値は保持する。
- 基本オプションにLR2のキー配置を使い、ASSIST / DETAIL / F1設定はBMZ機能へ接続。
- 自分側のResultゲージ履歴と累積EXスコア、左右反転、timer 150..152、STARTINPUTの待ち時間。
- LR2の画像 `blend=0` はDST alphaを無視する。WMIXの `a=0` のグラフ指定で確認した。
- CLEAR / FAILEDのロード時分岐、decode cache依存、サンプルResult直接起動時の再ロード。
- 共通画像の兄弟ディレクトリ参照、HDスキンの明示解像度指定、小数座標の読込。

ローカル参照実装の `LR2/Scene02_Songselect.cpp`、`LR2/Scene05_Result.cpp` と
`OpcodeScript/buttonOp.txt` を基準に番号や演出段階を確認した。
LR2に無いBMZ設定の維持・BMZパネル・解像度選択はBMZ側の仕様である。

## アセットを使った検証

追加された以下の8スキンが存在する環境で、CSV変換とアプリの画像decodeを実行した。
外部素材はコミットしない。テストは素材が無い環境ではSKIPを明示する。

| 種別 | スキン |
|---|---|
| Select | LR2、WMIX_HD、Seraphic、ECBE |
| Result | LR2、WMIX_HD、Seraphic |
| Course Result | WMIX_HD |

WMIX_HDは `LR2 Resolution (BMZ) = 1280x720` を指定した。
画像数は順に2 / 5 / 3 / 6 / 1 / 17 / 3 / 6。
別配布の壁紙等の欠落は、主要アトラスや兄弟ディレクトリの参照失敗とは区別する。

最終差分で `cargo fmt --check`、`cargo check --workspace --locked`、
`cargo clippy --workspace --all-targets --locked -- -D warnings`、
`cargo test --workspace --locked --no-fail-fast` が成功した。
`cargo build -p bmz-player --locked` も成功。
Windowsでは別プロファイルでLR2 / WMIX_HDのSelectとサンプルResultを起動し、
スクリーンショットで文字・曲バー・数値・自分側ゲージ曲線・ランクを確認した。
サンプルResultの再ロード、相手側への自分のランクの誤表示、WMIXの透明度指定を修正して回帰テストを追加した。

## 制限と未確認

相手のResult確定値・履歴、過去ベスト／ライバルのスコア曲線、BAR_RANK / BAR_RIVAL、
追加日によるタイトル切替、LR2のFX/EQ、埋込READMEと専用カーソルは未対応。
全カスタムオプションの組合せ、実入力による操作・リトライ、macOS / Linuxの実機表示は未確認。
BMZの保存・IR・コース進行をLR2側の状態機械へ置き換えてはいない。

検証ログとスクリーンショットは `.local/lr2-scenes-*.log`、`.local/lr2-*-smoke.png` に置く。
テスト用の設定・DBは `.local/lr2-smoke-data` に分離した。
