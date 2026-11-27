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

## SQLite

`rusqlite` 0.43.0 と `libsqlite3-sys` 0.40.0に更新し、同梱SQLiteを3.54.1にした。`fallible_uint` を有効にして、スコアDBの `u64` を従来どおりSQLiteの `INTEGER` に変換する。DBスキーマの移行は不要。

workspaceの整形・型検査・ClippyとmacOS arm64のライセンスレポート生成が成功した。workspaceテストではDBのmigration、スコア保存、外部スコアDBの取り込みを含む `bmz-player` の3508件が成功した。`bmz-video` の再開時刻テスト1件は断続的に失敗した。SQLite更新前後で同じテスト実行ファイルを使用しており、そのファイルを20回再実行すると19回成功、1回失敗した。

## ZIP

曲書庫、スキン資源、Windowsアップデータの `zip` を9.8.2に更新した。曲書庫は生の名前とUTF-8フラグを使う既存の名前解決を維持する。公開された名前取得APIに合わせ、スキン資源とアップデータはデコードエラーを呼び出し側へ返す。

workspaceの整形・型検査・全ターゲットのClippy・テスト、macOS arm64のライセンスレポート生成が成功した。CP932の曲ZIP、ZIP内のスキン参照、アップデータの新旧パッケージ構成をテストで確認した。Windows上での実アップデートは未実施。

## CPAL

`cpal` を0.18.2、Windows向けの `asio-sys` を0.4.0に更新した。Windows互換ターゲットでのASIOの依存解決と、SDKを `CPAL_ASIO_DIR` で渡す既存の配布手順との整合を確認した。

workspaceの整形・型検査・全ターゲットのClippy・テストが成功した。macOSの既定出力で48 kHz・3秒の無音プローブを実行し、144,896フレームを処理した。macOS arm64、Windows互換ターゲット、Linux x86_64のライセンスレポートを生成した。Windows・Linuxのビルドと実機確認は未実施。
