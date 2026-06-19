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
