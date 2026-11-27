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

workspaceの整形・型検査・全ターゲットのClippy・テストが成功した。macOSの既定出力で48 kHzの無音プローブを実行した。初回の3秒で144,896フレームを処理し、ストリームエラーの通知が1件あった。再確認の5秒では242,176フレームを処理し、通知は0件だった。macOS arm64、Windows互換ターゲット、Linux x86_64のライセンスレポートを生成した。Windows・Linuxのビルドと実機確認は未実施。

## RARと7z

`rars` を0.10.2、`sevenz-rust2` を0.27.0に更新した。RAR5の展開・検証と、7zのヘッダー解析、solidブロック、CRCの修正を取り込む。利用するcodecとBMZ側の書庫検証・資源上限は既存の実装に従う。

workspaceの整形・型検査・全ターゲットのClippy・テストが成功した。既存のフィクスチャでRAR4・RAR5・7zのsolid書庫、収録曲と資源の読み込み、RARのCP932ファイル名を確認した。最終lockfileでWindows互換ターゲットの依存解決と、macOS arm64・Windows互換ターゲット・Linux x86_64のライセンス生成を確認した。`wgpu`、`egui`、`naga`など描画関連17パッケージのバージョンとチェックサム、および `gpu-allocator` が参照する `windows` のバージョンは更新前と一致した。

## 互換性確認で保留した候補

- `reqwest` 0.13.5は `rustls-tls-webpki-roots`、`keyring` 4.2.0は `apple-native` を含む現行のストア選択用featureを使えず、バージョン変更だけでは依存解決が失敗した。採用にはそれぞれTLSの信頼設定、OS別ストアと既存の資格情報の扱いを確認する必要がある。
- `mlua` 0.12.3と `mlua-sys` 0.14.0は、一時コピーでLua 5.4とAPI差分に対応してテストした。ロード時の数値変換を検証する既存テストで `number.fraction` が整数0からエラーに変わったため、0.10系列を維持した。
- TypeScript 7.0.2を単独で組み合わせると `vue-tsc` 3.3.14の起動が `ERR_PACKAGE_PATH_NOT_EXPORTED` で失敗したため、TypeScript 6.0.3を維持した。`nuxt-auth-utils` 0.6系列はセッション秘密鍵の切り替えと再ログインを伴うため、0.5.34を選んだ。
