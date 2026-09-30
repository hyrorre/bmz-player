# Issue #23: スキンテキスト装飾の透明度

## 原因と修正

`text_render_item_with_draw_state()`では解決済みdestinationのαを本体に設定していたが、
outline / shadowには装飾色自身のαだけを渡していた。本番の`CachedTextFrameBuilder`は
各色で描画するため、本体が透明でも装飾が残っていた。

変換時に`A = clamp(frame.a, 0, 255) / 255`を使い、ref=30では検索文字の透明度も乗算する。
本体とキャレットには既存どおりAを設定し、縁取り・影の固有αだけにAを乗算する。
元の定義や共有スタイルは変更せず、フレームごとに作る描画用データへ適用する。
後段の`with_alpha()`と`apply_draw_command_alpha()`は各αへFを一度掛ける既存処理を維持する。
RGB、装飾寸法、配置、入力、Lua callbackの評価タイミングは変更しない。

## beatorajaとの区別

報告者が確認したコミットは`ad42f56c4658e968f93b24bf23440fe51cb9878e`。
通常のSkinTextFont / SkinTextBitmapは影に本体のcolor.aを使うが、距離フィールド方式は
装飾色をそのままshaderへ渡し、全装飾へ本体αを一律乗算していないという比較結果が提示された。
今回の修正はBMZの共通透明度方針であり、全方式のbeatoraja互換と位置付けない。
ローカル参照のHEADは`721856fbb4311241ea81711e0574e5cc094c13c2`で、提示コミットは未取得。
この参照でも通常フォントの影のcolor.aとdistance_field.fragの装飾αの独立性を確認した。

## 回帰検証

- JSON destination: 完全透明、半透明、固有α、省略時の不透明、αのclamp、
  255→128→0→255の復帰、アニメーション途中値、offset解決後のα、検索文字とキャレット。
- 既存judgeColor / judgeTimingColorテスト、後段全体フェードの各αへの一度だけの適用。
- ベクター・ビットマップの本番CachedTextFrameBuilderで生成quadの色と再表示時の復帰を確認。
  装飾の再帰描画はαを含むlayout keyを使用するため、前フレームのαを再利用しない。
  ベクターフォントが無ければ失敗するテストとし、素材不足の早期returnを避けた。
- Luaの最小fixtureを既存loaderでSkinDocumentへ変換し、destinationと固有色αの保持を確認。
  Luaも同じSkinDocument描画変換へ到達する。Lua変換実装の変更はない。
- `cargo fmt --all -- --check`: 成功。
- `cargo check -p bmz-render --locked`: 成功。
- `cargo clippy -p bmz-render --all-targets --locked -- -D warnings`: 成功。
- `cargo test -p bmz-render --lib --locked`: 655成功、3 ignored。
- `cargo test -p bmz-skin --lib lua_decorated_text --locked`: 1成功。
- `cargo clippy -p bmz-skin --all-targets --locked -- -D warnings`: 成功。

ログはGit管理外の`.local/text-opacity-*.log`へ保存した。
GPU実機での見た目は未確認。既存のignoredテストも実施していない。

## 手動確認

outlineWidth > 0、outlineColor / shadowColorを持つ非空テキストをJSONまたはLuaスキンで表示する。
destinationのaを255 / 128 / 0に切り替え、本体・縁取り・影が一緒に薄くなり、0で消えることを確認する。
装飾色自身のαを128等にして、RGBや幅・offsetが変わらず固有透明度が保たれることも確認する。
destinationに255→0→255のkeyframeを指定し、フェードアウト・再表示と画面全体フェードを確認する。
同じ手順をベクターとビットマップフォントで行い、検索欄のplaceholder・入力キャレットも確認する。

仕様は[skin.md](../../docs/skin.md#text-opacity)に記載した。
