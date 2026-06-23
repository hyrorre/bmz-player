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
