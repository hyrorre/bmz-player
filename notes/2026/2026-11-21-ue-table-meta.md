# UE 難易度表の HTML からヘッダー URL を取得できない問題（#145）

[Issue #145](https://github.com/hyrorre/bmz-player/issues/145) では、Windows インストーラーで導入した環境で 4UE、6UE、8UE の難易度表が表示されず、URL からの取得も失敗すると報告されました。

3 表とも既定の `table.html` と、そこから参照される `header.json`、譜面一覧の `score.json` は HTTP 200 を返しました。
公開 HTML では画面表示用の `viewport` とヘッダーを指定する `bmstable` が同じ行にあります。
修正前の Windows ビルドでは、`bmstable` がある行の最初の `content` を読み、`width=device-width, initial-scale=1` をヘッダーのパスとして取得して HTTP 404 になりました。

HTML の行単位での検索をタグ単位に変え、`name="bmstable"` を持つ `meta` タグの `content` を読みます。
既定の URL は引き続き公式の `table.html` を使用します。修正前の版でも、[難易度表の設定](../../docs/settings.md#難易度表)で各表の `header.json` を直接登録できます。

新規環境では起動時に未取得の表を取得します。ポータブル版で表が表示された環境では、以前の取得結果が保存されていた可能性があります。
報告者の導入版と保存データは未確認です。

## 検証

- `viewport` と `bmstable` が同じ行にある HTML の回帰テストを追加しました。難易度表のテスト 19 件が通過しました。
- Windows の修正版で、BMZ メニューと同じ取得処理を通る `table fetch` を空のライブラリに対して実行しました。既定の `table.html` から 4UE 1,170 譜面、6UE 1,544 譜面と 21 コース、8UE 199 譜面を取得し、`table list` で 3 表の保存を確認しました。
- 修正前のビルドでも 3 表の `header.json` を直接指定すると取得できました。メニューの手動操作と配布インストーラーでの再検証は未実施です。
- `cargo fmt --check`、`cargo check -p bmz-player --locked`、`cargo clippy -p bmz-player --all-targets --locked -- -D warnings` は通過しました。
- `cargo test -p bmz-player --locked` のライブラリテストは 3,384 件通過、94 件が既定で対象外、1 件が失敗しました。CJK ファイル名の ZIP を使う既存の書庫テストで期待より 1 回多く書庫を開く失敗が、修正前のビルドで書庫テスト群を実行したときも再現しました。該当テストを単独で実行すると修正前後とも通過しました。
