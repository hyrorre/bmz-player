# ResultパネルとLua状態の同期

## 症状と原因

Luxe FlatのResultでIR RANKINGへ切り替えると、GRAPH DATAの枠・ゲージ・密度グラフが
残り、ランキングの名前・スコアが重なった。手元の実スキンを現在のBMZで読み込むと、
単純な `result_mode == 1` は `result_panel(1)` に変換される一方、配置設定や
`pattern_mode`、ランキング行番号を含む条件はLua runtime callbackへ残っていた。
BMZ側の切替はLuaの `result_mode` に伝わらず、初期値GRAPHのまま評価されていた。
該当Lua callbackの実行エラーは0件だった。

WMIIの比較対象の枠・グラフは `Expand_op` からパネル条件へ変換されていたため、
通常モードでは同じ混在を起こさなかった。ただしruntimeに残る条件には同じ同期が必要。

## 変更

- `LuaMainState::result_panel` にappの描画状態を渡す。未指定の既存利用側では同期しない。
- runtime VMで、認識済みの `Expand_op` とcallbackが直接保持する `result_mode` を
  評価中に同期する。共有upvalueは同一性で重複を除き、元のclosureを保持する。
- 通常のcallback呼出しとrendererの共有評価scopeの双方へ適用する。
  入れ子やpanic後は元のパネル値へ戻し、他の可変localは維持する。
- compatでローカルパネルの初期値を記録し、runtime drawのみのパネルでも
  appの既存キー・タブ操作を有効にする。キー割当自体は変更しない。
- 外部スキン、設定、DBは変更しない。

仕様は [skin.md](../../docs/skin.md)、対応範囲は
[skin-compatibility.md](../../docs/skin-compatibility.md) を参照。

## 検証

- Lua単体: 異なるローカルupvalueとグローバルの同期、GRAPH/IR/非表示への切替、
  入れ子scope・panicでの復元、他のclosure stateの継続、compatの初期値認識。
- app側: パネル操作可否、実スキン由来の枠・グラフ・タブ・IR文字・順位装飾の
  draw/op条件をテキストmarkerへ載せ、実際のrendererで表示/非表示を判定する。
  同一VMでGRAPH→IR→IR→GRAPH→非表示→IR→GRAPHを評価し、通常／compatと左右配置を確認。
- 手元にLuxe FlatとWMIIの両アセットが存在する。画像・フォント・GPUの可用性で
  条件不具合を隠さないためmarkerを使うが、実画面の見た目の確認とは区別する。
- `cargo fmt --all -- --check`、`cargo check --workspace --locked`、
  `cargo clippy --workspace --all-targets --locked -- -D warnings` は成功。
- `cargo test --workspace --locked --no-fail-fast` は3,592 passed / 0 failed / 14 ignored。
  このうちbmz-playerは2,104 passed / 6 ignored、bmz-skinは232 passed、
  bmz-renderは655 passed / 3 ignored。上記の追加回帰テストも成功。
- `git diff --check` と変更文書のローカルリンク確認は成功。
  全テストの生ログは `.local/result-panel-workspace-tests.log`（Git管理外）。
- GPUを使う実画面操作、macOS/Linux実機確認は未実施。
