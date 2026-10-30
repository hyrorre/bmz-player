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
