# IR Webのdev SSRでi18nキーがそのまま出る問題

2026-10-22に、Node.js 24から26への移行の影響を調べる途中で見つけた。基点は `a7615105`。
IR Webの構成と検証コマンドは [IR](../../docs/ir.md) を参照する。

## 症状

- `bun run dev` のSSR HTMLが、翻訳済みの文字列ではなくi18nのキーを出していた。
  例: `<title>home.title - BMZ IR</title>`、`nav.login`、`home.description`。全localeで同じだった。
- hydration後はクライアント側で翻訳されるため、ブラウザでは正しく見える。
  ただしVueの `Hydration text mismatch` 警告が大量に出ていた。
- Node 24.22.0と26.12.0の両方で再現したので、Nodeのバージョンとは関係ない。
- 本番相当の環境（build後の `wrangler dev`、workerd）では再現しなかった。

## 原因

- dev SSRの `@nuxtjs/i18n` 10.4.1は、`localeLoaders` から各localeのファイルを直接importする（`loadMessagesFromClient`）。
  本番はNitroの `/_i18n/<hash>/<locale>/messages.json` を `$fetch` するので、読み込みの経路が違う。
- `@intlify/unplugin-vue-i18n` 11.2.4は、Vite 8（rolldown）でlocaleのファイルを `virtual:intlify-i18n-*` の仮想モジュールに
  `resolveId` し、`load` でファイルを直接読んで `@intlify/bundle-utils` に渡していた。
- この経路は、`@nuxtjs/i18n` のresource pluginが `defineI18nLocale(...)` を外すtransformを通らない。
  `i18n/locales/*.ts` の `export default defineI18nLocale(...)` は関数呼び出しの式なので、
  bundle-utilsの判定（object、function、arrow functionだけを受け付ける）に合わない。
  その結果 `You need to define 'export default' that will return the locale messages.` で失敗し、
  サーバーログに `Failed to load messages for locale "ja"` が出ていた（jaのfallback先のenで止まる）。
- クライアント側の読み込みが同じ形で失敗しなかった理由は、まだ確かめていない。

## 対応

- `bun update @intlify/unplugin-vue-i18n` で、lockの間接依存を11.2.7に更新した。
  `@nuxtjs/i18n` の指定範囲（`^11.2.4`）の中での更新で、`package.json` は変えていない。
- 11.2.7ではVite 8の仮想IDが廃止され（[intlify/bundle-tools#638](https://github.com/intlify/bundle-tools/pull/638)）、
  localeのファイルは通常のtransformを通るようになった。
- lockの差分:
  - `@intlify/unplugin-vue-i18n` と `@intlify/bundle-utils` を11.2.4から11.2.7に更新した。
  - build時に使う `@intlify/shared` と `@intlify/message-compiler` の11.4.16を入れ子で追加した。
  - 使われなくなった `escodegen` と `esprima` を削除した。
  - runtimeの `vue-i18n` 11.4.6は変わらない。
- 回避策としてlocaleのファイルから `defineI18nLocale` を外す方法もあるが、推奨の書き方から外れるので採らなかった。

## 検証

macOSのNode 26.12.0で確認した。

- dev: `/`、`/en`、`/ko`、`/zh-HK`、`/charts`、`/courses`、`/login`、`/manual` のSSR HTMLに翻訳済みの文字列が出て、キーは残っていなかった。
  サーバーログに読み込みの失敗やキー未検出の警告はなく、新しいタブで開いたページにもhydration mismatchは出なかった。
- 本番相当: `bun run build` の後、`wrangler dev --config .output/server/wrangler.json` を空の分離したstateで起動し、
  `/`、`/en`、`/ko`、`/charts`、`/login` を確認した。Web依存ライセンスのレポートは更新前と同じだった。
- `bunx vue-tsc --noEmit`、`bun run test:ir`、`bun run test:i18n` は成功した。
- 自動テストは追加していない。再現にはdevサーバーのVite SSRを起動する必要があるため。
- 本番Workersへのdeploy後の確認はしていない。本番はもともとこの問題の影響を受けていない。
