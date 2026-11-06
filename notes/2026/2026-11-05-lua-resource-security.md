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

## 2026-11-06: 追加レビューの3件

PR #73の`35c260e7`を対象に、次の経路を追加修正した。

- nativeのパターン照合中は命令数hookが動かず、64文字の入力と`a*`8個に末尾`b`を
  組み合わせるだけで外部timeoutまで停止しなかった。4つのパターンAPIをLua実装へ置き換え、
  検索・バックトラック・置換をVMの共有予算に含めた。標準Luaとの結果比較は、空一致、
  キャプチャの巻き戻し、balanced / frontier、byte文字列、関数・table置換、エラーを含む。
- `file:write`が引数を連結した後に上限を確認していた。Lua文字列への参照で先に合計を確認し、
  追記後のサイズも連結前に確認する。4MiB文字列を48引数で渡す再現例のVmHWMは、
  修正前406,980KiBに対し修正後13,192KiBとなり、内容を変更せずnilとエラーを返した。
- 保存量は内容だけを数えており、空ファイル・ディレクトリ・削除記録を増やせた。
  パス・固定管理領域・内容のcapacityを数え、4096bytesのパス長と4096件の上限も設けた。
  書き込み時の集計は保持し、共有保存領域へのmergeも適用前に検証する。

Linux / Rust stable 1.99.1でfmt、check、all-targets Clippyが成功。
bmz-skinテストは402件成功・1件ignored。上記パターン再現例は約266msで命令数上限エラーとなり、
1MiBの名前を80件作る空ファイル・ディレクトリの再現例も最初の作成で拒否した。
再現harnessは`/tmp/bmz-security-followup`に置いた一時データで、Git管理外。
外部スキン素材・実機描画およびプレイヤー全体の検証制約は前回と同じ。

## 2026-11-06: mz-selectの読み込み回帰

`92cb1589`で追加した純Luaパターン実装は、照合結果の比較テストには通ったが、
通常のCSVを既存の命令数予算内で処理できなかった。前回の素材なしテスト結果では
この互換性回帰を検出できていなかった。

- mz-selectの固定commit `a4ee0e1a`を取得し、
  `m_select_lua_select_skin_loads_when_available`が命令数上限で失敗することを再現した。
  `customize/advanced/test_bmsanal/parts.lua`は78,976bytes・1178行の`list.csv`を読み、
  `%s*(.-)%s*,%s*`の`gsub`を行ごとに2回使う。
- パターン処理をLua 5.4.7 `lstrlib.c`由来のC実装へ置き換えた。リンクするLua VMは変更せず、
  `mlua-sys`と同じヘッダーでコンパイルする。MITライセンスと配布物用noticeを同梱した。
- Lua命令数上限は維持し、nativeの探索・比較・コピー・バックトラックに別の共有予算を設けた。
  1024単位を先払いするため、短い呼び出しや途中エラーで計量を免れない。
  保護呼び出し・コルーチン・iteratorの反復にも適用し、フレーム切替後のiterator再開も検証する。
- 80,104bytes・1178行の合成CSVを常時テストし、結果一致とLua命令数・native作業量を確認する。
  実測時間は診断出力に留め、共有CIの速度に依存する合否条件にはしない。
- 必須素材モード`BMZ_REQUIRE_MZ_SELECT=1`と、3 OSでsubmoduleを取得するCIを追加した。
  必須モードでmz-selectが無い場合は、早期returnで成功扱いにしない。

Linux / Rust stable 1.99.1 / C最適化O2で、実CSVの同じ分割処理はstock Lua約41ms、
計量版約64ms、Lua命令数87,000・native作業量11,572,224だった（単発の参考値）。
Lua側の1600万命令上限を維持した状態でmz-selectの実読み込みも成功した。
Rmz-skin `cb08901c`、mz-select `a4ee0e1a`、Luxez-Flat `9e0d42e4`を取得した全テストは
406件成功・2件ignored（実CSV計測は別途明示実行して成功）。fmt・check・all-targets Clippyも成功。
Windows / macOSは追加CIでの確認待ち。GUIの実機描画とプレイヤー全体のビルドは未確認。
