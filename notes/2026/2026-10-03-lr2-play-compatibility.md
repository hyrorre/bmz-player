# LR2 Play skin互換修正

現在の仕様は [skin.md](../../docs/skin.md)、対応状況は
[skin-compatibility.md](../../docs/skin-compatibility.md) を参照。

## KCOOLの判定文字

手元のKCOOL Ver 1.72のCombo-1.pngは、PGREAT/GREAT/GOOD/BADの文字領域
（x=0..97、y=0..287）が空で、コンボ数字とPOORの文字だけを持つ。
123.pngにもPGREAT/GREAT/GOODの文字がない。画像を直接確認した。
画素も確認し、これらの領域は全てRGBA=(255,255,255,0)の単色だった。
alphaの裏に文字のRGBが残っているケースでもない。
スキンが参照する領域そのものに文字がないため、BMZ側で文字を補う変更は行わない。
画像は第三者素材として変更・コミットしない。
実素材がある場合に各判定・複数の経過時刻で描画命令へ到達するテストを追加した。

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

## ノーツのレーン状態

SRC_AUTO_*を通常画像から分離し、sessionの部分AUTO入力レーンをsnapshot経由で渡す。
全体AUTOおよび表示専用の対戦側は除外する。DST_NOTEを全行保持し、表示時計・条件と
毎フレームの座標・幅・高さを評価する。スクロール距離は初期レーン高を維持する。
SRC時計はDST開始時刻を考慮し、LNの非押下画像は先頭フレームを維持する。
参照実装のAddDrawingBuffer_PlayArea/LNに合わせ、通常合成とゲーム側alphaを使用する。
検証: workspace check / all-targets Clippyを通過。workspace testで従来のAUTO画像共用を
期待するテストが失敗したため分離後の格納先へ修正し、bmz-skin / bmz-renderを再実行して通過。
その他のworkspaceテスト（appのAUTOレーンsnapshotテストを含む）は通過。fmtも通過。

## 2P数値

LR2 play ref=120..136を内部ref=19220..19236へ変換する（127は従来の変換を維持）。
OpponentRenderSnapshotを描画状態へ保持し、実コンボ・判定数・EX・現在率・最終率・
スコア差・次ランク差を算出する。LR2_skinobject.cppの各refとScene04_Play.cppの配点を参照。
120は確定値のみ対応し、参照実装のscore_printの時間補間は含めない。
検証: workspaceのcheck / all-targets Clippy / test、fmtを通過。
実機での対戦表示確認は未実施。

## 画像の透過色

LR2_skinload.cppのSetTransColorに合わせ、既定緑とTRANSCOLOR/TRANSCLOLR/TRANSCLOLORを
画像ごとに保持する。alphaのない画像だけを透過し、元のalphaは維持する。
同一パスで別の透過色を指定しても混ざらないようCPU/GPUキャッシュのキーに加える。
RGB BMP/PNG・RGBA維持と、LR2ファイルから同一画像を二重ロードする回帰テストを追加。
検証: workspaceのcheck / all-targets Clippy / test、fmtを通過。実機確認は未実施。

## system font

読み捨てていたFONTを独立の番号表に保持し、LRDrawTextに合わせたDST高さ・幅上限と
縁取りに接続する。同番号のLR2FONTを優先する。字形は既存のbmz-font経路を使う。
DxLibのthickness・AA方式は未対応として残し、宣言値だけ保持する。
検証: workspaceのcheck / all-targets Clippy / testを通過。変更したファイルはfmt済み。
実機での文字の目視確認は未実施。

## 横スクロール

HORIZONTALを保持し、Scene04_Play.cppの移動方向に合わせて右から左へ進める。
参照実装はLNのlongYと小節線に縦計算が残るため、その不整合は再現せず、
BMZの補完としてLN両端・胴体・各線を横へ流す。初期Xから右端までを移動距離とし、
レーンYやUIは回転しない。LIFT/cover offsetをXへ変換し、DST_NOTE2を落下先Xにする。
横長キャンバス・異なるレーンY・LIFT有無・LN接続・小節線・実snapshot連携のテストを追加。
カバーのdisapearLineによるcropは既存の縦定義のままで、横向きcropは未対応。
検証: workspace check / all-targets Clippy / test、fmtを通過。縦スクロール時の不要な
距離計算を省いた後、bmz-renderのcheck / all-targets Clippy / testを再実行して通過。
最終状態でWindowsのbmz-player debug buildも通過。実機の目視確認は未実施。
