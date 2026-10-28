# 曲スキャンの書庫世代と重複root

PR #49 のレビューで、書庫の探索後の更新により実在する譜面の登録が削除されることと、
重なったrootで片方のjunction経由の登録を別のrootが削除することをWindowsで再現した。
Windowsのstrict ClippyではUnix専用テストのhelperが未使用となった。

探索時の書庫世代を取り込みと削除直前に照合し、一致しない場合は削除と完了時刻の更新を見送る。
複数rootの発見結果を共有して全取り込み後に削除し、未走査・無効・不完全な別rootの範囲を保護する。
mainの隠し属性判定とリンク探索を統合し、隠し設定によって既存の実在譜面が消えないことを維持した。
現在の仕様は [曲source](../../docs/song-sources.md) を参照。

Windowsで `cargo check -p bmz-player --locked`、strict Clippy、`cargo test -p bmz-player --locked` を実行。
testは2705件通過、44件ignore（別に起動テスト2件ignore）。共有targetから実行するため、
`BMZ_RESOURCE_DIR` にworktreeの `data` を指定した。junction、書庫の探索後・取り込み後の更新、
重複rootの順序・部分走査・利用不能、Windowsの隠し属性をテストした。
独立したdataを使うサンプル起動の3フレームsmokeは、サンドボックス外で正常終了した。
サンドボックス内では修正前後ともgilrsのWindowsゲームパッドAPIがpanicした。

Windowsのdebug buildでPR元 `998171da` とmain取り込み・修正後を比較した。400フォルダ、通常譜面400、
各フォルダに素材20、書庫40、別々のDB、キャッシュ済み再走査を交互に8回ずつ計測した。
scan全体の中央値は76 msと93 ms、探索の中央値は43 msと43.5 ms。後者にはmainの隠し属性対応も含み、
この比較だけでは17 msの差の内訳を特定できない。初回の計測は順序とcacheが揃わないため比較に使わない。
実利用ライブラリ、Everything、Linux/macOSの実動作とrelease buildの性能は未確認。
