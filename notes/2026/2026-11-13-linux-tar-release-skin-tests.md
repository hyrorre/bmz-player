# v0.5.2のLinux tarリリースで失敗した同梱スキンテスト

[v0.5.2のLinux tarジョブ](https://github.com/hyrorre/bmz-player/actions/runs/43432176767/job/131908371479)は、
`183693dc`の`bmz-player`の単体テスト3件が失敗して停止した。
[Cargo 1.100.0のビルドディレクトリ変更](https://doc.rust-lang.org/nightly/cargo/reference/build-cache.html)で、
テスト実行ファイルが`target/debug/build/bmz-player/<hash>/out/`に置かれる。
従来のテストは実行時のパス解決に依存しており、同梱スキンを実行ファイルの隣にある`resources`へ探しに行った。

テストではソースツリーの素材を直接参照し、レンダラーへ取り込むテストには既存のテスト用`AppPaths`を渡す。
同じ経路を使う外部スキンのテストでも、同梱素材の参照先は明示する。
同梱のPNG・JSONの存在と、プレイスキンのレンダラーへの登録を検査する。

macOS arm64のCargo 1.100.0で、修正前にCIと同じ3件の失敗を再現し、修正後に3件とも成功した。
`cargo fmt --check`、`bmz-player`の`cargo check`とall-targets Clippy、Linux tar用のPythonテスト30件が成功した。
macOSの`bmz-player`全テストでは、今回の変更箇所と独立した書庫内動画のテスト1件で、
白フレームのRGB値が期待値255に対して253となった。同じテストは元のLinux tarジョブで成功している。
パッケージのビルドと配布物の検証手順は[Linux tar.gz配布](../../docs/linux-tar.md)を参照。
