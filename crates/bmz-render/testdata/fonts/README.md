# テスト用フォント

`scripts/generate_test_fonts.py` が多角形から生成した、`bmz-render` の文字描画テスト専用フォントです。
第三者フォントのグリフ・データは含みません。リポジトリと同じライセンスで扱います。

| ファイル | 内容 |
|---|---|
| `bmz-test-latin.ttf` | `A B S W o l m n 1 2` と空白のみ。記号・`?`・□を持たず、`.notdef`は空 |
| `bmz-test-question.ttf` | latinに`?`を追加 |
| `bmz-test-symbols.ttf` | ✡ □ ♠ ♡ ♫ ∞ �。em 2048で、latinと異なるascent/descent |
| `bmz-test-color.ttf` | COLR v0（😀 ❤ 🇯🇵）・COLR v1（✡ 🎵 家族 👍🏻 👩‍💻 1️⃣）、✡と❤のモノクロ輪郭、ZWJ/国旗/肌色/キーキャップ/タグ旗（イングランド・スコットランド・ウェールズ）のGSUB合字 |
| `bmz-test-colr-empty-base.ttf` | glyf表を持つが、✡ ❤ 😀の基底グリフは空でCOLRレイヤーだけを持つ。★は空グリフでカラーもない。空白あり |
| `bmz-test-emoji-parts.ttf` | 家族・👩‍💻・👍🏻・🇯🇵・1️⃣・❤・タグ旗の構成文字（🏴とタグ文字を含む）だけをCOLRで持ち、合字を持たない |
| `bmz-test-keycap-mark.ttf` | `1`と、送り幅0で直前の数字に重ねる結合用囲み（U+20E3）の輪郭だけ。GNU FreeFontと同じ形 |
| `bmz-test-sbix.ttf` | 😀のsbix PNG（左半分赤・右半分青、輪郭なし） |

生成時の`head`の日時は固定しているため、再生成しても内容が同じなら差分は出ません。
