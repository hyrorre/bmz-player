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

選曲画面は`bmz_player::select_profile=debug`で、120フレームごとに`select frame profile`を記録します。
文字の準備時間（`text_ms` / `text_p99_ms` / `text_max_ms`）と、文字atlasのGPU転送を含みます。
`text_instance_upload_ms`は文字インスタンスバッファの確保と`write_buffer`、
`text_atlas_upload_ms`（`text_atlas_upload_p99_ms` / `text_atlas_upload_max_ms`）は
文字atlasテクスチャの作成と`write_texture`にかかったCPU時間で、どちらも`upload_ms`の内数です。
GPU上で転送が終わるまでの時間は含みません。atlasに変更がないフレームでは
`text_atlas_upload_ms`はほぼ0になります。`text_atlas_writes`は
`write_texture`の呼び出し回数、`text_atlas_max_writes`は1フレームの最大回数です。
`text_atlas_upload_kib`は転送量です。`text_atlas_full_uploads`はテクスチャ全体の転送、
`text_atlas_region_uploads`はグリフごとの部分転送、`text_atlas_rebuilds`はatlasの作り直しの回数です。
回数と転送量は区間の合計です。

文字atlasの空きがなくなった場合は、現在と過去3フレームにグリフまたはレイアウトから参照した
行を残し、古い行だけを空けて再利用します。残すグリフの座標・RGBA画素は変えず、
画素コピーや再転送はしません。追い出した行を含むレイアウトだけを無効にします。
新しい転送領域は、CPU側もGPU側も透明な1pxのパディングごと上書きします。
GPUテクスチャの寸法・倍々の確保・部分転送・Shrink倍率の量子化は変えません。

古い行を空けられた場合だけ、GPUへ渡す前にそのフレームを組み直します。
最初の組立で追加した文字の転送も引き継ぎます。新しい文字も収まらなければ空のatlasでも試します。
救えなかった溢れは従来どおり次のフレームで全消去し、1フレーム分が収まらない場合の
60フレームの再試行待ちを維持します。`text_atlas_rebuilds`は行の回収と全消去の合計です。

満杯の原因を調べるときは、通常の選曲profileに
`bmz_render::text_atlas=debug`を加えます。個別の時計と全グリフの集計は、この診断を
有効にしたときだけ動きます。診断ありの値と診断なしの値は分けて比較してください。

```powershell
$env:RUST_LOG = 'info,bmz_player::select_profile=debug,bmz_render::text_atlas=debug'
```

- `text atlas inventory`: 満杯時のグリフ数、font数、font内glyph IDの組の数
  （`characters`。Unicode文字数ではありません）、縦横倍率の組の数、種類別の数、
  装飾用レイアウト数、パディングを含む格納面積と現在・過去3フレームの参照面積。
  `kinds`はvector / bitmap / tofu / color / color silhouette / styled fallbackの順です。
  通常の縁取り・影はvector画素を共有し、別グリフにはしません。
- `text atlas frame`: フレーム番号、準備・ラスタライズ時間と回数、layout hit/miss、
  行の回収（`recycles`）・全消去（`full_clears`）の累計、atlas寸法、CPU画素バッファの確保容量。
  `layout_and_pack_us`は準備時間からラスタライズ時間を引いた値で、配置、行の回収、
  インスタンスの符号化、診断の時間も含みます。純粋なlayout時間ではありません。
  `missing_glyphs`は最終的に返すフレームの容量不足による未描画要求数、
  `attempted_misses`は救済前を含む要求数です。影・縁取りの重複を含み、文字数ではありません。
- `text atlas recycling`: 行の回収時間、追い出したグリフ数、保持したグリフ数、
  行の総数と直近に参照された行の数。
- `text atlas upload`: 同じフレーム番号で転送CPU時間、書き込み回数、転送量、
  GPUテクスチャ再作成の有無。GPU上で転送が終わるまでの時間は含みません。

さらに`bmz_render::text_atlas=trace`では満杯時に個々のグリフキー（font内glyph ID、
倍率のfloat bits、種類）と寸法を記録します。`bmz_player::select_profile=trace`では
各フレームのrender内訳も記録します。ログ出力の負荷が増えるため、通常の性能比較では
debugの区間集計を使います。Windowsの比較条件と検証限界は
[2026-10-24の計測記録](../notes/2026/2026-10-24-text-atlas-recycling.md)にあります。

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
