# Luaスキンのdocument変換の高速化

2026-11-10〜11に、Luaスキンのロード時間のうちBMZ側の変換処理を調べて短縮した記録です。
画面入場ごとの読み直しと先読みは[Result・Selectスキンの先読み](2026-11-10-result-skin-prewarm.md)を参照してください。

## 経緯

- SocialSkinのDecide（`decide.lua`、アニメーションタイプ1）は入場ごとに約110msかかっていた。
  ロード時に`math.random`でパーティクル3000個を配置し（1個につきdestination 2件・各4キーフレーム）、
  文字を6px幅の235本に分けて4種類ずつ描くため、約3850 destination・948 image、
  Luaの表で約2万4千個・フィールド約10万個のdocumentになる。ロード時乱数のためdocument cacheも効かない。

## 計測条件

- Windows、release build。一時的なプローブで`decode_beatoraja_skin_request`を7回実行し、
  `document_us`の中央値を比べた。変更前と変更後のbinaryを交互に各2回実行した。
- SocialSkinは利用者のスキン設定、他は既定のオプション。段階ごとの時間は一時的なタイマーで測った。
  プローブ・タイマー・生ログはコミットしない。

## 原因

変更前のSocialSkin Decide 1回（約108ms）の内訳:

| 段階 | 時間 |
|---|---|
| sandbox・headerの準備 | 約4ms |
| `decide.lua`の本体実行 | 約19ms（`math.random`の9070回は合計1.7ms） |
| Luaの表からJSONへの変換（`lua_value_to_json`） | 約51〜57ms |
| 整数の正規化 | 約9ms |
| `SkinDocument`へのdecode（`serde_path_to_error`） | 約21ms |

- 変換の内訳は、`pairs`の走査約13ms、キーの並べ替え約6ms、キーの文字列化と予算計算約10ms、
  関数推論用のmetadata（`id`の検索）約5msで、残りもフィールドごとの処理に分散していた。
- mluaは`send` featureのため、`mlua::String::as_bytes`等の呼び出しごとにVMのlockを取る。
  並べ替えの比較、予算計算、文字列化、`id`の検索でキーごとに何度もlockと複製をしていた。
- 正規化は全てのobjectを新しいmapへ作り直していた。decodeはfield pathの追跡で遅く、
  追跡なしでは値を消費するdecodeが15.7ms、採用した参照からのdecodeが約18msだった。

## 変更

- 表のキーを読み取り時に1度だけバイト列へ写し（`LuaTableKey`）、並べ替え・文字列化・予算計算・
  `id` / `dst`の検索で使い回す。文字列値もバイト列を1度だけ読む。並び順・予算・出力は従来と同じ。
- `normalize_lua_json_skin_integer_numbers`をその場で書き換える実装にした。
- Lua / LR2のdocument decodeは、field pathを追跡しない`SkinDocument::from_json_value`を先に試し、
  失敗したときだけ`serde_path_to_error`で読み直してエラーにfield pathを含める。

## 検証

- `data/skins`の169個の`.luaskin`を、乱数の種を固定する一時フックを入れた状態で読み、
  `SkinDocument`と`SkinLoadDependencies`のDebug出力（読込エラーはその文）を変更前後で比べた。
  変更前に2回取って毎回変わる10個（DotPop・PeacefulPlayの`pairs`順に依存するid、
  mz-select・simple-resultの時計表示）を除く159個（読込エラー47個を含む）が完全に一致した。
- workspace全体のfmt、all-targets Clippy -D warnings、テスト（5236 passed, 0 failed, 111 ignored）。

## 結果

| スキン | 変更前 | 変更後 |
|---|---|---|
| SocialSkin Decide | 106.9 / 105.0ms | 90.1 / 88.4ms（−16%） |
| SocialSkin Select | 98.8 / 98.4ms | 89.3 / 89.9ms（−9%） |
| mz-select Select | 184.9 / 182.8ms | 180.0 / 181.4ms（−2%） |
| Luxez-Flat Select | 138.6 / 139.7ms | 134.3 / 133.8ms（−4%） |
| ECFN Select | 33.3 / 33.7ms | 31.4 / 31.3ms（−6%） |
| mz-select Decide | 2.4 / 2.4ms | 2.4 / 2.5ms（cache命中） |

変更後のSocialSkin Decideの内訳は、本体実行約19ms、JSON変換約43ms（うち`pairs`の走査と並べ替え約18ms）、
正規化約5ms、decode約18ms。

## 残課題

- 残りの大部分は、mluaの値の取り出し（参照の作成・解放とlock）、JSON mapの構築、serdeのdecodeで、
  JSONを経由しないLuaからdocumentへの直接変換など、構造の変更が必要。
- mz-select / Luxez-FlatのSelectはLua本体とruntime VMの構築が大半で、今回の変更の効果は小さい。
  ResultからSelectへ戻るときの待ちは先読みで隠れている。
