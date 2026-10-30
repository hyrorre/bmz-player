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

## 4. 文字列act / timerとdestination timer callback

- `SkinLuaAccessor.loadEvent(String)`はscriptをクリック時に実行する。
  `loadTimerProperty(String)`は`return <script>`を試行し、関数ならその関数を保持し、
  その他は式を毎回実行する。SocialSkinはメニューのactと多くのcustom timerにこの形式を使う。
- 同じ変換をロードVMとclean runtime VMで行い、closure / module stateを維持する。
  destination timerは描画時にi64の開始時刻を取得し、クリックは最後の描画時刻で判定する。
  任意timerを描画cacheの対象にしない。Luaの実ファイル書き込みやJava実行は追加しない。
- 実在する11定義すべての型付きロードが成功した（ノーツ数1000、EX1500等のResult値を供給）。
  素材なしでの早期returnではなく、配置済みのSocialSkinを使用した。
- 検証: workspace check / all-targets Clippyが成功。workspace testは通常権限では
  既存のjunction作成テストが拒否されたため、bmz-playerを必要な権限で再実行し
  2743成功・46 ignoredを確認。他crateは通常実行で成功（bmz-render 974、bmz-skin 350等）。
  新規の描画・クリック回帰テストも成功。GPU上の見た目と実操作は未確認。

## 5. ヘルプ一覧のslider event

- SocialSkinの`helpOperationSliderEvent(value)`はFloatWriter。beatorajaの
  `SkinSlider.mousePressed`は方向別のトラック位置を0〜1としてwriterへ渡す。
  BMZは関数を破棄していたため、一覧ドラッグの処理に到達しなかった。
- eventを永続VMのcallbackへ保持し、floatのまま入力順で次フレームへ渡す。
  Select / Resultで最後のdraw / timer値を使い、複数のtype=0 sliderをcallback IDで区別する。
  通常のスクロール・音量sliderは既存の処理経路を使う。
- MainControllerの`mousePressed` / `mouseDragged`と入力processorを照合した。
  beatorajaも押下・移動イベントを消費してwriterを呼ぶため、静止中の毎フレーム再実行は追加しない。
- 検証: workspace check / all-targets Clippy / testが成功。bmz-playerは2744成功・46 ignored。
  位置のfloat維持、hidden時のhit抑制、2本のtype=0 sliderのドラッグ識別とclosure更新を確認。
  画面上のヘルプ操作は未確認。
