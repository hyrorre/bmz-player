# SocialSkinの読み込みとLua互換性

対象はローカルの`data/skins/SocialSkin`、実施日は2026-10-30、Windowsです。
第三者スキンと個人の保存データは変更しません。現在の契約は
[skin.md](../../docs/skin.md)、対応範囲は[skin-compatibility.md](../../docs/skin-compatibility.md)を参照します。

## 1. 一覧登録時のヘッダ型変換

- Selectは`property[18].item[3].op = "縦"`、Result / Course Resultは
  `property[6].def = 955`でBMZの型検査に失敗していた（添字は0始まり）。
- 参照先は`.local/beatoraja/src/bms/player/beatoraja/skin/lua/LuaSkinLoader.java`の
  `int.class -> LuaValue::toint`、`String.class -> LuaValue::tojstring`と、
  `skin/SkinHeader.java`の`CustomOption.getDefaultOption`。
- LuaからJSONへの境界だけでpropertyのopを整数へ、property / filepathのdefを文字列へ
  変換する。`"縦"`は0、数値defは文字列となり、項目名に合わなければ先頭項目を使う。
  スキンの誤記を推測して別のopへ置き換えない。JSONの型制約は維持する。
- 合成fixtureのヘッダ・本体の一致、appのcatalog登録、実在する3つのSocialSkinヘッダを検証する。
  描画と実機操作は未確認。
- 検証: fmt、bmz-skinのcheck / all-targets Clippy / test（344成功・1 ignored）、
  bmz-playerのcatalog回帰テストが成功。ビルド時のhard link不可によるコピーwarningあり。

初回調査の11定義のsnapshotと型付きロード結果は、Git管理外の
`.local/performance/socialskin-current.json` / `socialskin-typed.txt`に保存した。
汎用snapshotはヘッダの型付きdecodeを行わず、Result2へ必要なnumber 74も未設定のため、
snapshotの`ok`だけで一覧登録や実際のResult互換性を判定しない。

## 2. destinationの入れ子op

- Selectは`destination[29]`、7K Playは`destination[98]`の入れ子opでdecodeに失敗していた。
- `.local/beatoraja`の`LuaSkinLoader`は数値・文字列・関数だけを条件にし、table / booleanは
  `DestinationOption`の無条件（id 0、property null）として除外する。
  したがって`{{2,3},200}`は`{200}`、`{80,{270},{271}}`は`{80}`となる。
- Lua変換時に無効要素を除外する。ORへの解釈やJSON全体の型制約緩和は行わない。
  合成fixtureと実在する7K Playのロードを検証する。
- Selectは入れ子opを通過した後、`image[470].act = "switchMenu()"`の文字列eventで
  次の型エラーとなった。文字列eventは別の修正で対応する。
- 検証: fmt、bmz-skinのcheck / all-targets Clippy / test（346成功・1 ignored）が成功。

## 3. math.randomの小数引数

- beatoraja同梱の`luaj-jse-3.0.2-custom.jar`で小数・数値文字列・無効範囲・第3引数を
  実行して比較した。`random(1.9)=1`、`random(-1.9,-1.1)=-1`、第3引数は無視。
  `random(0)`と`random(0,2147483647)`はエラーになる。
- header / load / runtime全VMでLuaJの整数変換と32bit区間幅の計算を行い、乱数生成は
  Lua 5.4のPRNGを使う。ロード時のopaque依存記録を維持する。
- Decideの定義読み込みが成功。Resultは乱数を通過後、
  `destination[755].timer = "resultRankGaugeTimer"`で次の型エラーとなる。
  actと合わせて文字列callbackの対応で修正する。
- 検証: fmt、bmz-skinのcheck / all-targets Clippy / test（349成功・1 ignored）が成功。
