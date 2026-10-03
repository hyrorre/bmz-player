# LR2 Play skin互換修正

現在の仕様は [skin.md](../../docs/skin.md)、対応状況は
[skin-compatibility.md](../../docs/skin-compatibility.md) を参照。

## KCOOLの判定文字

手元のKCOOL Ver 1.72のCombo-1.pngは、PGREAT/GREAT/GOOD/BADの文字領域
（x=0..97、y=0..287）が空で、コンボ数字とPOORの文字だけを持つ。
123.pngにもPGREAT/GREAT/GOODの文字がない。画像を直接確認した。
スキンが参照する領域そのものに文字がないため、BMZ側で文字を補う変更は行わない。
画像は第三者素材として変更・コミットしない。
実素材がある場合に各判定・複数の経過時刻で描画命令へ到達するテストを追加する。

## 減算合成

WMIIのDST_IMAGEにあるblend=3が通常合成へ落ちていた。
source RGB×alphaを描画先RGBから引くGPU pipelineを追加し、描画先alphaを保持する。
通常描画・Ambientの両方にpipelineを接続する。

検証: workspaceのcheck / all-targets Clippy / test、fmtを通過。
Windows GPUのoffscreen readbackでも、半透明の赤を白から減算すると
RGBが約(127,255,255)、alphaが255になることを確認した。
手元のKCOOLを読み込む判定描画テストも実素材ありで通過。
ゲーム画面を操作しての目視確認は未実施。

## DST区間とSRC時計

LR2の各区間のacc（三次加速・減速）、blend/filter/centerを保持する。
比較箇所はEn_value.cppのChangeValueByTimeとLR2_skindraw.cppのSetDSTdrawByTime。
画像はSRC/DSTのタイマーが同一の場合に最初のDST時刻をSRC時計から差し引く。
数字はAddDrawingBuffer_Numbersに従い、SRC時計をそのまま使う。
境界時刻・区間属性・加減速・異なるタイマーの回帰テストを追加。
検証: workspaceのcheck / all-targets Clippy / test、fmtを通過。
