# Lua sandboxの資源制限

対象は`fd94c209`（PR #71反映済み）。Linux環境で、Luaの命令数制限とRust側の
メモリ確保を確認した。仕様は[skin.md](../../docs/skin.md)に反映した。

- コルーチンではmluaのhookが外れる経路があり、保護呼び出しでは上限エラーを捕捉できた。
  再開時にhookを切り替え、共有予算を消費する。短いyieldの反復も最低1間隔分を消費する。
- `pcall` / `xpcall`は通常のエラー・多値・yieldを保持し、予算切れを呼び出し境界から伝播する。
  `xpcall`のhandlerは巻き戻し後に実行する。hookが無効になるGC finalizerの登録は禁止した。
- Lua allocatorの上限に含まれない読み込みバッファにVM合計32MiB、文書変換の文字列コピーに
  合計64MiBの上限を設けた。閉じたファイルはバッファを直ちに解放する。

検証はRust stable 1.99.1で`cargo fmt --check`、`cargo check -p bmz-skin --locked`、
`cargo clippy -p bmz-skin --all-targets --locked -- -D warnings`、
`cargo test -p bmz-skin --locked`を使用し、396件成功・1件ignored。
check・Clippy・fmtも成功した。無限ループのテストには期限を設けた。
外部スキンsubmoduleは未初期化で、外部素材による互換・実機描画確認は未実施。
プレイヤーのビルドはシステム開発ライブラリ不足により未確認。
