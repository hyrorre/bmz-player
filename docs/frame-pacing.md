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

## Surface再設定の失敗と復旧

resize（Windows標準タイトルバーでの最大化・復元を含む）、Outdated、
present mode・frame latency変更は同じSurface復旧処理を使います。希望設定と最後に
正常適用できた設定を分け、再設定開始から成功確認まではSurfaceを取得・描画・presentしません。
`render_target_size()`、実効present設定、スキンのscreen-sizeは正常適用した値を使い、
復旧成功時に新しいサイズをスキンへ通知します。通常のウィンドウ最大化を排他フルスクリーンへの
切り替えとして扱うものではありません。

Outdatedでは取得処理内で保存済みサイズを再設定せず、ウィンドウを管理するappへ戻ります。
appは復旧処理の前に`window.inner_size()`で現在の物理サイズを取得し、最新の希望設定へ
反映します。Resizedの配送より先にOutdatedが届いた場合もこの経路を使います。
取得後にサイズが変化する可能性は残るため、再設定失敗後の利用抑止は引き続き必要です。
Suboptimalは有効な画像として通常どおり描画・presentし、それだけでは再設定しません。
再設定は描画関数から戻り、取得画像とtexture/view参照を解放した後で行います。

失敗時は250ms間隔、初回を含む最大5回で再設定します。ウィンドウ操作の一時的な状態変化を
待つ余地を残し、約1秒の待機期間に制限するための値です（configure自体の所要時間やOSの
イベント配送遅延は別）。イベントループの`WaitUntil`と既存のフレームスケジューラを使い、
resizeイベントの再到着やsetterの再呼び出しには依存しません。復旧待ち・0サイズ中のCPU側
フレームは最大30FPSとし、通常のFPS上限やバックグラウンドFPS上限がそれより低ければ低い値を
維持します。入力・終了操作・egui更新を継続し、再試行の期限はCPUフレームの期限と別に管理します。
描画保留中はWindowsの期限直前の短時間待機も使わず、復旧のためのsleepやspinは行いません。
正常時のフレーム待機・GPU同期の方式は変えません。
Windowsの移動・リサイズ中は通常の`about_to_wait`が止まるため、既存の再描画timerから届く
`RedrawRequested`でも同じ期限チェックを実行します。

復旧中にサイズや設定が変わった場合は、次回試行で最新の希望値を使います。同一設定の再要求、
連続リサイズ、最小化で期限・試行回数をリセットしません。幅または高さが0ならconfigureも
取得も行わず、試行予算を消費しません。非0サイズへ戻ったら残りの予算で復旧します。
eguiのテクスチャ作成・部分更新はスキップ中も順序どおり適用し、解放も処理します。
スキップしたフレームはpresent成功・FPS・smokeの描画件数には数えません。
スクリーンショット要求は描画成功まで保持し、復旧後の画像を保存します。

試行ごとに契機、バックエンド、GPU・ドライバ、取得した物理サイズ、希望・正常適用設定、試行回数、
元のエラーと成功／打ち切りをログへ記録します。上限に達したら一度だけ通常の終了処理へ入り、
保存・音声等の終了後にネイティブのエラーダイアログを表示し、エラー終了します。
`--smoke-*`指定時は無人実行を止めないようダイアログを表示せず、ログを残してエラー終了します。
描画不能なegui上の通知には依存しません。オフスクリーン動画出力、起動時のバックエンド候補試行、
Windows排他フルスクリーン起動時のフォールバックは維持します。

SurfaceLostは新しいSurfaceが必要な状態、DeviceLostはSurface再設定だけでは復旧できない
状態として区別し、どちらも同じ制御された終了経路へ進みます。実行中のSurface／Device全体の
再生成は行いません。DeviceLost callbackは状態を通知するだけでGPU操作を行わず、app側で
通知を消費します。正常終了に伴うDestroyed通知は無視します。error scopeが空でもDeviceLost
通知があれば描画を再開しません。通常の未捕捉GPUエラー検出は維持し、GPUドライバ内部の
ネイティブ例外まで防ぐものではありません。上位層から得られないVkResultは推測しません。

失敗注入による検証・Windows確認の結果は[作業記録](../notes/2026/2026-11-09-surface-reconfiguration-recovery.md)を参照してください。

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

## スキン準備中の画面遷移

初回起動のSelectだけは最初の描画前にスキンの準備を待ちます。
起動後のSelect・Decide・Play・Resultへの遷移は、decode/uploadの完了を
フレーム間で確認します。待機中もウィンドウのイベントとeguiを処理し、
直前の描画プランに読み込み表示を重ねます。古いスキンのLua callbackへ
次の画面の状態を渡さず、READY・Resultの入力解禁・自動退出を進めません。

現在の世代のスキン準備が終わったら、その画面のタイマーを開始します。
Playの音源先読みは同時に進め、スキン完成後にセッションを適用します。
Decideの読み込み中のキャンセルは、スキンの完成を待たずに選曲へ戻ります。
通常の設定変更によるスキン再読み込みとResultの表示内容更新は、
画面への新規入場としてタイマーをリセットしません。

## 選曲のDB処理

難易度表の保存はSelect中に同時1件のworkerへ渡します。開始済みのtransactionは
画面遷移後もworker上で完了させ、新しい保存と結果のUI反映は次のSelectまで保留します。
取得済みの表を待機させるchannelは2件に制限し、保存より取得が速い場合のメモリ増大を抑えます。
アカウント変更時の旧表削除は進行中の保存後に行い、旧世代の結果を画面へ反映しません。

一覧の読み取りにはworker専用のread-only DB接続を使います。
検索・並べ替え・譜面の取得元照合を終えた一覧をwindow threadへ返し、世代を確認して適用します。
同一フォルダ情報・クリア履歴のDB取得に失敗した場合は、毎フレーム再試行せず1秒空けます。

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

SCROLLがすべて有限の正倍率なら、区間積分の逆変換でノーツ・LN・小節線等の可視候補を
絞ります。0・逆向き・非有限倍率では全候補を評価します。判定履歴の公開は同容量の
HashMapを再ハッシュせずコピーし、BGA選択は描画と動画更新で同じ種類別の時系列cacheを使います。

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
