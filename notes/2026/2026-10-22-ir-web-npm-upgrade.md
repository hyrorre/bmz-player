# IR Webのnpmパッケージ更新（Nuxt 4.6）

2026-10-22に、IR Webの直接依存とoverridesの更新を調べ、まとめて更新した。基点は `a6250629`。
Node.js 26への移行の調査は [別の記録](2026-10-22-ir-web-node26.md) にある。
IR Webの構成と検証コマンドは [IR](../../docs/ir.md) を参照する。

## 更新したもの

| パッケージ | 更新前 | 更新後 | 主な変更・互換性 |
|---|---|---|---|
| nuxt | 4.4.8 | 4.6.2 | Vite 8を正式に採用し、unhead v3・unctx v3・Nuxt CLI v4に移った。Nodeの要件は `^22.22.3 \|\| ^24.15.0 \|\| >=26.0.0` |
| @nuxt/ui | 4.8.2（完全固定） | ^4.12.0 | Nuxt ≥4.6.1が必要。破壊的変更の記載はない。アイコンがbuildに埋め込まれるようになった。`UApp` はi18nのlocaleに自動で追従するが、`app.vue` は明示したlocaleを渡しているので変わらない |
| @nuxtjs/i18n | 10.4.1 | 10.6.0 | Nuxt ≥4.5.1とunhead v3が必要。メッセージの束ね方の最適化はJSON/YAMLが対象で、TSのlocaleは従来どおり |
| nuxt-auth-utils | 0.5.29 | 0.5.32 | 0.5.31でNuxt 4.6に対応した。OAuthのadvisoryは、OAuthを使っていないので該当しない |
| @nuxthub/core | 0.10.7 | 0.10.10 | blobのパス名を検証するようになった。chokidar ^5が必要 |
| marked | 17.0.6 | 18.1.1 | メジャー更新。ブロックトークン末尾の空行を削るようになった。マニュアル全48ページのHTMLは更新前と同じだった |
| @libsql/client | 0.17.3 | 0.18.0 | sqlite3の接続プールが変わった。使っているのはテストだけ |
| wrangler | 4.100.0 | 4.151.3 | miniflare 5のbeta版に依存する。`db:migrate`、dev、dry-runは動いた |
| drizzle-orm / drizzle-kit、vue-tsc、@types/bun、@nuxt/eslint、@iconify-json/lucide | | patch / minor | 関係する変更はない |

overridesは次のように整理した。

- `vite ^8.0.0` は削除した。Nuxt 4.5以降は自分でVite 8を使う。
- `chokidar 3.6.0` は削除した。Nuxt 4.6とNuxtHubはchokidar ^5を要求する。
  固定した理由は記録になかった。macOSのdevで、ファイルの変更と新しいページの追加が反映されることを確認した。
- `vue ^3.6.0-beta.15` は削除し、stableの3.5.44に戻した。Vapor Modeは使っていない。
  Nuxt自身も `vue ^3.5.44` に依存している。3.6.0-rc.12でも型検査・テスト・build・SSRが通ることは確認した。
- `typescript ^6.0.3` をdevDependenciesに追加した。指定しないとTypeScript 7.0が解決され、
  `vue-tsc` が `typescript/lib/tsc` を読み込めずに失敗する。typescript-eslintも `<6.1` を要求している。

## セキュリティ

- `bun audit` は94件（critical 6、high 49）から4件（high 2、moderate 1、low 1）に減った。
  残りは `braces`、`node-forge`、`esbuild` などbuild・devツールの間接依存で、範囲内に修正版がない。
- 開発機: `@nuxt/devtools` 3.2.4のRPCには認証がなく、devサーバーの起動中に悪意のあるWebサイトを開くと、
  任意のコマンドを実行されるおそれがあった（GHSA-279x-mwfv-vcqv）。3.4.4になって解消した。
- 本番のWorker: 該当するのはpayloadのシリアライズに使う `devalue` 5.8.1だけだった。Nuxt 4.6.2で6.0.3になった。
- Nuxtのisland系のadvisory（RCE、CPU消費など）は該当しない。island componentがないので、
  `/__nuxt_island/**` は更新前から空のハンドラーで204を返すだけだった。route rules系も、`routeRules` を使っていないので該当しない。

## 注意点

- 差分インストールのままbuildすると、lockにない古い `vaul-vue/node_modules/reka-ui` 2.9.8が残り、
  Workerのbundleに入った。`node_modules` を消して `bun install --frozen-lockfile` し直すと、lockどおりになった。
- Prettier 3.9は4ファイルの整形を変えるため、別のcommitで更新した。

## 検証

macOSのNode 26.12.0で確認した。Web用のファイルだけをscratchpadに取り出したコピーで一括更新を試し、
同じlockを本体に適用した。

- `bunx vue-tsc --noEmit`、`bun run test:ir`（105件）、`bun run test:i18n` は成功した。
- `bun run build` は成功し、Web依存ライセンスの判定も通った（74パッケージ、MIT / Apache-2.0）。
  Workerのuploadサイズは3422 KiBから3411 KiBになった。
- devの確認:
  - 主要ページ、6言語のSSR、マニュアル、譜面詳細、APIが表示・応答した。ブラウザに警告やエラーは出なかった。
  - コピーでは、分離したD1で `db:migrate`、使い捨てのアカウントでの登録、cookie・Bearer認証、ログアウトを確認した。
  - ファイルの変更が反映されることも確認した。
- workerd: コピーのbuildを `wrangler dev` で動かし、SSR、API、認証、islandのendpointを確認した。
- ESLintの結果は更新前後とも同じ53件（既存）だった。
- 確認していないこと:
  - 本番へのdeployとremote DB
  - Windows / Linuxでのファイル監視
  - BMZ Playerの実クライアントからのスコア送信とリプレイのアップロード
