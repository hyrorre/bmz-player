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
| 24 | 8KEYS | 予約のみ |

`22=4KEYS` と `23=6KEYS` は Scratch なしの `Key1..KeyN` 用 play skin type。
beatoraja には対応する skin type が無いため、BMZ 専用 skin として扱う。

## Profile Slots

profile の `[skin]` は key mode ごとに play skin path と設定を持つ。
4K は `play4`, `play4_options`, `play4_files` を使う。
6K は `play6`, `play6_options`, `play6_files` を使う。

未実装の 8K は当面 `play7` にフォールバックする。2K は skin type のみ予約し、
BMZ 本体の key mode としてはまだ扱わない。
