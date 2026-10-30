# LITONE5の読み込み互換性

2026-10-30。対象はローカル配置の`data/skins/LITONE5`。第三者製CSV・画像等は変更しない。
現在の契約は[skin.md](../../docs/skin.md)、対応範囲は[skin-compatibility.md](../../docs/skin-compatibility.md)を参照。

## 1. 1件だけのoffset定義

- 実行ログでSelectの`offset: invalid type: map, expected a sequence`を確認した。
  同じSelectをCONTINUE素材の提供元にするDecide / Playでも解決失敗が波及していた。
- `#CUSTOMOFFSET`の変換自体は正しいが、Lua / LR2共通の数値正規化が`offset`という
  キーだけで1要素配列を畳み込み、定義オブジェクトの配列を壊していた。
  数値画像の`offset`定義配列も同じ影響を受ける。
- `.local/beatoraja/.../lr2/LR2SkinHeaderLoader.java`の`CUSTOMOFFSET`は各定義を配列に
  追加し、件数1でも維持する。BMZもオブジェクト・配列を含む配列は畳み込まない。
  `x={10}`等の数値の互換変換は維持する。
- 修正前に合成の数値正規化テストと実在Selectで失敗を再現し、修正後に成功を確認。
  定義0 / 1 / 2件、数値画像offset、通常の数値変換、CONTINUE提供元を回帰テストにする。
- 実在する全9定義（Select / Decide / Result / Course Result / AC7 / CS7 /
  HALFWIDE7 / AC9 / AC14）の型付き読み込みを確認。素材が無い環境ではskipするため、
  今回配置がある状態で実行したことと、合成テストの成功を分けて扱う。
- 生ログは`.local/performance/litone5-offset-{before,after}.log`、
  `litone5-definitions-{before,after}.log`。画面上の描画・操作は未確認。
- 検証: fmt、workspace check / all-targets Clippy / testが成功。
  bmz-playerは2745成功・46 ignored、bmz-skinは355成功・1 ignored、
  bmz-skin-documentは28成功。全体テストはWindowsのリンク作成を許可した環境で実行した。
