# Frame pacing と計測

## 待機方式

描画周期は `FramePacer` の monotonic deadline で管理する。通常の起床ジッターでは
deadline 列を維持する。遅れを取り戻すため次の間隔が周期の半分未満になる場合は、
現在時刻から再開して長短フレームの連発を防ぐ。スキン更新の即時描画は周期を変更せず、
FPS・focus・実効 present mode・window mode が変わった場合は周期をリセットする。

Windows の focused Immediate・FPS制限ありの場合だけ、期限の
`min(600us, frame_budget / 8)` 前に winit の `WaitUntil` で起床する。
RedrawRequested 到着後、残りがこの範囲に収まる場合のみ render thread で短く待つ。
winit 0.30.13 自体がWindowsの高精度waitable timerを使っているため、同じtimerを
二重に追加しない。600usは実測した約0.5〜0.6msのwake p95を踏まえた上限であり、
OSによる長いpreemptionを防ぐものではない。

FIFO / Mailbox / background / Unlimited / Windows以外ではこの短時間待機を使わない。
設定上のImmediateが別modeへfallbackした場合も、surfaceの実効modeで判断する。
待機中にgameplay/input/audio/GPUのlockは保持せず、GameplayRuntimeのwakeは変更しない。
追加のCPU実行時間は必要になるが、長い待機はOSへ返す。通常のperiodic frameで
短時間待機に使う上限は1周期の1/8、120FPSでは600us、240FPSでは約521usとなる。

これはdisplay/compositorとの位相同期を実装したものではない。VSync、VRR、driver設定や
present queue設定は変更しない。ImmediateのtearingやGPU stallを解消する保証もない。

Waylandで実効present modeがFifo/FifoRelaxedのときは、取得できたsurfaceをpresentする直前に
winitの`pre_present_notify()`を呼ぶ。winitのframe callback待機中も入力・user eventは処理できる。
取得失敗や描画スキップでは通知せず、commitされないframe callbackを残さない。
このcallbackを使う場合はUnlimited FPSでもControlFlow::Waitとし、callback待ちをPollでspinしない。
Immediate/Mailboxおよび他のウィンドウbackendの待機方式は維持する。
イベント配送の比較方法と限界は[Linux遅延検証](linux-latency.md)を参照。

## 診断

通常の `info` ログでも、focusedなPlay中の長い描画停止を
`bmz_player::frame_stall` のWARNとして記録する。再現のための環境変数設定は不要。

- `play redraw gap`: 前回の描画ハンドラ終了から次の開始まで、設定FPSの1周期に
  250msを加えた時間以上空いた場合。待機からの起床遅れと、その後の描画開始までの時間も残す。
- `slow play redraw`: 描画ハンドラ内で250ms以上かかった場合。入力、background結果の反映、
  画面遷移、egui、play消費、scene描画、描画後処理の時間を残す。
- `slow play scene render`: scene処理が250ms以上かかった場合。snapshot、動画、描画処理と、
  surface取得・queue submit・presentなどの既存renderer計測値を残す。

gapは前フレームの処理時間を含めず、focus・FPS・present mode・window modeの変更、
Play以外の画面、新しいplay generationで比較をリセットする。復帰時に記録するため、
停止中のクラッシュではgapが残らないことがある。描画処理が継続しているcompositor側の
表示停止は、このログだけでは検出・原因確定できない。

より細かい集計が必要な場合は、以下を追加する。

```powershell
$env:RUST_LOG = 'info,bmz_player::play_profile=debug,bmz_player::frame_pacing=debug'
```

`CPU frame cadence` は描画開始間隔と、理想周期からの絶対誤差を5秒ごとに集計する。
`snapshot cadence` は描画時刻へ投影したsnapshotのage、前回描画との時刻差、
同じ描画時刻を使った回数を集計する。`publication_age_us` は投影前のgameplay状態の古さで、
描画時刻のageとは別に記録する。histogramのp95/p99はbucket上限の近似値。
`bmz_player::frame_pacing=trace` にすると個々のsampleを記録でき、offlineで正確な分位を計算できる。
通常プレイでTRACEは必要ない。既存profilerのwake latenessはOSへ渡した早期wake期限に
対する遅れなので、この変更後の描画期限の遅れそのものとは区別する。

FPS表示とアプリのpresent間隔だけでは実際の表示間隔を証明できない。
[PresentMon](https://github.com/GameTechDev/PresentMon) の対象process限定CSVと併用する。
Vulkanで取得できない値（NA等）はゼロ遅延として扱わない。Composed Flipから
Independent Flipへの遷移、focus喪失、ロード中を分け、計測中は他のbuildを走らせない。

設定の比較には `BMZ_DATA_DIR` / `BMZ_LOGS_DIR` を作業用ディレクトリへ向け、
`BMZ_RESOURCE_DIR` だけ同梱dataを参照させる。普段のconfig/score DBを上書きしない。
既存configからコピーする場合はOBS・IR等の外部連携を検証用環境へ持ち込まない。

## 描画時刻でのノーツ投影

ノーツ位置は最新のimmutableな描画用状態から、consumerのAudioClock時刻で計算する。
gameplayの公開周期やrendererとの位相差に座標更新周期を固定しない。
描画時刻の単調性はconsumer内で保持し、seek/retryの新しいgenerationでリセットする。
FPS制限、present mode、gameplay wake、判定・音声の処理はこの変更の対象外。

自動テストは120/240FPS、5種類の公開位相、0/16/33/100/250msの描画停止を組み合わせ、
等速区間の座標と移動量を数式に照合する。さらに、公開データを受け取れないconsumer、
新たに可視範囲へ入るノーツ・Mine・LN、STOP、BPM/SCROLL/SPEED、CONSTANT、
PMS/retention、LN/CN/HCN状態、表示offsetの変更とclock resetを検証する。
既存のruntimeテストは、描画停止の有無で判定・replay・score・audio schedulingが
従来のsession advanceと一致することを引き続き確認する。

## 回帰テスト

`cargo test -p bmz-player frame_runtime` で次を検証する。

- 120/240FPSの待機区間と短時間待機の上限
- FIFO / Mailbox / background / Unlimited / 他OSで短時間待機しないこと
- 即時描画を連続して要求しても周期が変わらないこと
- 16/33/100/250ms遅延から復帰しても極端な短間隔を連発しないこと
- FPS切替、通常のdeadline維持、wakeの統計、FPSカウンターの既存動作

gameplayの独立性は既存の `gameplay_runtime::tests` とworkspace testでも検証する。

## 計測・検証記録

過去の比較条件・実測値・テスト実行結果は
[2026-09-08からの作業記録](../notes/2026/2026-09-08-frame-pacing.md) を参照してください。
