# 数字画像にスキンの filter を適用（#114）

[Issue #114](https://github.com/hyrorre/bmz-player/issues/114) の比較画像では、
`simple-play-simple` の判定コンボ数字の輪郭に差があった。手元のスキンでは
`judge_typeS` と既定の `default2` の両方が `judge.numbers` に `filter = 1` を指定している。

BMZ は判定文字の画像に指定を渡していたが、コンボ数字や通常の `value` の描画では
最近傍を既定値として固定していた。数字描画に destination の指定を渡し、
数値描画キャッシュのキーに含めた。LR2 の各 DST 行にある補間指定は引き続き優先される。
補間の仕様は [スキンの画像拡大縮小](../../docs/skin.md#画像の拡大縮小) を参照。

Windows の wgpu オフスクリーン描画で手元の `simple-play-simple` の
`play7.luaskin` と `judge_typeS` を読み込み、判定の表示先を抽出した。
1600×900、PGREAT のコンボ832、判定から120 msの状態を使った。比較元は同じ
スキンのコンボ数字の `filter` だけをメモリ上で 0 にして旧実装の最近傍を再現した。
3桁の描画位置・大きさは一致し、117×63の切り出しで2,919画素が変化した。
3倍に拡大した比較画像をチャットに添付した。
報告者と同一の配布ファイル・設定での実プレイは未確認。

Windows で `cargo fmt --check`、`cargo check -p bmz-render --locked`、
`cargo clippy -p bmz-render --all-targets --locked -- -D warnings`、
`cargo test -p bmz-render --locked` を実行した。テストは 1,048 成功、29 ignore。
