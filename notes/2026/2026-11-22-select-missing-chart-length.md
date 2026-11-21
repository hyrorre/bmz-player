# 選曲の未ダウンロード譜面での譜面長 Lua 参照

[Discussion #81](https://github.com/hyrorre/bmz-player/discussions/81) で、選曲中に未ダウンロード譜面へカーソルを移すと Lua の `main_state.number(1164)` を整形したテキストに `-2147483648` が出ると報告された。

v0.5.1 の `e45f62f47` で追加した譜面長の欠損値が選曲にも適用されていた。beatoraja（`83574048`）は難易度表から作った未所持の `SongData` の譜面長を `0` のまま保持する。BMZ でも選曲の未所持譜面行に限り、`1163` と `1164` の Lua 参照と数値式に `0` を返すようにした。

[スキン仕様](../../docs/skin.md)に挙動を記載した。回帰テストではローカルの譜面から未所持譜面、フォルダへ選択を切り替え、Lua の値、数値式、数値画像と `numberRef` の非表示を確認する。

Windowsで `cargo fmt --check`、`bmz-render` の `cargo check` と全ターゲット Clippy、全テストを実行した。テストは1073件成功、29件 ignored。既定の一時ディレクトリではパスの大文字・小文字の違いで既存のビットマップフォントテスト3件が失敗したため、`TMP` と `TEMP` を worktree 内の `.local/test-temp` に指定して再実行した。実機のスキン表示は未確認。
