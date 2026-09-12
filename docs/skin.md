# BMZ Skin Notes

BMZ は beatoraja JSON / Lua skin の互換を基本にする。既存 beatoraja skin type は
そのまま扱い、BMZ 独自の key mode だけ拡張 skin type を予約する。

## LR2 Play 表示参照

LR2 play skin の `SRC_BUTTON 40/41` は、1P/2P の実際に有効なゲージ種類を
LR2 の画像順（NORMAL / HARD / HAZARD / EASY / P-ATTACK / G-ATTACK）へ変換する。
ASSIST EASY は EASY、EX HARD と段位ゲージは HARD の画像へフォールバックする。
表示用 ref とクリック用 act は分離し、HS-FIX (`55`) は従来の参照を維持する。

`SRC_NUMBER 10/11` は共通の現在HSを100倍して整数化する（例: `2.50 → 250`）。
桁数はスキンの指定を維持するため、3桁欄ではHS 10以上の上位桁は表示できない。
`127` は2Pゲージ残量の整数部。BATTLE skin type `12/13` の `121` は2P実EX SCOREを
参照し、ターゲットスコアの有無に依存しない。通常playの `121` とJSON/Luaの参照は
従来どおり。2P不在の数値は非表示とし、2Pの残量・スコアが0の場合は0を表示する。

## Lua Runtime Compatibility Mode

通常の `auto` モードはLua functionをロード時に宣言的なref/式へ変換し、推論できない
対応済みfieldだけ永続Lua VMのruntime callbackへ残す。スキン開発・beatoraja比較では
`--lua-skin-runtime compat` を指定すると、次のfunction fieldを推論せず実行時に評価する。

- destination `draw`
- `value[]`, `text[]`, `graph[]`, `slider[]` の `value`

Luaファイル全体をフレームごとに再ロードするのではなく、ロード時に作成したclosureと
module stateを保持する。callbackからは現在フレームの `main_state` を参照できる。
各callbackの命令数上限、フレーム全体の命令数上限、Lua VMのメモリ上限を適用し、失敗時は
draw=false、数値/文字列は未取得として扱う。runtime callbackを含むdocumentはVMをclone
できないためdocument cacheとLua-to-JSON変換の対象外になる。

Lua sandboxはロード時の互換APIとして`os.clock` / `os.date` / `os.time`を提供する。
`os.time`の日付tableは標準Luaと同様にローカル時刻として正規化し、正規化後の値を
tableへ書き戻す。引数なしでは現在時刻を返し、`os.date`は`!` prefixでUTCを選べる。libGDXの
`Controllers.getControllers()`は入力のない空配列として`size` / `get()` / `first()`を
提供する。`io`の書き込みstubは内容を永続化しない。初回document構築は、蓄積データを
Luaで解析するResult skinも通せる専用の命令数上限を持ち、runtime callback単位の上限は
従来の小さい値を維持する。

判定の存在を表すoption `2241..2246` (PGREAT / GREAT / GOOD / BAD / POOR / EMPTY POOR)
はResultだけでなくPlay中も現在の判定数を参照する。これにより、LITONE11などが
`-2244` / `-2245`で制御するフルコンボ演出はBAD/POOR発生後に表示されない。

skin catalogのJSON候補はinclude・条件・数値正規化後に数値の`type`を明示した文書だけを受理する。
これによりskin配下のplayer dataやparts JSONが既定値`type=0`のPlay 7KEYS skinとして
候補へ混入することを防ぐ。Selectの曲行テキストIDは区切り文字を無視して`bartext`を
照合し、`bar_text` / `folderbar_text`形式もbar textへ割り当てる。Lua skinのslider / graph
で使う`isRefNum`はJSONのbooleanに加えて互換表現の整数`0` / `1`も受理する。

### Lua Module Path

skin library root 配下の Lua skin へ公開する初期 `package.path` は、過去の beatoraja と同じ
`?.lua;skin/<skin entry directory>/?.lua` 形式にする。sandbox 内の実ファイル解決はこの論理
パスを library root に対応付け、`require` / `dofile` / `loadfile` が root 外へ出ないことを
別途検証する。library root を持たない低レベル API は、従来どおり entry directory の絶対
パスを使う fallback を維持する。

## Skin Type

beatoraja 互換の主な play skin type:

| type | meaning |
| ---: | --- |
| 0 | 7KEYS |
| 1 | 5KEYS |
| 2 | 14KEYS |
| 3 | 10KEYS |
| 4 | 9KEYS |
| 12 | 7KEYS BATTLE |
| 13 | 5KEYS BATTLE |
| 14 | 9KEYS BATTLE |
| 16 | 24KEYS |
| 17 | 24KEYS DOUBLE |
| 18 | 24KEYS BATTLE |

BMZ 独自拡張:

| type | meaning | status |
| ---: | --- | --- |
| 19 | reserved | 未使用 |
| 20 | reserved | 未使用 |
| 21 | 2KEYS | 予約のみ |
| 22 | 4KEYS | 実装済み |
| 23 | 6KEYS | 実装済み |
| 24 | 8KEYS | 実装済み |

`22=4KEYS`, `23=6KEYS`, `24=8KEYS` は Scratch なしの `Key1..KeyN` 用 play skin type。
beatoraja には対応する skin type が無いため、BMZ 専用 skin として扱う。

`#8K` BMS の U_E/BMSE 系 channel は、7K の表示順 `S1234567` を 8K の
`Key1..Key8` として扱う。つまり `Scratch -> Key1`, `Key1 -> Key2`, ...,
`Key7 -> Key8` に正規化してから skin へ渡す。

## Battle Session Skin

`SessionMode::AutoplayBattle` / `SessionMode::GBattle` では、5K は skin type `13`、7K は
skin type `12` の専用スロット (`skin.battle5` / `skin.battle7`) を優先する。
未設定なら同じ譜面モードの通常スロット (`skin.play5` / `skin.play7`) へフォールバックする。
5K/7K以外のG-BATTLEは通常play skinを使う。

SessionMode の battle は、Select上のkey modeとスキン選択キーを5K/7Kのまま維持する。
Play内部ではbattle skinへ2P側レーンと対戦表示を公開するが、通常のDP OPTION `BATTLE` の
10K/14Kスキン選択とは区別する。

2P側のノート、LN、key-on/off、hold、bomb、HCN、judge は通常の2Pレーン ref/timerで
公開する。次の対になっているtimerは1P/2Pを別状態として扱う。

| 1P | 2P | meaning |
| ---: | ---: | --- |
| 42 | 43 | gauge increase |
| 44 | 45 | gauge max |
| 46 | 47 | judge |
| 446 | 447 | combo |
| 48 | 49 | full combo |
| 143 | 144 | end of note |

対戦側の進行中 EX score / max combo / BP / judge counts は、beatoraja の rival number
ref `271`, `275`, `276`, `280..284` に公開し、judge rate は `285..289` に公開する。
G-BATTLE の相手判定は
表示専用で、1P側のスコア、ゲージ、キー音、保存用リプレイには混ぜない。

## Static Image Sources

beatoraja skin の `source.path` は PNG / BMP / JPEG / GIF / TGA に加え、libGDX
`PixmapIO` の CIM を読み込める。CIM は zlib stream 内の width / height /
Gdx2DPixmap format と pixel buffer を RGBA8 へ展開する。MILLIONDOLLAR RESULT の
主要 atlas は配布時点から `.cim` のため、PNG fallback へ書き換えずそのまま扱う。

## Play Gauge Source Layout

`gauge.nodes` が 4 / 8 / 12 枚だけ定義された skin は、beatoraja の
`JsonSkinObjectLoader` と同じ index map で 36 段階へ展開する。これにより gauge type ごとの
通常色・警告色・点滅色を取り違えずに表示する。36 枚定義は従来どおり直接対応させる。

## PMchara

JSON / Lua play skin の `pmchara` を decode し、source が指すディレクトリまたは `.chp` を
Shift_JIS で読み込む。type `0..15`、`Pattern` / `Texture` / `Layer`、frame 時間、loop 開始、
source / destination 矩形、alpha、angle を通常の skin source と render item へ展開する。
type 0 は通常・判定・曲終了状態からモーションを選び、type 1..15 は beatoraja の固定
モーション対応を使う。filepath のユーザ選択と既定値は通常画像と同じ優先順位で解決する。

現状は PMchara の `--` 座標補間と、画像右下色を透過色にする chroma-key 処理には未対応。
PNG 等が持つ alpha は通常どおり反映する。

## Text Overflow

beatoraja互換のtext `overflow` は次の値を使用する。

| `overflow` | behavior |
| ---: | --- |
| `0` | destination幅を超えて描画する |
| `1` | destination幅へ収まるよう縮小する |
| `2` | destination幅へ収まる位置で文字列を打ち切る |

`overflow: 1` の既定動作はbeatorajaと同じ横方向のみの縮小で、文字の高さとY位置を
維持する。BMZ拡張の `shrinkMode: 1` を指定すると、従来のBMZと同じ縦横等比縮小へ
切り替わり、縮小後のtextはdestination内で上下中央に配置される。

| `shrinkMode` | behavior |
| ---: | --- |
| 省略 / `0` / その他 | 横方向のみ縮小（beatoraja互換） |
| `1` | 縦横等比縮小（BMZ拡張） |

`shrinkMode` は `overflow: 1` かつ `wrapping: false` の場合だけ参照する。
`wrapping: true` は縮小より優先される。JSON skinとLua skinで同じフィールドを使用できる。

```json
{
  "text": [
    { "id": "beatoraja-title", "size": 30, "overflow": 1 },
    { "id": "uniform-title", "size": 30, "overflow": 1, "shrinkMode": 1 }
  ]
}
```

## BMZ Default JSON Skin

`data/skins/default/` のデフォルトスキンは JSON skin document を主経路にする。
選曲 / 決定 / リザルトは `select.json`, `decide.json`, `result.json`、プレイ画面は
key mode ごとに `play4.json` / `play5.json` / `play6.json` / `play7.json` /
`play8.json` / `play9.json` / `play10.json` / `play14.json` を読む。

BMZ default JSON では、digit atlas を同梱せずに既存フォントで数値を表示するため、
`text` 要素に BMZ 拡張の `numberRef`, `judgeRegion`, `judgeColor`, `judgeTimingRegion`,
`judgeTimingColor`, `prefix`, `suffix` を使える。
`numberRef` は既存の `value.ref` と同じ `SkinDrawState` number ref を文字列化し、
未取得時は空文字として扱う。外部 beatoraja JSON skin の `value` sprite 表示は従来通り。
`judgeRegion` は最新判定の表示領域 index (通常は `0`) を文字列化し、判定タイマーが
非アクティブなら空文字として扱う。`judgeColor` は `judgeRegion` 用の表示色を判定種別で
切り替え、PGREAT は水色、GREAT / GOOD は黄色、BAD / POOR / EMPTY POOR は赤で表示する。
`judgeTimingRegion` は同じ判定領域の FAST / SLOW だけを文字列化し、`judgeTimingColor` で
FAST を青、SLOW を赤に切り替える。

### BMZ Arrange Refs

beatoraja 互換の `ref` / `event_index` / `number` `42` (1P RANDOM) と `43` (2P RANDOM)
は既存スキン互換のため 0..9 の値を返す。BMZ 独自 ARRANGE の `F-RANDOM` と
`MF-RANDOM` は、非対応 beatoraja skin で option panel が崩れないよう、`42` / `43`
では `RANDOM` と同じ `2` として扱う。

BMZ 対応 skin で新 ARRANGE を区別したい場合は、BMZ 拡張 ref を使う。

| ref | meaning |
| ---: | --- |
| 344 | 1P ARRANGE extended index |
| 345 | 2P ARRANGE extended index |

extended index は beatoraja 互換値 `0=NORMAL`, `1=MIRROR`, `2=RANDOM`, `3=R-RANDOM`,
`4=S-RANDOM`, `5=SPIRAL`, `6=H-RANDOM`, `7=ALL-SCR`, `8=RANDOM-EX`,
`9=S-RANDOM-EX` に加えて、`10=F-RANDOM`, `11=MF-RANDOM` を返す。

### BMZ Attempt Session Mode Ref

beatoraja 互換の assist `ref` / `event_index` `73` は従来どおり 2 値を返す。
`NORMAL` / `PRACTICE` / `G-BATTLE`は`0`、`AUTOPLAY` / `AUTOPLAY BATTLE`は`1`とし、
既存 skin の 2 行 option panel を崩さない。

BMZ 対応 skin で5種類を区別する場合は、BMZ 拡張 ref `1970` を使う。
`number(1970)` / `event_index(1970)` は `0=NORMAL`, `1=PRACTICE`, `2=AUTOPLAY`,
`3=AUTO BATTLE`, `4=G-BATTLE` を返す。BMZ デフォルトスキンのplay mode panelも
このrefを使用する。Selectではこれから開始するモード、Decide / Play / Resultでは
試行開始時に固定したモードを返す。

### Practice Configuration Position

beatoraja JSON / Lua play skin の `practice` オブジェクトをdecodeする。`practice.id` と同じ
`destination.id` の先頭 `dst` にある `x` / `y` / `h` を、BMZのプラクティス設定ウィンドウの
初期位置として使う。指定がないskinでは画面左上に配置する。設定内容はBMZのeguiウィンドウで
表示し、ウィンドウは初期位置からドラッグ移動できる。

### Play Gauge Type Ref

`ref=44`は、Lua skinのimageset `value`が`main_state.gauge_type()`を返す場合に、BMZが
JSONのimage indexへ変換するための内部bridgeである。beatorajaの公開NumberPropertyや
スコア互換Rule Modeではなく、image / imagesetの`ref`専用として扱う。値は現在適用中の
gauge typeで、`0=ASSIST EASY`, `1=EASY`, `2=NORMAL`, `3=HARD`, `4=EX HARD`,
`5=HAZARD`, `6=CLASS`, `7=EX CLASS`, `8=EX HARD CLASS`。JSON `gauge.type`の
ゲージアニメーション種別とも別の値である。

### BMZ Rule Mode / LN Policy Refs

スコア互換ルール、選曲設定のLN方針、ScoreKeyへ正規化されたLN方針を別の状態として公開する。
JSON skinではnumber/value・imagesetの`ref`、textの`ref`、destinationの`op`から参照できる。
Lua skinでは`main_state.number()` / `event_index()` / `text()` / `option()`から同じ値を参照できる。

Rule Modeは次のindexとlabelを使う。

| index | text | exact option |
| ---: | --- | ---: |
| 0 | `BEATORAJA` | 1988 |
| 1 | `LR2ORAJA` | 1989 |
| 2 | `DX` | 1990 |

`ref=1987`はnumber / event index / imageset refでは上表のindex、textではlabelを返す。
Selectでは現在のprofile設定、Decide / Playでは開始する試行のScoreKey、Resultでは終了した
単曲またはコース試行に保存されたRule Modeを使う。Replayとコースでもprofileの現在値へ
読み替えず、試行開始時に固定された値を維持する。

LN方針のindexとlabelは、設定値と正規化済みScoreKeyで共通の次の順序を使う。

| index | text | setting exact option | score policy exact option |
| ---: | --- | ---: | ---: |
| 0 | `AUTO(LN)` | 19161 | 19151 |
| 1 | `AUTO(CN)` | 19162 | 19152 |
| 2 | `AUTO(HCN)` | 19163 | 19153 |
| 3 | `FORCE(LN)` | 19164 | 19154 |
| 4 | `FORCE(CN)` | 19165 | 19155 |
| 5 | `FORCE(HCN)` | 19166 | 19156 |

`ref=19160`はSelectで現在のprofileに設定された`LnPolicySetting`を返す。譜面内容による
AUTO解決前の値で、19167はAUTO系、19168はFORCE系のときtrueになる。Select以外では
settingのnumber / imageset ref / textは値なし、setting optionはすべてfalseになる。

`ref=19150`は譜面内容、コース制約、Replay情報を反映してScoreKeyへ正規化された
`LnScorePolicy`を返す。19157はAUTO系、19158はFORCE系、19159は値を取得できるときtrueになる。
Selectの曲行では選択譜面から正規化し、コース行ではコース共通ScoreKeyを使う。フォルダ・設定行
では値なし。Decide / Play / Resultではその試行に固定されたScoreKeyを使う。値なしの場合、
number / imageset refは未取得、textは空文字、exact / AUTO / FORCE optionはfalseになる。
`event_index()`だけはAPI仕様上`0`へフォールバックするため、未取得とAUTO(LN)を区別する場合は
必ず`option(19159)`または`op: [19159]`を併用する。

既存のbeatoraja互換`ref=308`は、設定やScoreKeyではなく、変換後の実効譜面を描画する
`0=LN`, `1=CN`, `2=HCN`の3値を引き続き返す。AUTO/FORCEの区別には19160または19150を使う。

LNの有無はbeatoraja互換option `172=OPTION_NO_LN` / `173=OPTION_LN`で判定する。
Selectでは選択中のライブラリ譜面、Decide / PlayではLN policy適用後の開始譜面、Resultでは
終了した実効譜面を使う。フォルダ・設定行や開始譜面未確定時は172/173の両方がfalseになる。
Selectで個数も必要な場合は`ref=351`（通常LN数）と`ref=353`（ロングスクラッチ数）を使う。
`ref=308`はLNが存在しない場合でもLN種別indexを返し得るため、LN有無の判定には使わない。

### BMZ Score Grade Refs

ランク差分の標準表示は固定NEXTとする。`NUMBER_NEXT_RANK_EXSCORE` (`ref=154`) は、
次の正式な DJ LEVEL 境界までの距離を `次境界 - 現在のEX SCORE` で返すため、未到達時は
正、MAX時は0になる。select / play / result のいずれでも、現在のEX SCOREと譜面全体の
ノート数を使用する。スコアが無いselect行または総ノート数0では値を返さない。

総ノート数を `N`、MAX EX SCOREを `M=2N` とし、境界は整数演算で次のように求める。

| grade | minimum EX SCORE |
| --- | ---: |
| F | `0` |
| E | `ceil(2M/9)` |
| D | `ceil(3M/9)` |
| C | `ceil(4M/9)` |
| B | `ceil(5M/9)` |
| A | `ceil(6M/9)` |
| AA | `ceil(7M/9)` |
| AAA | `ceil(8M/9)` |
| MAX | `M` |

beatorajaの実装に含まれる浮動小数点の丸め誤差、正式なDJ LEVELではない`1/9`境界、
play中だけ経過ノート数を差分量に使う挙動は引き継がない。現在値が境界と一致する場合、
NEXTはさらに上の境界を指す。MAXではMAXを指して差分0を返す。

BMZ拡張は次をselect / play / result skin共通で公開する。grade indexは
`0=F`, `1=E`, `2=D`, `3=C`, `4=B`, `5=A`, `6=AA`, `7=AAA`, `8=MAX`。
1974〜1976はnumberに加えて同じ値をevent index / imageset refとして使用でき、textでは
grade labelを返す。

| ref / option | kind | meaning |
| ---: | --- | --- |
| 1974 | number / event index / text | 現在到達済みのgrade |
| 1975 | number / event index / text | 現在より高い次のgrade。MAX時はMAX |
| 1976 | number / event index / text | 最も近いgrade |
| 1977 | number | `EX SCORE - current border` |
| 1978 | number | `EX SCORE - next border`。符号付き。正値の`ref=154`とは独立 |
| 1979 | number | `EX SCORE - nearest border`。次側なら負 |
| 1980 | number | `abs(ref 1979)` |
| 1981 | option | NEARESTが現在側。完全一致と同距離を含む |
| 1982 | option | NEARESTが次側 |
| 1983 | option | grade境界に完全一致 |
| 1984 | option | 現在側と次側から同距離 |
| 1985 | option | grade計算に必要なスコアが存在する |

NEARESTは現在側までの距離が次側以下なら現在側を選ぶため、同距離では現在側になる。
別のtie policyが必要なLua skinは1977と`abs(1978)`を比較して独自に選択できる。JSON skinは
1976をimageset ref、1980を差分値、1981 / 1982を符号画像のopとして利用できる。
スコア無しでimagesetの先頭画像へフォールバックさせたくない場合は、destinationに
`op: [1985]`を指定する。

削除済みの表示設定で使っていた1971〜1973は再利用しない。旧BMZ skin向けに
`number(1971)=1`, `option(1972)=false`, `option(1973)=true`の固定NEXT値を返す。

### First Play Compatibility and BMZ Option

Play / Result skinの初回プレイ値はbeatoraja互換として扱う。保存済みスコアが無い場合、
`NUMBER_HIGHSCORE` (`ref=150`) と `NUMBER_HIGHSCORE2` (`ref=170`) は`0`を返す。
Resultの前回ミスカウント (`ref=176`) とミスカウント差分 (`ref=178`) は通常のnumber表示では
非表示のまま、Luaの`main_state.number()`では`Integer.MIN_VALUE` (`-2147483648`)を返す。
そのため既存Lua skinは`main_state.number(176) < 0`で初回を判定できる。Resultの前回ランクは
初回時にFとして扱い、option `327`が有効になる。

値が`0`の保存済みスコアと未プレイを明確に区別するため、BMZ拡張option `1986`を公開する。
Play開始時に現在のScoreKey（SHA256、LN方針、DP区分、rule mode）に対応する保存済みベストが
存在しなければtrueになる。Resultでも保存前の状態を維持する。JSON skinは`op: [1986]`、
Lua skinは`main_state.option(1986)`で参照でき、`op: [-1986]`は既プレイ時に有効になる。

### Result Autoplay Options

beatorajaの `OPTION_AUTOPLAYOFF` (`32`) / `OPTION_AUTOPLAYON` (`33`) はプレイ画面でのみ
評価されるが、BMZではAUTO PLAYでもリザルト画面を表示するため、Result skinにも拡張して
公開する。AUTO PLAYのResultでは33がtrue、32がfalseになり、通常プレイでは逆になる。
JSON skinは`op: [33]`、Lua skinは`main_state.option(33)`でAUTO PLAY専用表示を定義できる。

### Select Rival and Chart Replication

beatoraja互換の選曲イベントを次のように扱う。

| event id | action |
| ---: | --- |
| 10 | 難易度フィルターを巡回（309と同じ） |
| 14 | F1メニューのスキン設定を開く |
| 17 | 選択譜面フォルダの `.txt` を開く |
| 57 | HS倍率を設定刻みで増減 |
| 59 | ノーツ表示時間を1msずつ増減（`1..=10000`） |
| 74 | キーモード別の表示タイミングを1msずつ増減（`-500..=500`） |
| 79 | primary IRに属するライバルを `NONE → 登録順` で切替 |
| 210 | 選択譜面／コースのprimary IRページを開く |
| 211 | 現在フォルダをF5相当で更新 |
| 212 | 選択譜面または通常曲フォルダをファイルブラウザで開く |
| 213 | 選択譜面の `url` / `appendurl` をブラウザで開く |
| 343 | GUIDE SEを切替 |
| 344 | `NONE → RIVALCHART → RIVALOPTION` で譜面再現モードを切替 |
| 350 | EXTRA NOTE DEPTHを4状態で巡回 |
| 351 | MINEモディファイアを5状態で巡回 |
| 352 | SCROLLモディファイアを3状態で巡回 |
| 353 | LONGNOTEモディファイアを6状態で巡回 |
| 400 | CONSTANTを切替 |

イベント引数が正なら増加／次、負なら減少／前として扱う。`event_index(10)`、
`event_index(343)`、`event_index(350..353)`、`event_index(400)` と同じ番号の
image / imageset refから現在値を取得でき、option 400はCONSTANT有効時にtrueとなる。
イベント360/361（7→9KEY変換）は未対応。

イベント79で選んだライバルに選択譜面の
スコアがあれば、プレイ開始時のターゲットへ自動設定する。未プレイ譜面では通常の
TARGET設定を使う。名前の表示と比較スコアの有無は独立して扱う。

### ターゲット名のtext ref (BMZ仕様)

Select / Play / Resultで次の意味に統一する。beatorajaのPlay / Resultでref 1とref 3が
同じ名前を返す仕様とは異なる。

| ref | 内容 |
| --- | --- |
| 1 (`STRING_RIVAL`) | 選択中ライバルの名前を優先し、なければ確定したターゲットの名前。どちらもなければ空文字 |
| 3 (`STRING_TARGET`) | 選択中のターゲット設定名 (`RANK AAA`, `IR TOP`, `IR NEXT`等)。未選択は`NO TARGET` |

ref 1の選択ライバル名は、その譜面のスコアがなくても維持する。G-BATTLEではそのプレイで
選択した対戦相手を優先する。固定ランク目標は設定時に名前が確定し、IR系目標は取得した
ランキングの対象者名を使う。IR未取得・取得失敗・該当データなしの場合、選択ライバルが
なければref 1は空文字のままにする。名前とEXスコアは同じランキング行から解決し、
取得がプレイ開始に間に合わなかった場合も表示へ反映する。
Resultはプレイ時の名前とターゲット設定を保持し、リザルトのランキング取得や選曲側の
設定変更で置き換えない。JSON refとLuaの`main_state.text`は同じ規則で評価する。

譜面再現モード名はbeatoraja互換の `STRING_CHART_REPLICATION_MODE` (`ref=86`) と、
BMZの動的text id `bmz_select_chart_replication` から `NONE` / `RIVALCHART` /
`RIVALOPTION` として取得できる。`RIVALOPTION`は配置種別だけ、`RIVALCHART`は配置種別と
ライバルのside別24bit seedを適用する。ghostは使用しない。

### BMZ Dynamic Select Option Panels

選曲オプションの現在値は、状態ごとの画像セルを用意せず `text.id` から直接描画できる。
ラベルは select snapshot の文字列を使うため、ARRANGE などへ項目を追加しても
skin 側のスプライト行追加は不要。

| text id | value |
| --- | --- |
| `bmz_select_arrange` / `bmz_select_arrange_2p` | 1P / 2P ARRANGE |
| `bmz_select_target` | target |
| `bmz_select_gauge` | gauge |
| `bmz_select_gauge_auto_shift` | gauge auto shift |
| `bmz_select_bottom_shiftable_gauge` | bottom shiftable gauge |
| `bmz_select_double_option` | DP option |
| `bmz_select_hs_fix` | hi-speed fix |
| `bmz_select_assist` | assist |
| `bmz_select_mode` | mode |
| `bmz_select_sort` | sort |
| `bmz_select_ln_mode` | LN mode |
| `bmz_select_bga` | BGA |
| `bmz_select_chart_replication` | chart replication (`NONE` / `RIVALCHART` / `RIVALOPTION`) |
| `bmz_select_judge_timing_auto_adjust` | judge timing auto adjust (`ON` / `OFF`) |

BMZ 拡張の `panel` は画像を使わない単色矩形で、`color`, `borderColor` は
`RRGGBB` / `RRGGBBAA`、`borderWidth` は skin canvas pixel で指定する。
destination に `act` / `click` を置くと、text や panel も image / imageset と同じ
イベント対象になる。`clickable: false` は destination 自身と同名 image / imageset の
クリックを明示的に無効化する。

中央・右揃えtextの destination `x` は描画上の基準点だが、イベント領域の `x` は
常に左端として扱う。操作可能な中央揃えtextは、描画用textと透明panelのdestinationを
分け、panel側へ元のセル矩形と `act` / `click` を指定する。

```json
{
  "panel": [{
    "id": "arrange-hit",
    "color": "00000000"
  }],
  "text": [{
    "id": "bmz_select_arrange",
    "font": "option-font",
    "size": 18,
    "align": 1,
    "overflow": 1
  }],
  "destination": [
    {
      "id": "bmz_select_arrange",
      "dst": [{ "x": 93, "y": 10, "w": 166, "h": 19 }]
    },
    {
      "id": "arrange-hit",
      "act": 42,
      "click": 2,
      "dst": [{ "x": 10, "y": 10, "w": 166, "h": 19 }]
    }
  ]
}
```

### BMZ Select Settings Rows

設定入口、設定カテゴリ、`戻る`、`閉じる` は、通常の検索フォルダや曲フォルダとは
独立した `SelectRowKind` として skin へ渡す。設定ルートの一覧先頭は `閉じる`、
設定カテゴリ内の一覧先頭は `戻る` になる。

beatoraja互換の選曲イベント `13` (`KEY CONFIG`) は、現在の選曲フォルダ履歴を維持したまま
`設定 > キー設定` を直接開く。キー設定ルートの `戻る` を実行すると、イベントを実行する
直前のフォルダとカーソル位置へ復帰する。

通常設定行とキー設定行の `songlist` バーテキストは `設定項目名 [設定値]` の形式で渡す。
カーソルを合わせた行の詳細表示では、タイトル (`ref=10`) に設定項目名、サブタイトル
(`ref=11`) に設定項目自体の説明文、アーティスト (`ref=14`) に現在値を渡す。説明文には
設定項目名、現在値、設定操作の案内を含めない。編集中表示 `[編集中]` はジャンル
(`ref=13`) に渡し、サブアーティスト (`ref=15`) は空にする。カテゴリ、`戻る`、`閉じる`
には設定値や説明文を付加しない。

BMZ 対応 select skin は `songlist` の配列末尾へ次の専用 slot を追加できる。
index は 0 始まり。`image` は songlist 用 imageset の `images`、
`text` は `songlist.text` の destination index を表す。

| row | image index | text index | slot がない既存 skin での fallback |
| --- | ---: | ---: | --- |
| 設定入口 / 設定カテゴリ | 8 | 11 | 入口は検索フォルダ `6 / 10` → 曲フォルダ `1 / 4`、カテゴリは曲フォルダ `1 / 4` |
| `戻る` | 9 | 12 | 検索フォルダ `6 / 10` → 曲フォルダ `1 / 4` |
| `閉じる` | 10 | 13 | 検索フォルダ `6 / 10` → 曲フォルダ `1 / 4` |

専用 slot が配列長の外なら表の順に fallback し、曲フォルダ slot もなければ先頭 slot を使う。
このため、既存 skin は変更しなくても従来と同じ画像・文字 destination で表示される。
image index `7` は既存 skin が no-song bar に使用するため、BMZ 専用 image slot は `8` から始める。

行種別を destination 条件や imageset の選択に使う場合は次の BMZ ref / option を使う。

| ref / option | kind | meaning |
| ---: | --- | --- |
| 1960 | number / event_index | `0=設定以外`, `1=設定入口または設定カテゴリ`, `2=戻る`, `3=閉じる` |
| 1961 | option | 設定入口または設定カテゴリ |
| 1962 | option | `戻る` |
| 1963 | option | `閉じる` |

beatoraja 互換の `OPTION_FOLDERBAR` (`op: 1`) は、設定入口・設定カテゴリ・`戻る`・
`閉じる`のいずれでも引き続き真になる。

### BMZ RANDOM Lane Refs

beatoraja result skin互換の `450..469` は、BMZではplay/select skinにも拡張して公開する。
配列は `pattern[表示先レーン] = 元レーン` で保持し、skinへは各sideのレーン番号を
1始まりで返す。

| ref | meaning |
| ---: | --- |
| 450..458 | 1P key 1..9 の元レーン番号 |
| 459 | 1P Scratch の元レーン番号 |
| 460..466 | 2P key 1..7 の元レーン番号 |
| 469 | 2P Scratch の元レーン番号 |

`467` / `468` はbeatorajaに対応する定義がないため未使用。resultでは従来どおり
RANDOM / R-RANDOM / RANDOM-EX（ScratchはRANDOM-EXのみ）の固定配置を返す。
playでは現在プレイ中の同じ固定配置を返す。selectでは「このまま開始した場合に
適用する予定の配置」を返し、通常の未抽選RANDOMや予定配置がない場合は全て `0`。
リプレイや将来のライバル配置コピーなど、選曲中に配置が確定している機能が
select snapshotへ予定配置を設定する。

### BMZ Hispeed Mode Refs

`1900..1996` はBMZ共通extension、`1997..1999`はLR2変換のJudge Detail用に予約し、
`19000`以降はBMZの高位extensionとして扱う。beatoraja互換ref / option / timerと
衝突させないため、BMZ独自状態はこれらの管理済み範囲へ追加する。

| ref | kind | meaning |
| ---: | --- | --- |
| 1900 | number / event_index / text | current HS mode。number / event_indexは `0=base`, `1=Floating`、textは `CHS` / `NHS` / `FHS` |
| 1901 | number / option | Floating active flag。`0=OFF`, `1=ON` |
| 1902 | number | target green number。Floating時は固定target、それ以外は現在green number |
| 1916 | number / event_index / text | base HS。`0=Classic`, `1=Normal`、textは `CHS` / `NHS` |
| 1917 | number / event_index | Normal HS level (`1..=20`) |
| 1918 | number / event_index / text | HS設定。`0=NORMAL`, `1=CLASSIC`, `2=FLOATING`, `3=NORMAL+FLOATING`, `4=CLASSIC+FLOATING` |

`op: [1901]` または `draw: "number(1901)==1"` でFloating時だけdestinationを表示できる。

### BMZ Key Mode Refs

選曲中は曲行の譜面、決定・プレイ・リザルトでは実際のプレイ譜面から値を得る。
フォルダ行や設定行では、譜面モードの number / option を無効として扱う。

| ref / option | kind | meaning |
| ---: | --- | --- |
| 1903 | number / event_index | key mode (`4`, `5`, `6`, `7`, `8`, `9`, `10`, `14`) |
| 1904 | number | Scratch を含む実レーン数 |
| 1905..1912 | option | 順に 4K / 5K / 6K / 7K / 8K / 9K / 10K / 14K 完全一致 |
| 1913 | option | Scratch なし (4K / 6K / 8K / 9K) |
| 1914 | option | single play (5K / 7K) |
| 1915 | option | double play (10K / 14K) |

`1903..1915`は、Selectでは現在の設定を適用した場合、Decide / Play / Resultでは実際に
開始した試行の**実効key mode**を返す。SelectのSessionMode `AUTO BATTLE` / `G-BATTLE`
は5K/7Kのまま維持し、7K→6K変換と通常のDP OPTION `BATTLE`だけを反映する。

### BMZ Source Chart Refs

変換前の譜面情報と実効譜面を区別するため、次の高位extensionを公開する。

| ref / option | kind | meaning |
| ---: | --- | --- |
| 19180 | number / event_index / imageset ref | 変換前key mode (`4`, `5`, `6`, `7`, `8`, `9`, `10`, `14`) |
| 19181..19188 | option | 変換前key mode。順に4K / 5K / 6K / 7K / 8K / 9K / 10K / 14K |
| 19189 | option | 7K→6K変換をこの試行へ適用した |
| 19190 | number / event_index / imageset ref | 変換前LN profile bitmask (`0..15`) |
| 19191 | option | 未定義LNを含む (`bit 0`) |
| 19192 | option | 定義済みLNを含む (`bit 1`) |
| 19193 | option | 定義済みCNを含む (`bit 2`) |
| 19194 | option | 定義済みHCNを含む (`bit 3`) |
| 19195 | option | 上記LN種別を2種類以上含む |
| 19196 | option | 変換前LN profileを取得済み |

`19190`はLN policy、コース制約、7K→6K、譜面オプションを適用する前に固定する。
LNがない譜面も取得済みなら`19190=0`かつ`19196=true`になるため、未取得状態と区別できる。
Selectでは曲行だけ取得でき、フォルダ・設定・未解決コース行では値なし。Decide / Play /
Resultでは同じ試行値を維持する。実効key modeは`1903`、実効LN種別はbeatoraja互換
`308`、実効LN有無はoption `172/173`を使う。

### Beatoraja Attempt Property Compatibility

次のbeatoraja互換index refはSelectの現在設定だけでなく、Decide / Play / Resultでは
実際に開始した試行の値を返す。JSONのimage / imageset `ref`、Luaの`event_index()`、
対応するindex propertyで共通の値を使う。NumberPropertyとIDが重なる場合はbeatorajaの
NumberProperty側の意味を優先する（例: Selectの`number(78)`はclear count）。

| ref | meaning |
| ---: | --- |
| 54 | 適用済みDP option (`0=OFF`, `1=FLIP`, `2=BATTLE`, `3=BATTLE AS`) |
| 55 | HSFIX (`0=OFF`, `1=START`, `2=MAX`, `3=MAIN`, `4=MIN`) |
| 78 | gauge auto shift (`0=OFF`, `1=CONTINUE`, `2=HARD TO GROOVE`, `3=BEST CLEAR`, `4=SELECT TO UNDER`) |
| 308 | 実効LN種別 (`0=LN`, `1=CN`, `2=HCN`) |
| 340 | judge algorithm (`0=COMBO`, `1=DURATION`, `2=LOWEST`) |
| 341 | bottom shiftable gauge (`0=OFF`, `1=EASY`, `2=NORMAL`) |

譜面プロパティoptionもSelect / Decide / Play / Resultで同じ実効譜面を参照する。

| option pair | meaning |
| ---: | --- |
| 170 / 171 | BGAなし / あり |
| 172 / 173 | LNなし / あり |
| 176 / 177 | BPM変化なし / あり |
| 178 / 179 | BMS `#RANDOM`系列なし / あり |

Selectで譜面行が選ばれていない場合は、これらの正負optionをどちらもfalseにする。

### BMZ Logical Input Refs

| option | timer | logical input |
| ---: | ---: | --- |
| 1920 | 19000 | E1 |
| 1921 | 19001 | E2 |
| 1922 | 19002 | E3 |
| 1923 | 19003 | E4 |
| 1924 | 19004 | UI Left |
| 1925 | 19005 | UI Right |
| 1926 | 19006 | UI Up |
| 1927 | 19007 | UI Down |

option は論理入力の押下中、timer は直近の論理入力 press edge からの経過 ms を返す。
同じ論理入力に複数の物理キーを割り当てた場合は OR 集約し、押下中に別キーを追加しても
timer を再起動しない。scene 入場時から押されている入力は press edge として扱わない。
E1 は設定済み E1 と legacy Start、E2 は設定済み E2 と legacy Select を含む。

E1/E2共通パネル向けには次の集約状態を使う。

| option | press / hold timer | release timer | aggregate input |
| ---: | ---: | ---: | --- |
| 1928 | 19008 | 19009 | E1 または E2 |

option 1928はE1/E2のどちらかを押している間true。timer 19008は最初の一方を押した
false-to-true edgeで0msから始まり、どちらかを押している間だけ有効になる。もう一方を追加で
押しても再起動しない。両方を離すと19008はoffになり、timer 19009が0msから始まる。
次の共通pressで19009はoffになる。scene入場時から押されている入力ではどちらも発火しない。

入場後の表示を押下中に維持し、release後に退場させる場合は、同じ見た目を別IDで定義して
次のように組み合わせる。

```json
{
  "destination": [
    {
      "id": "hs-panel-held",
      "timer": 19008,
      "loop": 120,
      "op": [1928],
      "dst": [
        { "time": 0, "x": 80, "y": 100, "w": 400, "h": 120, "a": 0 },
        { "time": 120, "x": 100, "a": 255 }
      ]
    },
    {
      "id": "hs-panel-release",
      "timer": 19009,
      "loop": -1,
      "op": [-1928],
      "dst": [
        { "time": 0, "x": 100, "y": 100, "w": 400, "h": 120, "a": 255 },
        { "time": 180, "x": 80, "a": 0 }
      ]
    }
  ]
}
```

`runtimeEvent` に `triggerAction` を指定すると、Lua で入力状態を取得せずに runtime flag を
切り替えられる。値は `e1_press`, `e2_press`, `e3_press`, `e4_press`,
`ui_left_press`, `ui_right_press`, `ui_up_press`, `ui_down_press`。

```json
{
  "runtimeFlag": [{ "id": 1, "initial": false }],
  "runtimeEvent": [{
    "id": -20001,
    "toggleFlags": [1],
    "triggerAction": "e1_press"
  }]
}
```

### BMZ Scratch / Keys Judge Refs

プレイ中の最新判定を、判定領域ごとに Scratch と鍵盤へ分けて公開するBMZ拡張。
slotは `region * 2 + lane kind` の順で、Scratch判定はScratch側だけ、鍵盤判定は鍵盤側だけを
更新する。両方のtimerは同時に有効にでき、互いの判定では再起動しない。

| 判定領域 / lane kind | timer | PGREAT option | FAST / EARLY option | SLOW / LATE option | タイミング差 ref |
| --- | ---: | ---: | ---: | ---: | ---: |
| region 0 / 1P Scratch | 19010 | 19020 | 19030 | 19040 | 19050 |
| region 0 / 1P Keys | 19011 | 19021 | 19031 | 19041 | 19051 |
| region 1 / 2P Scratch | 19012 | 19022 | 19032 | 19042 | 19052 |
| region 1 / 2P Keys | 19013 | 19023 | 19033 | 19043 | 19053 |
| region 2 / 3P Scratch | 19014 | 19024 | 19034 | 19044 | 19054 |
| region 2 / 3P Keys | 19015 | 19025 | 19035 | 19045 | 19055 |

判定領域は既存の `timer 46/47/247` と同じく、skinの `judge[].index` から得た領域数と
beatoraja互換のレーン分割で決まる。`1P` / `2P` / `3P` は物理プレイヤー数ではなく、
それぞれregion 0 / 1 / 2の別名。Scratchは `Lane::Scratch` / `Lane::Scratch2`、それ以外の
レーンはKeysに分類するため、ScratchのないモードではScratch側timerは開始しない。

timerは該当チャンネルの最新判定からの経過ms。標準判定表示に使う800msの
`recent_judgements`保持期間とは別に、プレイ中の最後の値をrenderer runtimeへ保持する。
表示時間はdestinationの `dst` と `loop` で決める。プレイ開始、リトライ、次曲開始、
skin runtimeのresetでは全チャンネルを未開始へ戻す。

FAST/SLOW optionとタイミング差refには、既存の判定表示設定と同じフィルタを適用する。

- `Auto`: PGREATのFAST/SLOW optionだけfalse。タイミング差refは返す。
- `ThresholdMs`: PGREATは閾値未満でFAST/SLOW optionをfalseにし、タイミング差refも値なしにする。
  GREAT / GOOD / BAD / POOR / EMPTY POORは閾値内でも常にFAST/SLOW optionとタイミング差refを返す。
- タイミング差refの符号は既存ref `525..527` と同じで、正がFAST、負がSLOW。

判定前はtimerがOFF、各optionがfalse、タイミング差refが値なしとなる。PGREAT optionは
最新判定がPGREATかどうかを表し、FAST/SLOWの表示フィルタとは独立する。

既存の `timer 46/47/247`、PGREAT option `241/261/361`、FAST/SLOW option
`1242/1243/1262/1263/1362/1363`、タイミング差ref `525..527` の挙動と800ms保持は変更しない。
このため拡張IDを参照しない既存skinの表示は変わらない。拡張IDはbeatorajaおよび古いBMZでは
利用できない。

### Modified LR2 FAST/SLOW Refs

LR2プレイスキンのdecode時に、改造LR2 / OpenLR2のFAST/SLOW拡張refを次のBMZ状態へ変換する。
JSON / Lua skinの同名refは変換せず、beatorajaの意味を維持する。

| LR2 ref | decode後 | meaning |
| ---: | ---: | --- |
| 210 | 19170 | 最新の1P判定。`0=非表示`, `1=FAST`, `2=SLOW`。PGREATは常に`0` |
| 212 | 423 | FAST合計。GREAT / GOOD / BAD / POOR / EMPTY POORを集計 |
| 214 | 424 | SLOW合計。GREAT / GOOD / BAD / POOR / EMPTY POORを集計 |

`19170`はLR2変換専用の内部ref。`212/214`は既存のBMZ `423/424`へaliasするため、
OpenLR2の原仕様と異なりPOOR / EMPTY POORも合計へ含む。

### BMZ Result IR Scope

リザルトの IR パネル (`result_panel(1)`) は、BMZ 対応 skin に限り全体ランキングと
「自分 + IR ライバル」の一覧を切り替えられる。既存 skin は全体ランキングを表示し続ける。

```json
{
  "resultIrScopeBinding": "active",
  "resultIrScopeToggle": "e1_press"
}
```

`resultIrScopeBinding` は省略時または `global` で従来互換の全体ランキング固定、`active`
では既存の IR number / text / option が選択中スコープを表示する。`resultIrScopeToggle` は
省略時または `none` で E1 切替を無効にし、`e1_press` は IR パネル表示中の E1 press edge
でスコープを切り替える。`resultIrScopeToggle` は `active` binding と組み合わせた場合だけ
有効で、ほかの組み合わせでは無効になる。

| scope | label | IR API scope |
| ---: | --- | --- |
| `0` | `RANKING` | `global` |
| `1` | `RIVAL` | `self_and_rivals` |

`RIVAL` はライバルだけではなく、自分を含む `self_and_rivals` を指す。リザルト対象や
provider が Rival scope をサポートしない場合、E1 と Rival 選択イベントは no-op になる。

以下の ref / option / クリックeventは Select IR Scope と共通である。

| ref / option | kind | meaning |
| ---: | --- | --- |
| 1964 | number / event_index / text | 選択中 scope (`0` / `1`、text は `RANKING` / `RIVAL`) |
| 1965 | option | `RANKING` 選択中 |
| 1966 | option | `RIVAL` 選択中 |
| 1967 | option | `global` を取得可能 |
| 1968 | option | `self_and_rivals` を取得可能 |
| 1969 | number | 選択中 scope 内の総人数 |

クリックイベントは `-10003=RANKING`、`-10004=RIVAL`、`-10005=toggle`。いずれも当該sceneの
`*IrScopeBinding: "active"` の skin でだけ IR scope を変更する。E1 は `-10005` と同じ動作で、
IR パネルを自動では開かない。E2 / Select の IR・グラフ切替とは独立している。

`runtimeEvent.triggerAction: "e1_press"` は runtime flag の更新専用であり、scope 切替には
使わない。対応 skin が同じ E1 trigger を定義した場合、scope 切替と runtime flag 更新は
ともに実行される。

### BMZ Select IR Scope

選曲中の曲行は、BMZ 対応 skin に限り全体ランキングと「自分 + IRライバル」の一覧を
切り替えられる。既存 skin は全体ランキングを表示し続ける。コース行・フォルダ行・設定行は
`RANKING` のみで、Rival scope は選択できない。

```json
{
  "selectIrScopeBinding": "active",
  "selectIrScopeToggle": "e3_press"
}
```

`selectIrScopeBinding` は省略時または `global` で従来互換の全体ランキング固定、`active` では
既存の IR number / text / option が選択中scopeを表示する。`selectIrScopeToggle` は省略時または
`none` で E3切替を無効にし、`e3_press` は通常の選曲表示中の E3 press edge でscopeを切り替える。
`active` binding 以外との組み合わせ、検索・設定編集・選曲オプションパネル表示中、またはRival
scope未対応時は no-op になる。E3は設定済みのE3操作だけを対象とし、Start互換は持たない。

Selectの `RIVAL` も `self_and_rivals` を指す。既存の `STRING_RIVAL` 等の単一ライバル表示と
ターゲット用のライバル順位列は変更しない。`runtimeEvent.triggerAction: "e3_press"` を同時に
定義した場合も、scope切替とruntime flag更新はともに実行される。

### BMZ Daily Statistics Refs

`score.db` の local / non-autoplay `score_history` をプロファイル単位で集計する。
`profile.toml` の `[statistics] day_start_hour = 0` で日付境界のローカル時刻を指定できる。

| ref | kind | meaning |
| ---: | --- | --- |
| 1930 / 1931 | number | play count / clear count |
| 1932..1937 | number | PGREAT / GREAT / GOOD / BAD / POOR / EMPTY POOR |
| 1938 / 1939 | number | 処理ノーツ / 完了ノーツ |
| 1940 / 1941 | number | EX score / max EX score |
| 1942 | number | rate (0..10000) |
| 1943 | number / text | rank index (`0=AAA` .. `7=F`) / rank label |
| 1944..1946 | number | score / clear / miss count の更新回数 |
| 1950..1959 | text | 当日の直近曲名 (新しい順、連続重複を除外) |

event `-10100` は表示上の日次集計を現在時刻でリセットする。score history 自体は削除しない。
MILLIONDOLLAR / m-select の既存オブジェクトID特例と仮想ファイル互換経路も継続する。

### BMZ Course Result Refs

| ref | meaning |
| ---: | --- |
| 19100 | result stage count |
| 19110..19119 | stage 1..10 EX score |
| 19120..19129 | stage 1..10 gauge (整数部) |
| 19130..19139 | stage 1..10 BP |
| 19140..19149 | stage 1..10 rate (0..10000) |

stage title は beatoraja 互換 text `150..159` を使う。WMII RESULT 用
`skin/WMII_FHD/result/courseData.json` の read-only 仮想ファイルも互換性のため併存する。

## Profile Slots

profile の `[skin]` は key mode ごとに play skin path と設定を持つ。
4K は `play4`, `play4_options`, `play4_files` を使う。
6K は `play6`, `play6_options`, `play6_files` を使う。
8K は `play8`, `play8_options`, `play8_files` を使う。

2K は skin type のみ予約し、BMZ 本体の key mode としてはまだ扱わない。

## Bundled Rmz-skin Extensions

`data/skins/Rmz-skin` の BMZ 同梱版は、BMZ 独自 play skin type として
`play4main.luaskin` (`type=22`), `play6main.luaskin` (`type=23`),
`play8main.luaskin` (`type=24`) を提供する。

8K 版はレーンごとのノーツ色を property で選択できる。property 名は
`8Key Lane 1 Color` から `8Key Lane 8 Color` までで、選択肢は
`White`, `Blue`, `Yellow`, `Scratch`。既定値は `Yellow, White, Blue, White, White, Blue, White, Yellow`。

5K 版は `Notes 5Key Color` property でノーツ色の並びを選択できる。
`Default` は従来通り `Scratch, White, Blue, Yellow, Blue, White`
(scratch left 時の画面左からの並び)。`6Key-like` は scratch side に関わらず、
画面左から `White, Blue, White, White, Blue, White` になる。

`F-RANDOM` / `MF-RANDOM` は既存の146×19 ARRANGE sprite領域へフォントで表示する。
Lua定義の `align=1` は destination の `x` を中央基準にするため、Rmz-skin側で
sprite左端から半幅だけ中央へ移し、フォントサイズもspriteの実字高へ合わせる。
