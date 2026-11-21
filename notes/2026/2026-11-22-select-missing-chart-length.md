# 選曲の未ダウンロード譜面での譜面長 Lua 参照

[Discussion #81](https://github.com/hyrorre/bmz-player/discussions/81) で、選曲中に未ダウンロード譜面へカーソルを移すと Lua の `main_state.number(1164)` を整形したテキストに `-2147483648` が出ると報告された。

v0.5.1 の `e45f62f47` で追加した譜面長の欠損値が選曲にも適用されていた。beatoraja（`83574048`）は難易度表から作った未所持の `SongData` の譜面長を `0` のまま保持する。BMZ でも選曲の未所持譜面行に限り、`1163` と `1164` の Lua 参照と数値式に `0` を返すようにした。

[スキン仕様](../../docs/skin.md)に挙動を記載した。回帰テストではローカルの譜面から未所持譜面、フォルダへ選択を切り替え、Lua の値、数値式、数値画像と `numberRef` の非表示を確認する。

Windowsで `cargo fmt --check`、`bmz-render` の `cargo check` と全ターゲット Clippy、全テストを実行した。テストは1073件成功、29件 ignored。既定の一時ディレクトリではパスの大文字・小文字の違いで既存のビットマップフォントテスト3件が失敗したため、`TMP` と `TEMP` を worktree 内の `.local/test-temp` に指定して再実行した。実機のスキン表示は未確認。

## 2026-11-22 追記　`text.numberRef` の桁指定

[Issue #21 のコメント](https://github.com/hyrorre/bmz-player/issues/21#issuecomment-5922122192) では `string.format("%02d", ...)` を案内していた。フォント描画でも数値スプライトと同じフィールドを使えるよう、`text.numberRef` に `digit`、`zeropadding`、`padding` を追加した。

数値スプライトとパディング指定の解釈を共有する。テキストの `digit` は符号を含む最小文字数で、最大64。回帰テストでは JSON と Lua の読み込み、ゼロ埋めと空白埋め、負数、幅を超える値、未所持譜面での空欄を確認する。

Windowsで `cargo fmt --check`、ワークスペースの `cargo check` と全ターゲット Clippy が通過した。関連3クレートのテストは `bmz-render` 1074件、`bmz-skin` 432件、`bmz-skin-document` 40件が成功した。

ワークスペースの全テストでは Windows のループバック接続が拒否され、ジャンクション作成も権限エラーになった。ローカルのテストサーバーを待つテストが進まなくなり実行を中断した。実機のスキン表示は未確認。

固定コミットの Luxe Flat、Rmz-skin、mz-select を展開して再検証した。`BMZ_REQUIRE_MZ_SELECT=1` での `bmz-skin` 全テストは432件成功。ワークスペースの関連テストはプレイヤー307件、描画6件、Lua9件が成功した。Starseeker など別途用意する外部スキンのテストは素材なしで早期終了した。
