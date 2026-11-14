# simple-play-bmzの8鍵・9鍵のボム色

`default`と`photonring`は、8鍵に`imageset.ref = 508`、9鍵に`509`を使います。
ボムタイマー`58`・`59`は両鍵に対応していましたが、画像選択側は`507`までしか判定を引けず、PGREATでも通常色を表示していました。

`bmz-render`で`501..=509`を既存のレーン変換に接続しました。[スキン互換の対応範囲](../../docs/skin-compatibility.md)を参照してください。

回帰テストでは、2つのレーンにPGREATとGREATを交互に設定し、通常画像・PGREAT画像・ダミー画像の選択を描画プラン上で確認します。実プレイでの目視確認は未実施です。

Windowsで`cargo fmt --check`、`cargo check -p bmz-render --locked`、`cargo clippy -p bmz-render --all-targets --locked -- -D warnings`が通過しました。
`cargo test -p bmz-render --locked`はテスト用の`TEMP` / `TMP`を指定して1,048件成功、29件ignoreです。
既定の一時パスでは、サンドボックスが返すパスの大文字・小文字の差で既存のフォントテスト3件が失敗しました。
