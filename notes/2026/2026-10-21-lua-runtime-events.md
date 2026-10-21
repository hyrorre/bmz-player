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
  （下の追記で対応しました）
- `event_observe_timer_off`は最初の評価で発火しない差を残しています。
- 描画・custom timer callbackからの`audio_*`は鳴らしません。
- Selectの曲バーのLua `draw`はクリック判定で評価しません。
- クリック方向（`click` 2 / 3）の判定は、canvas座標の矩形とsurface座標のcursorを比べていました。
  skinの縦横比と画面が違うと、中央付近で左右・上下を誤る可能性がありました（今回の変更前からの動作）。
  同日、hit testの矩形をcursorと同じsurface座標で返すように直しました。

## 追記（2026-10-21）: 比較ハーネスの評価順

- [比較ハーネス](../../crates/bmz-skin/src/tests/installed_skin_compat.rs)のruntime callbackを、
  番号順ではなく描画と同じ順で評価するようにしました。destinationを文書順にたどり、Luaの数値`value`を
  そのdestinationの`draw`の前（beatorajaの`SkinNumber.prepare`）、Luaの`text`を後（`SkinText.prepare`）に
  評価します。各callbackは1フレームに1回、最初に使われる位置で評価し、どのdestinationからも
  使われないcallbackはfield path順で続けます。
- snapshotの形式は変わりません。変更前のsnapshotと比べると、globalを共有するskinのframe 0が
  変わることがあります。比較するときはベースラインを取り直します。
- Luxez-Flatの3ファイルで、変更前に2回、変更後に3回ハーネスを実行しました。同じfield pathの
  `callback_results`の差は、変更前35件、変更後0件でした。出力は`.local/lua-eval-order/`に残し、共有しません。
- 残る揺れ: `music_select.luaskin`の`document` / `document_scene`では、`flag_score` / `scorerate`を読む
  `draw` 28件（`$.destination[262]`等）がruntime callbackになるかどうかが実行ごとに変わります。
  Lua-to-JSON変換はtableのkeyを`pairs`順にたどります。そのため、`value`関数が`destination`の`draw`より
  先にprobeされるかどうかがhash seedで変わることが原因と考えられます。読み込み側の動作なので今回は変更していません。
  ハーネスは同じ実行内の2回の読み込みで出た差を`unstable`として扱い、regressionにしません。

## 追記（2026-10-21）: Lua推論の`pairs`順依存

上の「残る揺れ」の原因を調べ、読み込み側を直しました。

- 原因: Lua-to-JSON変換はtableのfieldを`pairs`順に変換し、推論のためにfunctionを呼びます。
  Luxe Flat Selectでは`scorerate`を数字の`value`だけが作ります。`value`が先にprobeされた読み込みでだけ、
  `scorerate`が残っていました。
  - ランク画像（`diff_rank_*`）と差分桁の配置は、`scorerate`が無いと推論を諦めてruntime callbackになります。
    `flag_score` / `scorerate`を書き込む数字の`value`は組み込み式に推論されるため、runtimeの`draw`では
    `flag_score`がfalseのままで、常に非表示です。
  - スコアレート帯（`scorerate == 10/10`、`1/10`以上`10/10`未満、`1/10`未満）は、`value`の残した
    `scorerate`を定数として`select_score_available()`だけに推論されていました。100%の帯がスコアの
    ある曲で常に表示され、残る2帯はruntimeで常に非表示です。`value`が後なら3帯とも非表示です。
- 修正:
  - [変換](../../crates/bmz-skin/src/lua/conversion.rs)はfieldを固定順（整数keyは値順、文字列keyはバイト順）で
    処理します。推論結果とruntime callback IDが読み込みごとに変わりません。
  - Luxe Flatのランク推論は、未作成の`scorerate` globalも一時的に設定して確かめ、nilへ戻します。
    boolean globalの`flag_score` / `rank_plus`は実在する場合だけ使います。mluaはnilを`false`として読むため、
    mz-selectのclosure内の`flag_score`をglobalで隠していました（この修正の途中、ハーネスで見つけました）。
  - スコアレート帯は`select_score_available() and score_rate_tenths(lower,upper)`へ推論し、
    [描画側](../../crates/bmz-render/src/skin/condition/draw.rs)に`score_rate_tenths`を追加しました。
    `select_score_available()`の推論は、`scorerate`を変えると結果が変わる`draw`を受け付けません。
  - [回帰テスト](../../crates/bmz-skin/src/tests/lua_inference_order.rs)は同じskinを8回読み込みます。
    修正前は両テストとも失敗しました。
- ハーネス（全123件、`data/skins`）: 変更前の3回をベースラインに、変更後の3回と比べました。
  - regression 0件。変化はLuxez-Flat `music_select`の`document` / `document_scene`のスコアレート帯だけです。
  - 読み込み間で差が出るentryは20件から18件になりました。Luxez-Flat `music_select`の文書と
    GenericThemeのwarning順が安定しました。Luxez-Flat / mz-selectの`music_select`の`header.hash`は
    以前から揺れており、未調査です。
  - 残る16件（ModernChic、PeacefulPlay、WMII_FHDのcourseResult、mz-select / simple-resultのResult等）は、
    skin自体が`pairs`や乱数・時刻を使います。読み込み側の`pairs`はほかに3か所あり、いずれも順序に
    依存しないことを確認しました。skin側の原因は未調査です。
- 未確認: 実機の選曲画面でのLuxez-Flatのランク画像・スコアレート表示。
