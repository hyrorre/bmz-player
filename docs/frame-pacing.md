# Frame pacing と計測

## 待機方式

描画周期は`FramePacer`のmonotonic deadlineで管理します。通常の起床ジッターでは
deadline列を維持します。遅れを取り戻すため次の間隔が周期の半分未満になる場合は、
現在時刻から再開して長短フレームの連発を防ぎます。スキン更新の即時描画は周期を変更せず、
FPS・focus・実効present mode・window modeが変わった場合は周期をリセットします。

Windowsのfocused Immediate・FPS制限ありの場合だけ、期限の
`min(600us, frame_budget / 8)`前にwinitの`WaitUntil`で起床します。
RedrawRequested到着後、残りがこの範囲に収まる場合のみrender threadで短く待ちます。
winit 0.30.13自体がWindowsの高精度waitable timerを使っているため、同じtimerを
二重に追加しません。600usは実測した約0.5〜0.6msのwake p95を踏まえた上限であり、
OSによる長いpreemptionを防ぐものではありません。

FIFO / Mailbox / background / Unlimited / Windows以外ではこの短時間待機を使いません。
設定上のImmediateが別modeへfallbackした場合も、surfaceの実効modeで判断します。
待機中にgameplay/input/audio/GPUのlockは保持せず、GameplayRuntimeのwakeは変更しません。
追加のCPU実行時間は必要になるが、長い待機はOSへ返します。通常のperiodic frameで
短時間待機に使う上限は1周期の1/8、120FPSでは600us、240FPSでは約521usとなります。

これはdisplay/compositorとの位相同期を実装したものではありません。VSync、VRR、driver設定や
present queue設定は変更しません。ImmediateのtearingやGPU stallを解消する保証もありません。

Waylandで実効present modeがFifo/FifoRelaxedのときは、取得できたsurfaceをpresentする直前に
winitの`pre_present_notify()`を呼びます。winitのframe callback待機中も入力・user eventは処理できます。
取得失敗や描画スキップでは通知せず、commitされないframe callbackを残しません。
このcallbackを使う場合はUnlimited FPSでもControlFlow::Waitとし、callback待ちをPollでspinしません。
Immediate/Mailboxおよび他のウィンドウbackendの待機方式は維持します。
イベント配送の比較方法と限界は[Linux遅延検証](linux-latency.md)を参照してください。

## 診断

```powershell
$env:RUST_LOG = 'info,bmz_player::play_profile=debug,bmz_player::frame_pacing=debug'
```

`CPU frame cadence`は描画開始間隔と、理想周期からの絶対誤差を5秒ごとに集計します。
`snapshot cadence`は描画時刻へ投影したsnapshotのage、前回描画との時刻差、
同じ描画時刻を使った回数を集計します。`publication_age_us`は投影前のgameplay状態の古さで、
描画時刻のageとは別に記録します。histogramのp95/p99はbucket上限の近似値です。
`bmz_player::frame_pacing=trace`にすると個々のsampleを記録でき、offlineで正確な分位を計算できます。
通常プレイでTRACEは必要ありません。既存profilerのwake latenessはOSへ渡した早期wake期限に
対する遅れなので、この変更後の描画期限の遅れそのものとは区別します。

FPS表示とアプリのpresent間隔だけでは実際の表示間隔を証明できません。
[PresentMon](https://github.com/GameTechDev/PresentMon)の対象process限定CSVと併用します。
Vulkanで取得できない値（NA等）はゼロ遅延として扱いません。Composed Flipから
Independent Flipへの遷移、focus喪失、ロード中を分け、計測中は他のbuildを走らせません。

設定の比較には`BMZ_DATA_DIR` / `BMZ_LOGS_DIR`を作業用ディレクトリへ向け、
`BMZ_RESOURCE_DIR`だけ同梱dataを参照させます。普段のconfig/score DBを上書きしません。
既存configからコピーする場合はOBS・IR等の外部連携を検証用環境へ持ち込みません。

## 描画時刻でのノーツ投影

ノーツ位置は最新のimmutableな描画用状態から、consumerのAudioClock時刻で計算します。
gameplayの公開周期やrendererとの位相差に座標更新周期を固定しません。
描画時刻の単調性はconsumer内で保持し、seek/retryの新しいgenerationでリセットします。
FPS制限、present mode、gameplay wake、判定・音声の処理はこの変更の対象外です。

自動テストは120/240FPS、5種類の公開位相、0/16/33/100/250msの描画停止を組み合わせ、
等速区間の座標と移動量を数式に照合します。さらに、公開データを受け取れないconsumer、
新たに可視範囲へ入るノーツ・Mine・LN、STOP、BPM/SCROLL/SPEED、CONSTANT、
PMS/retention、LN/CN/HCN状態、表示offsetの変更とclock resetを検証します。
既存のruntimeテストは、描画停止の有無で判定・replay・score・audio schedulingが
従来のsession advanceと一致することを引き続き確認します。

## 回帰テスト

`cargo test -p bmz-player frame_runtime`で次を検証します。

- 120/240FPSの待機区間と短時間待機の上限
- FIFO / Mailbox / background / Unlimited / 他OSで短時間待機しないこと
- 即時描画を連続して要求しても周期が変わらないこと
- 16/33/100/250ms遅延から復帰しても極端な短間隔を連発しないこと
- FPS切替、通常のdeadline維持、wakeの統計、FPSカウンターの既存動作

gameplayの独立性は既存の`gameplay_runtime::tests`とworkspace testでも検証します。

## 計測・検証記録

過去の比較条件・実測値・テスト実行結果は
[2026-09-08からの作業記録](../notes/2026/2026-09-08-frame-pacing.md)を参照してください。
