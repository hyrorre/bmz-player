# 2026-11-28の依存関係更新

`65f83b9b` を起点に、CargoとBunの更新候補をグループごとに検証した。`wgpu` と `egui` はそれぞれ29.0.3と0.34.2に固定した状態で確認した。実行環境はmacOS arm64、Rust 1.100.0、FFmpeg 9.1、Bun 1.4.5、Node.js 26.13.1。

## Cargoの修正版

- 取り下げ済みの `clipboard-win 5.4.1` を5.4.3に更新した。`tokio` 1.54.1、`anyhow` 1.0.105、`memmap2` 0.9.12、`webbrowser` 1.2.7も現行の指定範囲で修正版を固定した。
- 推移依存の `rustls` 0.23.46、`rustls-webpki` 0.103.17、`webpki-roots` 1.0.12、`rustix` 1.1.6、`h2` 0.4.21、`url` 2.5.9と `form_urlencoded` 1.2.3を固定した。
- `cargo fmt --check`、workspaceの型検査・全ターゲットのClippy・テスト、macOS arm64のライセンスレポート生成が成功した。ローカル通信を使う統合テストは一時ポートを許可して検証した。Windows・Linuxの実機確認は未実施。
