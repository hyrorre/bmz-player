# テスト用フォント

`scripts/generate_test_fonts.py` が多角形から生成した、`bmz-render` の文字描画テスト専用フォントです。
第三者フォントのグリフ・データは含みません。リポジトリと同じライセンスで扱います。

| ファイル | 内容 |
|---|---|
| `bmz-test-latin.ttf` | `A B S W o l m n 1 2` と空白のみ。記号・`?`・□を持たず、`.notdef`は空 |
| `bmz-test-question.ttf` | latinに`?`を追加 |
| `bmz-test-symbols.ttf` | ✡ □ ♠ ♡ ♫ ∞ �。em 2048で、latinと異なるascent/descent |
| `bmz-test-color.ttf` | COLR v0（😀 ❤ 🇯🇵）・COLR v1（✡ 🎵 家族 👍🏻）、✡と❤のモノクロ輪郭、ZWJ/国旗/肌色のGSUB合字 |
| `bmz-test-sbix.ttf` | 😀のsbix PNG（左半分赤・右半分青、輪郭なし） |
