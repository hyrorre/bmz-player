# Antique Playのロード進捗表示

対象は `5bc92c37` 時点の同梱Antique Play 7KEYSです。

Antiqueはoption 80が有効な間、`loading.png`の「NOW LOADING...」と数値ref 165を描き、
その上にgraph type 102を重ねます。BMZではそのgraphが常に全幅で、destinationの
`blend=9`も反映せずに描いたため、不透明な帯で文字と数字の両方を覆っていました。

graph type 102を既存の音声・BGA進捗につなぎ、graphのdestinationから合成指定を渡すようにしました。
`blend=9`のRGBはbeatorajaの `GL_ONE_MINUS_DST_COLOR, GL_ZERO` に対応し、描画先alphaは保持します。
詳細は[Playのロード進捗](../../docs/skin.md#playのロード進捗)を参照してください。

WindowsのオフスクリーンGPUテストでは、同梱Antique Play 7KEYSで0%・50%・100%の
ラベルと数字の描画、およびREADY移行時にロード表示が消えることを確認しました。
実ウィンドウでの選曲からの遷移と、macOS/Linuxの表示は未確認です。
