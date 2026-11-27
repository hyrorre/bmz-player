# 2026-11-28の依存関係更新

`65f83b9b` を起点に、CargoとBunの更新候補をグループごとに検証した。`wgpu` と `egui` はそれぞれ29.0.3と0.34.2に固定した状態で確認した。実行環境はmacOS arm64、Rust 1.100.0、FFmpeg 9.1、Bun 1.4.5、Node.js 26.13.1。

## Cargoの修正版

- 取り下げ済みの `clipboard-win 5.4.1` を5.4.3に更新した。`tokio` 1.54.1、`anyhow` 1.0.105、`memmap2` 0.9.12、`webbrowser` 1.2.7も現行の指定範囲で修正版を固定した。
- 推移依存の `rustls` 0.23.46、`rustls-webpki` 0.103.17、`webpki-roots` 1.0.12、`rustix` 1.1.6、`h2` 0.4.21、`url` 2.5.9と `form_urlencoded` 1.2.3を固定した。
- `cargo fmt --check`、workspaceの型検査・全ターゲットのClippy・テスト、macOS arm64のライセンスレポート生成が成功した。ローカル通信を使う統合テストは一時ポートを許可して検証した。Windows・Linuxの実機確認は未実施。

## IR Webの依存

- Nuxt 4.7.0、Nuxt UI 4.13.1、NuxtHub 0.10.13、Nuxt i18n 10.7.0、Tailwind 4.3.5、marked 18.3.0、Wrangler 4.169.0に更新した。Bun型定義と同じBun 1.4.5、`vue-tsc` 3.3.14、Lucideのアイコンデータ1.2.160で検証した。
- `nuxt-auth-utils` は既存のセッション設定と互換性がある0.5.34を選んだ。型検査は既存の `vue-tsc` が使用するTypeScript 6.0.3で行った。
- 一時コピーで固定lockfileによるインストール、Prettier、`vue-tsc --noEmit`、IRテスト272件、`bun run build` が成功した。ビルドはWranglerのdry-runと78パッケージの配布用ライセンスレポート生成まで実行した。

## Prettier

固定版を3.9.10から3.10.1へ更新した。既存の `.prettierignore` を使ったIR Webの整形検査が通り、整形差分は発生しなかった。一時コピーで `vue-tsc --noEmit` とIRテスト272件も成功した。
