# BMZ Skin Notes

BMZ は beatoraja JSON / Lua skin の互換を基本にする。既存 beatoraja skin type は
そのまま扱い、BMZ 独自の key mode だけ拡張 skin type を予約する。

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

## Static Image Sources

beatoraja skin の `source.path` は PNG / BMP / JPEG / GIF / TGA に加え、libGDX
`PixmapIO` の CIM を読み込める。CIM は zlib stream 内の width / height /
Gdx2DPixmap format と pixel buffer を RGBA8 へ展開する。MILLIONDOLLAR RESULT の
主要 atlas は配布時点から `.cim` のため、PNG fallback へ書き換えずそのまま扱う。

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

### BMZ Hispeed Mode Refs

`1900` 台は BMZ play runtime extension として扱う。beatoraja 互換 ref と衝突させないため、
BMZ 独自のプレイ中状態はこの範囲へ追加する。

| ref | kind | meaning |
| ---: | --- | --- |
| 1900 | number / event_index / text | HS mode。number / event_index は `0=NHS`, `1=FHS`、text は `NHS` / `FHS` |
| 1901 | number / option | FHS active flag。`0=NHS`, `1=FHS` |
| 1902 | number | target green number。FHS 時は固定 target green、NHS 時は現在 green number |

`op: [1901]` または `draw: "number(1901)==1"` で FHS 時だけ destination を表示できる。

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
