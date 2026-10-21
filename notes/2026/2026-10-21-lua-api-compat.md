# LuaスキンAPIの互換性調査と拡張

実施日: 2026-10-21。ブランチ`claude/lua-skin-api-compat`、基点`c010fd35`。
現在の仕様は[skin.md](../../docs/skin.md#lua-apiの互換範囲)、対応状況は
[skin-compatibility.md](../../docs/skin-compatibility.md#luaの推論とruntime)、比較手順は
[development.md](../../docs/development.md#インストール済みluaスキンの互換比較)を参照します。

## 対象と方法

| 比較対象 | 版 | 位置 |
|---|---|---|
| beatoraja | `83574048`（LuaSkinLoaderのluajava facade追加以降） | `.local/beatoraja/src/bms/player/beatoraja/skin/lua/` |
| LR2oraja Endless Dream | `a31aa5f6`（rian-eimu fork） | `.local/lr2oraja-endlessdream/core/src/bms/player/beatoraja/skin/lua/` |
| LuaJ | beatoraja同梱`lib/luaj-jse-3.0.2-custom.jar` | 標準ライブラリの参照出力をJava 21で取得 |

- BMZは`mlua 0.10.5`（`lua54` / `vendored`）のLua 5.4です。`LUA_COMPAT_MATHLIB`により
  `math.pow`等は使えますが、`bit32`はありませんでした。
- インストール済みスキンは`data/skins`の30フォルダ、Luaを持つ18フォルダ（`.luaskin` 123件、
  `.lua` 1424件）を対象にしました。スキンへの書き込みはしていません。
- 使用APIはLuaの字句解析による静的調査（コメント・文字列を除外し、`local s = require("main_state")`
  等の別名を追跡）と、実行時計測の両方で抽出しました。実行時計測は、API関数を転送する
  wrapperとAPI table / globalの未定義参照を記録するmetatableをtest buildだけに一時的に入れ、
  計測後に削除しました（コミットしていません）。文字列から組み立てたAPI名・動的な`require`の
  API参照は見つかりませんでした。
- 変更前ベースラインは、拡張前のcommit `4389027d`で[比較ハーネス](../../crates/bmz-skin/src/tests/installed_skin_compat.rs)を
  3回実行して取得しました。各`.luaskin`を初期ロード・代表的なscene状態・compatモードで読み、
  ヘッダの選択肢、文書の各セクション、font / source、警告、runtime callbackの評価結果を記録します。
  スナップショットと計測スクリプトは`.local/lua-api-compat/`に残し、共有しません。

## BMZのLua実行環境（調査時点）

- 1回のロードでheader pass、document pass、必要時のruntime VMの3つのLua VMを作ります。
  VMはロードごと・スキンごとに独立し、`package.loaded`も共有しません。
- `main_state`はglobalではなく`require("main_state")`で取得します（実体は`bmz_main_state`）。
  `timer_util` / `event_util` / `luajava`も`require`で取得します。
- document passは関数fieldを推論して宣言的なref / 式へ変換し、推論できないdraw / value /
  text / graph / slider / custom timerだけをruntime VMのcallbackへ残します。推論中のAPIは
  probe値を返し、実行時だけ意味を持つ値（画面寸法等）は推論中にエラーにしてruntimeへ回します。
- `require` / `dofile` / `loadfile` / `io.open`は`SkinPathContext`の許可rootに限定され、
  `debug`はnil、`os`はclock / date / timeのみ、`io`の書込はstubです。
- Luaエラーはロード時はスキンの読込失敗、runtimeではcallbackごとに1回だけ警告し、
  draw=false / 数値・文字列は未取得にします。

## API互換性一覧

「BMZ前」は拡張前、「BMZ後」は本作業後です。EDはbeatoraja 0.8系のAPIを持ち、Lua APIの
独自拡張はありませんでした。beatoraja最新版で追加されたものは「新」と記します。

| API・機能 | beatoraja | LR2orajaED | BMZ前 | BMZ後 | 方針 |
|---|---|---|---|---|---|
| `option` / `number` / `float_number` / `text` | 実装済み（名前も可） | 実装済み（IDのみ） | 完全（名前も可） | 完全 | 維持 |
| 上記のnil / boolean引数 | 0 / false / 空 | ID 0 | Luaエラー | Luaエラー | 維持（文書化） |
| `offset` / `event_index` / `judge` | 実装済み | 実装済み | 完全 | 完全 | 維持 |
| `timer` / `time` / `set_timer` | 実装済み | 実装済み | 部分（OFF値が32bit最小値） | 同左 | 維持（既存値） |
| `timer_off_value` | `Long.MIN_VALUE` | 同左 | `-2147483648` | 同左 | 維持（既存値） |
| `exscore` | 実装済み | 実装済み | 完全 | 完全 | 維持 |
| `rate` / `rate_best` / `rate_rival` | 実装済み | 実装済み | 未対応 | 完全 | 追加（ref 111 / 112 / 115） |
| `exscore_best` / `exscore_rival` | 実装済み | 実装済み | 未対応 | 完全 | 追加（ref 150 / 121） |
| `gauge` | Play中のみ | Play中のみ | 未対応 | 完全 | 追加 |
| `gauge_type` | Play中のみ、他0 | 同左 | 部分（全sceneで値） | 同左 | 維持 |
| `volume_*` / `set_volume_*` | 設定を読み書き | 同左 | 部分（setは無作用） | 同左 | 維持 |
| `audio_play` / `audio_loop` 音量省略 | 1.0 | 1.0（0以下も1.0） | 記録しない | 1.0 | 修正 |
| `audio_play` 音量0以下 | 0（clamp） | 1.0 | 0 | 0 | 維持（最新版と同じ） |
| `audio_stop` | 実装済み | 実装済み | 宣言的に対応 | 同左 | 維持 |
| `audio_preload` / `audio_dispose` | 新 | なし | 未対応 | 無作用でtrue | 追加 |
| `event_exec` | 実装済み | 実装済み | 未対応 | 未対応 | 未対応（汎用イベントruntimeが必要） |
| `numbers` / `timer_is_on` / `timer_is_off` / `timer_elapsed*` / `screen_*` | 新 | なし | 完全 | 完全 | 維持 |
| `key_pressed` | 新 | なし | 未対応 | 未対応 | 未対応（入力供給が必要） |
| `file_*` / `http_*` / `open_url` | 新 | なし | 未対応 | 未対応 | 未対応（書込・通信） |
| `skin_config.option` / `offset` / `file_path` / `get_path` | 実装済み | 実装済み | 完全 | 完全 | 維持 |
| `skin_config.enabled_options` | 実装済み | 実装済み | 未対応 | 完全（read-only proxy） | 追加 |
| `timer_util.*` | 実装済み | 実装済み | 部分 | 部分 | `new_passive_timer`の戻り値をtrueへ修正 |
| `event_util.event_observe_timer_off` | 初回OFFで発火 | 同左 | 初回は発火しない | 同左 | 維持（理由はskin.md） |
| `event_util.event_min_interval` | scene時計 | 同左 | 壁時計（推論時のみ実行） | 同左 | 維持 |
| `luajava.bindClass` / `newInstance` | facade | 完全なLuajava | 無害なstub | 同左 | 維持 |
| `luajava.new` | File facade | 完全 | 未対応 | 無害なstub | 追加 |
| `bit32` | LuaJ標準 | LuaJ標準 | 未対応 | 完全 | 追加 |
| `os.difftime` | 実装済み | 実装済み | 未対応 | 完全 | 追加 |
| `os.date`の指定子 | LuaJ | LuaJ | `%Y %m %d %H %M %S %%`のみ | LuaJの指定子 | 追加 |
| `os.date()`の既定書式 | `%c` | `%c` | `%Y-%m-%d %H:%M:%S` | 同左 | 維持（既存表示） |
| `os.date`の未知指定子 | エラー | エラー | 文字のまま | 同左 | 維持 |
| `os.getenv` / `execute` / `remove`等 | 最新版は削除 | 実装済み | なし | なし | 提供しない |
| `debug` | `getmetatable`のみ | 標準 | nil | nil | 維持 |
| `unpack` / `loadstring` | なし | なし | なし | なし | 維持 |
| 整数値floatの`tostring` | `5` | `5` | `5.0` | `5.0` | 差を文書化（Lua 5.4） |
| `string.format("%d", 2.5)` | 切り捨て | 切り捨て | エラー | エラー | 差を文書化（標準ライブラリは変更しない） |
| `font.fallback`（Luaのtable） | 配列のみ | なし | 文字列・table・配列 | 同左 | 維持 |

BMZ独自APIは`bmz.option` / `bmz.get_option`、runtime VMだけの`main_state.float`（float_numberの
別名）、`total_play_counts_in_session` / `total_play_notes_in_session` / `score_date_sec_time`、
名前付きプロパティです。いずれも変更していません。

## インストール済みスキンの調査結果

静的調査で使用を確認したAPIです（`string` / `table` / `math`を除く）。

| スキン | 使用API |
|---|---|
| ADFX02 | `bmz.get_option`, `io.open`, `luajava.bindClass`, `main_state.event_index` / `gauge_type` / `number` / `offset` / `option` / `text` / `time` / `timer` / `timer_off_value`, `skin_config.get_path` / `offset` / `option`, `timer_util.timer_observe_boolean` |
| ECFN | `bmz.get_option`, `main_state.gauge_type` / `number` / `option` / `text` / `time` / `timer` / `timer_off_value`, `skin_config.get_path` / `offset` / `option` |
| GenericTheme | `main_state.judge` / `number` / `option` / `time` / `timer` / `timer_off_value`, `skin_config.file_path` / `get_path` / `offset` / `option` |
| Hub | `io.open` / `close`, `main_state.audio_play` / `event_index` / `number` / `option` / `timer` / `timer_off_value` / `volume_sys`, `os.clock` / `date`, `skin_config.get_path` / `offset` / `option`, `timer_util.now_timer` |
| LITONE9 | `main_state.gauge_type` / `number` / `option` / `score_date_sec_time` / `text` / `total_play_counts_in_session` / `total_play_notes_in_session`, `os.date`, `skin_config.get_path` / `offset` / `option` |
| Luxez-Flat | `bmz.get_option`, `io.open`, `luajava.bindClass`, `main_state.audio_play` / `event_index` / `number` / `option` / `set_timer` / `text` / `time` / `timer` / `timer_off_value`, `skin_config.get_path` / `option` |
| MILLIONDOLLAR | `io.open`, `luajava.bindClass`, `main_state.audio_loop` / `audio_play` / `audio_stop` / `event_index` / `number` / `option` / `text` / `timer` / `volume_sys`, `os.date` / `time`, `skin_config.get_path` / `option`, `timer_util.is_timer_on` / `timer_observe_boolean` |
| ModernChic | `io.open`, `luajava.bindClass` / `new` / `newInstance`, `main_state.audio_play` / `event_index` / `exscore` / `float_number` / **`gauge`** / `gauge_type` / `judge` / `number` / `option` / **`rate`** / `set_volume_*` / `text` / `time` / `timer` / `timer_off_value` / `volume_*`, `os.date`（**`%y`**）/ `time`, `skin_config.get_path` / `offset` / `option`, `timer_util.timer_observe_boolean` |
| PeacefulPlay | `main_state.event_index` / **`gauge`** / `gauge_type` / `judge` / `number` / `option` / `text` / `time` / `timer` / `timer_off_value`, `skin_config.option` |
| Rmz-skin | `main_state.event_index` / `float_number` / `gauge_type` / `number` / `offset` / `option` / `text` / `timer` / `timer_off_value`, `skin_config.get_path` / `option` |
| Starseeker | `io.open`, `luajava.bindClass`, `main_state.event_index` / **`gauge`** / `gauge_type` / `number` / `offset` / `option` / `text` / `time` / `timer` / `timer_off_value`, `skin_config.get_path` / `offset` / `option` |
| WMII_FHD | `io.open`, `luajava.bindClass`, `main_state.event_index` / `exscore` / **`float`**（BMZ独自） / `float_number` / `gauge_type` / `number` / `offset` / `option` / `text` / `timer` / `timer_off_value`, `os.clock`, `skin_config.offset` / `option` |
| mz-select | `bmz.get_option`, `io.open` / `lines` / `close`, `luajava.bindClass`, `main_state.audio_play` / `event_index` / `number` / `option` / `set_timer` / `text` / `time` / `timer` / `timer_off_value` / `volume_sys`, `os.clock` / `date`, `skin_config.get_path` / `offset` / `option`, `timer_util.now_timer` / `timer_observe_boolean` |
| simple-decide | `skin_config.option` |
| simple-play | `luajava.bindClass`, `main_state.event_index` / `exscore` / `float_number` / `number` / `option` / `timer` / `timer_off_value` / `volume_sys`, `skin_config.get_path` / `offset` / `option`, `timer_util.now_timer` |
| simple-play-simple | `io.open`, `main_state.exscore` / `float_number` / `number` / `timer` / `timer_off_value`, `skin_config.file_path` / `get_path` / `offset` / `option` |
| simple-result | `io.open`, `main_state.number` / `option` / `text`, `os.clock` / `date`, `skin_config.option` |

太字は拡張前に未対応・差異のあったAPIです。実行時計測では、既定設定で実際に評価された未定義参照は
WMII_FHDの`main_state.float`（document VM）とPeacefulPlayの`main_state.gauge`でした。
ModernChicの`rate` / `gauge`、Starseekerの`gauge`は既定外の設定でだけ読まれます。
Lua 5.3以降専用の構文・関数（`//`、ビット演算子、`goto`、`utf8`等）を使うスキンはありませんでした。

### 変更前後の比較

| スキン | entry | header 前→後 | 本体 初期 / scene / compat 前→後 | callback失敗 前→後 |
|---|---:|---|---|---|
| ADFX02 | 31 | 31→31 | 29→29 / 29→29 / 29→29 | 0→0 |
| ECFN | 8 | 8→8 | 7→7 / 7→7 / 7→7 | 27→27 |
| GenericTheme | 2 | 2→2 | 2→2 / 2→2 / 2→2 | 0→0 |
| Hub | 11 | 0→0 | 0→0 / 0→0 / 0→0 | 0→0 |
| LITONE9 | 6 | 6→6 | 6→6 / 6→6 / 6→6 | 0→0 |
| Luxez-Flat | 3 | 3→3 | 3→3 / 3→3 / 3→3 | 4→4 |
| MILLIONDOLLAR | 1 | 1→1 | 1→1 / 1→1 / 1→1 | 0→0 |
| ModernChic | 10 | 10→10 | 9→9 / 10→10 / 10→10 | 0→0 |
| PeacefulPlay | 2 | 2→2 | 2→2 / 2→2 / 2→2 | 104→0 |
| Rmz-skin | 6 | 6→6 | 6→6 / 6→6 / 6→6 | 6→6 |
| Starseeker | 7 | 7→7 | 7→7 / 7→7 / 7→7 | 0→0 |
| WMII_FHD | 10 | 10→10 | 9→9 / 9→9 / 9→9 | 0→0 |
| mz-select | 8 | 8→8 | 8→8 / 8→8 / 8→8 | 0→0 |
| simple-decide / simple-play / simple-play-simple / simple-result | 1 / 5 / 10 / 2 | 全件成功→同左 | 全件成功→同左 | 0→0 |

- 回帰（読込失敗・選択肢 / font / sourceの消失・object数の減少・新しいERROR警告・callback失敗の
  増加）は0件です。文書の内容、選択肢、font（fallback含む）、source、警告に差分はありません。
- 意図した差分はPeacefulPlayのcompatモードだけです。`main_state.gauge()`を読む52件×2のcallbackが
  失敗しなくなりました。autoモードの同スキンはobject IDによる組み込み式のままで変化しません。
- 既定外の設定でも比較しました。Starseekerのフレーム`[CFM] CV01 Hatsune Miku`（1P / 2P）と
  `[CFM] LR2`、ModernChicの「プレイ履歴の保存」「プレイ状況詳細モード」は前後の差分0件です。
  Starseekerのフレーム数字は`gauge()`が2未満のとき値を返さないため、ハーネスの既定状態では前後とも
  未取得です。実際の値はゲージ値を与える[回帰テスト](../../crates/bmz-skin/src/tests/lua_api_extensions.rs)で確認しました。
- 3回のベースライン間で揺れる項目（ModernChicの乱数、mz-select / simple-resultの時刻表示、
  Luxez-Flat選曲の推論結果）は比較から除外しました。Luxez-Flat選曲は、同じ設定でもLua 5.4の
  ハッシュ順によってruntime callbackになるdestinationが28件変わります。既存の挙動で、本作業の対象外です。

### 変更前から読み込めないもの

いずれも拡張の前後で同じエラーです。

| entry | 原因 |
|---|---|
| Hub（11件） | Hubは他スキンのフォルダへ置く改造用部品です。単独では`package.path`からホストスキンを判定できず、beatorajaでも同じ箇所で失敗します |
| ECFN / ADFX02のplay14 | 既定のスコアグラフ位置（902 / 903以外）で`graphside_x`が未定義になるスキン側の分岐です |
| ADFX02/ADFX_02/RESULT/result2 | `destination[19]`がmapで、BMZの文書decodeが配列を要求します（Lua API以外の既存差異） |
| ModernChic/decide | ロード時に難易度option `150..155`を読み、すべてfalseだと色tableがnilになって61行目で失敗します。appもDecideのロード状態（`lua_runtime_state_for_frontend_last_play`）に難易度を渡さないため、実際のappでも読めません（2026-10-15のログで同じエラーを確認）。beatorajaは選択曲のある状態でDecideを読むため、いずれか1つがtrueになります。ハーネスのscene状態は153をtrueにしているため読めます |
| WMII_FHD/result/result | `config_sys.json`と`player/<name>/config_player.json`をnil確認なしで読みます。appは全Luaスキンへ読取専用の仮想ファイル（`lua_compat_virtual_io_files`、空のキー割当）を渡すため読めますが、bmz-skinを直接呼ぶハーネスにはありません。2026-10-20のログで、appでの読込とパネル切替を確認しました |
| ModernChic play（詳細モード有効時） | 解析ログを書き込んでから読む処理で、BMZは`io`の書込を永続化しないため読み込みに失敗します |

変更前からのruntime callback失敗（件数は前後で同じ）の原因です。

| entry | 原因 |
|---|---|
| ECFN/select（9件） | 単独の`ECFN`フォルダから`../../rank_diff.lua`を読み、`data/skins`直下にファイルがありません。`ADFX02/ECFN`の配置では読めます |
| Rmz-skin play（compat、各1件） | `slider[3].value`が`nil`を返します。BMZは数値callbackの`nil`を失敗として描画しませんが、LuaJの`tofloat()`は0です |
| Luxez-Flat/music_select（2件） | ハーネスの仮の状態で0除算のNaNになります |

数値callbackの`nil`は、beatoraja / EDでは`toint()` / `tofloat()`で0になります（timer callbackの
`nil`はBMZも0として扱います）。Starseekerのフレームのゲージ数字は、ゲージが2未満のとき
beatorajaでは0を表示し、BMZでは表示しません。既存スキンの表示が変わるため本作業では変えず、
残課題とします。

## 実装の判断

- 新APIは既存の評価経路を再利用しました。`rate*`はfloat_number 111 / 112 / 115、`exscore_*`は
  number 150 / 121と同じprobe・依存値・推論・runtime値になり、値計算を重複させていません。
- `gauge()`に相当するrefはないため、推論中はエラーにしてruntime callbackへ回します。
  ロード時の0を定数として畳み込むと、ゲージに追従しない描画になるためです。
  appの`LuaMainState::gauge`はPlay中だけ現在ゲージを返し、beatorajaの`BMSPlayer`判定と合わせました。
- `audio_play`の音量省略は両実装とも1.0です。数値の扱い（0以下を0とする最新版と、1.0とするED）は
  既存のBMZ（最新版と同じ）を維持しました。
- `skin_config.enabled_options`は`skin_config.option`と同じく読んだopだけを文書の依存値に記録する
  proxyにし、読まないスキンのdocument cacheに影響しないようにしました。
- `os.date()`の既定書式と未知指定子の扱いは、mz-select / simple-resultのResult表示や
  既存スキンの読込を変えないため維持しました。
- runtime VMの音声命令記録を止めました。runtime callbackで毎フレーム`audio_play`を呼ぶと、
  誰も取り出さない`Vec`が増え続けていました。

## 性能

releaseビルド、Windows 11、同じ一時ベンチを変更前（`4389027d`）と変更後で3回ずつ交互に実行しました
（ベンチはコミットしていません）。

| 項目 | 変更前 | 変更後 |
|---|---|---|
| ロード（compat、draw callback 500件、中央値） | 5.84〜6.07 ms | 6.00〜6.22 ms |
| draw callback 1件 | 1322〜1335 ns | 1336〜1373 ns |
| API 53回を呼ぶtext callback 1件 | 13.9〜14.1 µs | 14.6〜15.1 µs |
| インストール済み123件の比較ハーネス合計（8並列） | 67.6 / 72.9 s | 66.4 / 65.9 s |

- ロードは1 VMあたり`bit32`・`enabled_options`・`main_state`の追加関数の生成で約0.15 ms増えました。
- API呼び出しの多いcallbackは約5%（1回あたり約13 ns）遅くなりました。`main_state`のfield数の
  増加によるtable参照の差と見ていますが、原因は切り分けていません。通常のdraw callbackは1〜2%です。
- 実スキンの合計時間は並列実行の揺れの範囲内で、差は見られませんでした。
- 2万フレーム評価してもLuaヒープは増えません（[回帰テスト](../../crates/bmz-skin/src/lua/tests/runtime_resources.rs)）。

## 未確認事項・残課題

- 検証はWindowsだけです。macOS / Linuxは未検証です。追加したコードにOS依存のパス・文字コード
  処理はなく、`os.date("%z")`は既存の`localtime`経路を使います。
- 実際のゲーム画面での目視確認、同じスキン・条件でのbeatorajaとの描画比較、スクリーンショット比較は
  行っていません。比較は文書・runtime callbackの評価結果までです。
- `event_exec`（act / customEventsの汎用runtime）、`key_pressed`、`file_*` / `http_*` / `open_url`、
  luajavaのFile facadeは未対応です。`act = function() main_state.event_exec(id) end`のような定数
  イベントの推論は、今後の追加候補です。
- Lua 5.4とLuaJの数値の文字列化・`string.format("%d")`の差は残ります。標準ライブラリを変えずに
  吸収する方法は未検討です。
- 数値callbackの`nil`を0として扱うか（beatoraja互換）は、既存スキンの表示変更を伴うため未対応です。
- インストール済みスキンに`font.fallback`を使うLuaスキンはありませんでした。Luaのtableから
  文書への変換（文字列・`{path, type}`・配列・空table、日本語のパス）とUnicodeの文字列は
  [回帰テスト](../../crates/bmz-skin/src/tests/lua_document_conversion.rs)で確認しています。
