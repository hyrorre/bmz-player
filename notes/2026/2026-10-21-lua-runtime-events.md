# Luaのkey_pressed・act・customEvents・音声・音量の実行時対応

実施日: 2026-10-21。`main`、基点`41f50323`。
[LuaスキンAPIの互換性調査](2026-10-21-lua-api-compat.md)で残った、beatoraja最新版の
`main_state.key_pressed` / `event_exec`と、Luaの`act` / `customEvents`を実行時に動かす作業です。
現在の仕様は[skin.md](../../docs/skin.md#lua-actとcustomeventsの実行)と
[Lua APIの互換範囲](../../docs/skin.md#lua-apiの互換範囲)、対応状況は
[skin-compatibility.md](../../docs/skin-compatibility.md)を参照します。

| 段階 | commit | 内容 |
|---|---|---|
| 1 | `c26d2447` | `key_pressed`（libGDXのキーコードとキー名） |
| 2 | `c0b8e2bb` | `act` / `customEvents` / `event_exec`の実行時実行 |
| 3 | `3aa37d48` | `act` / `customEvents`内の`audio_*`を全sceneで再生 |
| 4 | `92312774` | `set_volume_*`をプロファイルの音量へ反映 |
| 4 | `002b9a64` | クリック判定に最後のフレームのLua `draw` / custom timerを使い、Play / Decideでも`act`を実行 |

## beatorajaの参照動作

参照は`.local/beatoraja/`の`83574048`（前回の調査と同じ版）です。

- `SkinLuaAccessor.loadEvent`: functionの`act`を0〜2引数で呼び、例外はログに出して無視します。
- `CustomEvent`は`action` / `condition` / `minInterval`を持ちます。`Skin.updateCustomObjects`は
  入力処理の後、描画の前に、custom timer、custom eventの順でID順に更新します。
- `MainState.executeEvent(id)`はcustom event `1000..1999`だけを実行します。入れ子の上限はありません。
- `SkinAudioLuaApiExporter`: 音量を0〜2に制限してシステム音量を掛け、共有の音声処理でpathごとに
  再生・停止します。sceneが変わっても止めません。
- `set_volume_*`はAudioConfigのシステム・キー・BGM音量を制限なしで書き換えます。
- `MainController.render`は全stateで`Skin.mousePressed`を呼び、各objectは直前の`prepare`で
  決まった`draw`で判定します。
- `key_pressed`のキーコードは同梱`lib/gdx.jar`の`Input.Keys`です。LuaJの`luajava.bindClass`で
  名前と番号の対応140件を取り出しました（Escape=131、F1〜F12=244〜255等）。

## 実装の判断

- Luaの`act`は文書上も整数の`act`のまま扱えるよう、runtime callback番号を
  `-1_000_000..=-1_999_999`のイベントIDへ符号化しました。組み込みイベントと衝突しません。
- クリックはqueueに積み、そのsceneの次フレームでcustom timerより先に実行します。
  クリック判定とLuaの実行を分け、描画中にLua VMを二重に使わないためです。
- ロード時に宣言的な音声イベント（timer条件と`audioActions`）へ変換できる`customEvents`は、
  従来の経路を維持し、毎フレームは評価しません。`event_exec`からは実行します。
- skinのVMは音声・設定を直接触らず、組み込みイベント・音声・音量の効果を`SkinLuaEffect`として
  appへ渡します。上限は効果256件、保留クリック64件、`event_exec`の入れ子16段です。
- 音声は許可root内で解決した絶対pathをappへ渡し、sceneごとのSoundId範囲で鳴らします。
  未知のfileはbackgroundでdecodeし、decode中の命令はsampleの登録と同時に順に送ります。
  sceneを離れるとloopだけを止めます。beatorajaとの差は次の2点です。
  - 描画callbackの`audio_*`は鳴らしません（beatorajaは呼ぶたびに鳴らします）。
  - 音量は単発がシステムSE、loopがシステムBGMです（beatorajaはどちらもシステム音量）。
- `set_volume_*`のシステム音量はBMZのマスター音量です。Selectの音量スライダー17〜19と同じhelperで
  反映し、保存も同じ経路です。同じフレーム内の`volume_*`は設定値を返します。
- クリック判定は、最後に描画したフレームのLua `draw`の結果とcustom timerを使います。
  判定のためにLuaを再実行しないので、stateful callbackの状態を変えません。
  Selectの曲バーは行ごとに条件を評価するため対象外です。
  Play / Decideは、Luaの`act`を持つskinだけ最後に描画した状態を保持します。
- Lua-to-JSON変換は、従来どおり`act` functionを除いて変換を続けます（警告を追加）。

## 検証

- 追加したテスト:
  - Lua VM: [lua_runtime_events.rs](../../crates/bmz-skin/src/tests/lua_runtime_events.rs)、
    `lua_keys`のunit test、Lua-to-JSON変換
  - app側: [events.rs](../../crates/bmz-player/src/skin_loader/tests/lua/events.rs)（クリックから
    描画・効果まで、最後のフレームによるクリック判定、Decide / Play）、
    [skin_audio.rs](../../crates/bmz-player/src/skin_audio.rs)（backgroundのdecodeと命令の順序、
    loopの停止、decode失敗、宣言済みassetの共有）、音量の対応
- 各段階で`cargo fmt --check`と、対象crateのcheck / all-targets Clippy / testを実行しました。
  最後にworkspace全体のcheck / Clippy / testを実行し、testは4565件成功、失敗0件、ignored 55件でした。
- インストール済みスキンの[比較ハーネス](../../crates/bmz-skin/src/tests/installed_skin_compat.rs)の
  変更前ベースラインは、作業開始直後（`41f50323`のcommit後、`key_pressed`のcommit前）に取得しました。
  出力は`.local/lua-event-plan/`に残し、共有しません。
  - 段階2の後はregression 0、変化99件、全段階の後（`002b9a64`）はregression 0、変化98件でした。
    主な変化は、推論できない`act`がruntime callbackになったこと（Luxez-FlatのSelect / Result、
    MILLIONDOLLARのResult等）です。2回の件数の差は次の評価順の揺れです。
  - Luxez-Flatの一部の`draw`がframe 0だけ`false`になる差も出ました。`flag_score`等のglobalを
    value callbackが更新し、drawがそれを読むためです。ハーネスはcallbackを番号順に評価し、
    番号はLuaの`pairs`順（VMごとにhash seedが変わる）で決まります。同じコードでも実行ごとに
    結果が入れ替わることを確認しました。今回の変更による差ではありません。
- 未確認: 実機でのクリック・音声・音量の変化、macOS / Linux。

## 残課題

- 比較ハーネスのcallback評価順が実行ごとに変わり、globalを共有するskinで差が揺れます。
  描画と同じdestination順など、決まった順で評価すると比較が安定します。
- `event_observe_timer_off`は最初の評価で発火しない差を残しています。
- 描画・custom timer callbackからの`audio_*`は鳴らしません。
- Selectの曲バーのLua `draw`はクリック判定で評価しません。
- クリック方向（`click` 2 / 3）の判定は、canvas座標の矩形とsurface座標のcursorを比べています。
  Select / Resultの既存の処理と同じで、skinの縦横比と画面が違うと中央付近で左右・上下を誤る
  可能性があります（今回の変更前からの動作）。
