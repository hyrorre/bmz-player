# Gameplay runtime

Play開始後の`GameSession`は`bmz-gameplay::runtime::GameplayRuntime`が所有し、
`bmz-player::gameplay_runtime`の専用スレッドだけが変更します。
window/event-loopスレッドは入力設定、開始前準備、画面遷移、永続化、GPU、BGA upload、
skin、egui、surface acquire / submit / presentを担当します。

対戦相手のscore・gauge・timerと独立した判定セッションは`GameSession::opponent`
（`OpponentRuntime`）に保持します。表示用の値は`display_state`から取得し、独立した
対戦相手がある場合はその状態を優先します。各方式の判定と時計の進め方は維持します。

```text
以前: window/event loop
  入力取得 → advance / 判定 / 発音 → snapshot → render / present [100ms 停止]
       └──────── 次の入力収集とゲーム進行も 100ms 待つ ─────────┘

現在:
  input capture ── timestamped bounded queue ── gameplay thread ── audio command queue
                                                   │                  ↓
                                                   │             audio callback
                                                   ↓
                                      latest snapshot + sequenced events
                                                   ↓
  window/event loop ───────────────── render / present [100ms 停止]
```

## 入力と時計

- Windowsキーボードは入力専用スレッドのmessage-only windowでRaw Inputを受け取ります。
  message取得時に既存`monotonic_timestamp_ns()`でtimestampを確定します。
  foreground windowを入力スレッドでも確認します。キーのrepeat / releaseとscancodeは
  winitのPhysicalKeyと既存の共通`DeviceInputEvent`へ正規化します。
- Windows HID Raw Inputは同じmessage threadが取得します。
- gilrs / experimental GameInputとanalog scratchのpolling・停止検知も入力スレッドで行います。
  GameInputの構築、COM使用、破棄は同じowner thread上に保ちます。
- macOS GameControllerは専用の直列dispatch queueから直接SharedInputBackendへ配送します。
  macOS 14以降は入力履歴を通知時に全件drainし、11〜13は要素別callbackを使います。
  scratch停止は同じqueueの1ms timerで確認します。履歴の軸処理は入力サンプル時刻で進めます。
- Gameplayに送る入力と、メニュー・キー設定用のUIコピーは別キューにします。
  UIのコピーが滞留してもgameplayの配送には影響しません。
- macOS / Linuxのゲームパッドも独立取得します。macOSキーボードは`macos-iohid` featureで有効化したIOHID専用run loop、
  またはmacOS 11以降のGCKeyboard専用dispatch queueで独立取得できます。
  winitを選んだ場合とLinuxではwindow/event-loop停止中のキーボード取得遅延が残ります。
- 判定時刻の変換は既存の`AudioClock`、`InputTimestampAnchor`、input offsetを使います。
  wall clockの時刻補正に依存しません。GameInput等の既存 バックエンドtimestamp変換は維持します。

## Wake と audio

入力到着とcontrol commandはgameplay threadを`unpark`します。
次のノート、見逃し期限、LN終端、auto key release、replay、BGM schedule-aheadの期限と
2msのsafety wakeのうち早いものまで`park_timeout`で待機します。
safety wakeはHCNや時計の進行を補完するもので、rendererのtickに依存しません。

既存の`advance_session_frame`が入力、判定、Mine、見逃し、LN/CN/HCN、ゲージ、score、
replay recording、autoplay、終了判定を処理します。keysoundとBGMは`ScheduledSound`の
既存frame座標を保ってAudioEngineの有界command queueへ送ります。
送信できなかったkeysoundとHCN音量更新は次のwakeで再送します（途中FAILED時は破棄）。
判定ガイド音、既定の地雷SE、失敗SEもgameplayが直接送信します。
callbackには判定・入力処理を移していません。command scratchと通常再生用queue / voice領域は
callback接続前に事前確保し、callbackは
queueとengineの`try_lock`に失敗した場合も待機しません。

macOSの実験用Core Audio IOProcも同じNativeOutputRenderer・source command queue・
AudioEngineを使います。HALの出力バッファへfloat PCMを書き、出力時刻の大きな欠落は既存の
frame座標を前進させて処理します。取得・判定・音声コマンドの所有権と旧playの取消を維持します。
IOProcの登録・開始・停止・解除とcontext破棄はapp側で行い、停止・解除後にcontextを解放します。
出力形式やレートが変わった場合は無音化し、設定再適用による再生成を必要とします。

## Snapshot とイベント

window threadはlive GameSessionを参照せず、detached `PlaySessionObservation`と
immutable publicationを受け取ります。publicationは最新の1件だけを保持します。
交換区間だけで双方が`try_lock`を使い、古いpublicationの破棄、snapshot構築、GPU処理は
ロック外で行います。rendererが交換lockを保持してもgameplayは進行します。

snapshotはconsumerの要求時と最大8ms間隔で生成し、入力のないsafety wakeごとの
snapshot allocationを避けます。scoreとresult graphはsnapshotの生成頻度から独立して記録します。

ノーツ座標はgameplay threadでは確定しません。publicationに付随する`PlayfieldProjection`
は譜面・TimingMap・scroll cacheと、判定済みノーツ・LN/HCN状態・表示オプションの
読み取り専用コピーを持ちます。chart-sizedの判定mapを事前確保した3つのArcバッファを再利用し、
consumerが参照していないバッファだけを更新します。GameSessionや判定engineは渡しません。

consumerは取得したAudioClockの現在時刻で、ノーツ・Mine・LN端点・ガイド線・表示BPMを
既存のスクロール計算から投影します。同じpublicationを再利用しても表示座標は進みます。
可視ノーツだけを外挿する方式ではないため、stall中に新しく可視範囲へ入ったノーツも描画できます。
STOP、SCROLL/SPEED、CONSTANT、note retention、PMS見逃し表示の計算は準備画面と共通です。
consumerの投影はgameplayのadvance、入力判定、audio enqueueを呼ばず、交換lockの外で行います。

入力・判定のpresentation eventは既存sequenceとgame timeを保持します。
consumerが受領したsequenceまでをacknowledgementで破棄します。
skin runtimeは既読sequenceを再処理せず、timerはイベントの元時刻から評価します。
これにより、古いbomb / keybeamを復帰時点から新しく再生しません。
累積状態を使うobserverには、受領まで保持した全イベントを渡します。
極端に長い描画停止へのメモリ上限は8,192 eventsです。上限を超える演出履歴は古いものから捨てます。
判定、replay、結果、発音にはこの破棄を適用しません。

判定画像・コンボ・ボムに渡す`DisplayJudgementEvent.display_time`は、実入力では
入力オフセット補正前の譜面時刻を使います。正のオフセットでも打鍵直後から表示を開始し、
負のオフセットでもアニメーションを途中から始めません。入力配送・描画が遅れた場合は
打鍵からの経過時間を維持し、受領時刻から再生し直しません。見逃し・autoplay・replay・
対戦相手は元の判定時刻を使い、現在のprofileの入力オフセットを差し引きません。
表示履歴の800ms保持期間もこの表示時刻で計算します。採点用の`JudgementEvent`、
FAST/SLOW、保存する結果・replay、キー音、順序付き`SkinRuntimeEvent`の時刻には影響しません。

自動timing調整で負方向へvisual offsetが変わる場合は、描画投影の時刻だけを前回値以上に
抑えます。判定用offset、保存値、FAST/SLOW、replayの時刻には影響しません。
seek / retryは新しいgenerationなので、このvisual floorもリセットします。

## Lifecycle

READY前は準備用runtimeをローカルに保持し、音声時計の開始後にworkerへ所有権を移します。
各playは単調増加するgeneration、入力queue、snapshot、command channelを持ちます。
Viewer seekでも入力queueを作り直します。入力ルートの切り替え時は旧sinkへreleaseを送り、
新しいplayに旧押下状態を持ち越しません。

windowの実効focus変更は、そのイベント処理中に入力captureのrouteへ反映します。
Waylandの非表示等でRedrawRequestedが止まっていても、喪失時の保持解放と入力抑止、
復帰時の配送再開を次の描画まで待ちません。gilrs workerの反映待ちは既存の最大50ms以内です。

workerの停止はatomic flagとunparkで要求します。UIは終了済みのworkerだけjoinし、
まだ終了していないworkerの後始末を待ちません。audio commandも停止flagを保持し、
enqueue時とcallbackの適用時に検査します。旧workerの遅い送信を次のplayに適用しません。

workerはスコア確定時・終端遷移時にimmutable `FinishSessionSnapshot`とgraphを発行します。
通常play、course、replay、abort、結果画面への遷移はこの結果からstorage / IRの保存・送信経路を呼びます。
UIは終了要求を送った直後の古い観測値を保存せず、workerの確定結果を待ちます。

### 途中FAILED時の譜面音声

手動中断（Esc / E1+E2等）またはゲージ枯渇によって`PlayState::Failed`が確定すると、
`GameplayRuntime`はそのプレイの譜面音声を停止します。閉店演出開始時の要求であり、
描画更新・RESULT入場・システム効果音の初期化を待ちません。Practiceで中断と同時に
時計が停止しても、停止要求と未送信要求の破棄を行います。

- `flush_audio()`はFAILEDフレーム内で生成済みのBGM・キー音、先行予約、再送待ちの
  発音・HCN音量変更を破棄します。通常の発音時刻・入力 / 音声オフセットは変更しません。
- プレイ別の`AudioEngineHandle::stop_playback()`は、有界キューとは独立したatomicの
  停止フラグを保持します。送信時とcallback適用時に要求を拒否し、遅着・並行送信からの
  再発音を防ぎます。workerの既存取消フラグとは別なので、worker破棄で停止自体は取り消されません。
- callbackは停止フラグを観測した時点で`clear_playback()`により全voiceと予約を消去します。
  キューの`try_lock`が失敗しても消去します。engineを取得できないcallbackは無音で返し、
  停止フラグは次回以降まで保持します。callback内でロック待ち・I/O・ビジーループは行いません。
- デコード済みsample bankを保持し、`clear_playback()`に伴うgainリセットは元の値へ戻します。
  pause状態、playback rate、音声時計は変えません。既にデバイスへ渡したバッファの巻き戻しは行いません。
- 停止は当該sourceの寿命中は解除しません。同じ停止の再要求は安全で、`mark_draining()`でも
  復活しません。再プレイは既存経路の新しいengine / handleを使うため、古い要求は作用しません。
  Practice等が共有するデコード済みPCMは再利用できます。シーク用の`replace_playback()`は
  終端停止の解除APIではなく、途中FAILEDしていないViewer sourceの差し替えに使います。

判定条件は最終結果のクリアランプではありません。`PlayState::Finished`の通常完走と
全ノーツ処理後の終了操作は余韻をRESULTのdrainingへ引き継ぎます。完走時のFAILEDランプも同じ扱いで、
RESULT退出時の既存フェードを維持します。GASで別ゲージへ移って続行する場合も停止しません。
PlayStop、RESULTのBGM / SE、スキン音声は別のsystem sourceで再生し、この停止の対象外とします。

AUTOPLAY / REPLAY / PRACTICE / コースも実際に`Failed`へ入る場合にだけ共通処理を使います。
この停止処理は終了理由・画面遷移と、スコア保存・IR・リプレイ記録の条件には影響しません。
Viewerの終了・待機・一時停止・シークはViewer専用の経路で処理します。

調査と回帰テスト、実機確認手順は
[issue #26の作業記録](../notes/2026/2026-10-05-failed-chart-audio.md)を参照してください。

## 任意の終了・発音設定

プロファイルの`play.wait_all_notes_result`は既定OFFです。ONでは通常終了に使う
`GameSession.result_wait_end_time`を、譜面末尾とノート・不可視ノート・BGM・通常BGA
レイヤー（Base / Layer / Layer2）の最終配置時刻の最大値にします。その時刻から
譜面時刻で5秒の余白を経て終了します。PoorだけのBGA・BPM等の制御イベント・未使用音声定義・
PCM長は延長に使いません。`chart.end_time`、結果確定の`result_is_settled`、保存用の
譜面長・identityを変えず、終了時の確定snapshotから既存Result遷移を通します。
Practiceは設定の対象外で、Replay / Autoplay / Courseは通常終了時に適用します。
手動終了と途中FAILEDでは待機せず、上記のFAILED音声停止を行います。

`play.play_keysound_on_miss`も既定OFFです。ONでは見逃し処理から発音対象だけを
`KeySoundEvent`として出し、既存schedulerのキー音音量・正規化・多重定義・同一音の
再発音規則を使います。音はPOOR確定時刻で予約し、判定オフセットを再適用しません。
通常の自動キー音が有効なら追加分を抑止します。Replayでも現在のプロファイル設定を
使い、判定・ゲージ・保存される入力列やスコア区分には含めません。
見逃したHCN始端は上流の処理順に合わせて発音した更新ではミュートせず、次回更新から
通常のpassing音量制御を適用します。押し直すと既存のHCN音量復帰が働きます。

`play.hide_misslayer_on_good`は既定OFFです。ONの通常snapshot経路では表示判定履歴の
発生順と`DisplayJudgementEvent.display_time`を使ってPGREAT / GREAT / GOOD時に
ミスBGAを解除するため、正負の入力オフセットで解除を遅らせません。
ミス画像は判定の譜面時刻から選びます。OFFでは、GOOD以上の判定でミス画像を解除しません。

## Diagnostics と検証

INFOにgameplay / inputの専用thread、audio schedulingの担当、play generationを記録します。
rendererの既存初期化ログが バックエンド、実効present mode、maximum frame latencyを記録します。

DEBUGでは5秒ごとにgameplay advance interval、input event age、collect → judgement、
judgement → enqueueのavg / p95 / p99 / maxを集計します。histogramのpercentileは
bucket上限による近似値です。audio scheduling latenessはcallback到着時のscheduled frame差を
frame / usで出します。renderのacquire / encode / submit / present / pacing wakeは既存
frame profilerを使い、present間隔の分布を追加しています。

```powershell
cargo test -p bmz-player gameplay_runtime::tests
cargo test -p bmz-player input::
cargo test -p bmz-audio command::tests
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt --check
```

stall harnessは実際の専用workerと240fps相当consumerを使い、consumerを
0 / 16 / 33 / 100 / 250ms停止します。交換mutexの保持中にも進行し発音すること、
旧advanceとのscore / gauge / LN state / replay / result一致、autoplay / replayの一致、
callback到着遅延がrender stallによって増えないこと、snapshot時計の非逆行を検証します。
物理デバイス、driver、DWM、実GPUの遅延はこのharnessの測定対象ではありません。
## macOS入力・音声診断への案内

macOSの独立IOHID入力、時計変換、診断の測定境界と100ms停滞試験は
[macOS遅延検証](macos-latency.md)を参照してください。winit経路のOSイベント時刻は測定不可として扱い、
入力キューの観測時刻と判定用タイムスタンプを分離します。

Linuxのgilrsは公開blocking APIで入力到着またはスクラッチ期限まで待機します（制御応答上限50ms）。
任意evdevはnative X11/logind/デバイス権限に加え250msのwindow側フォーカス確認期限を要求し、
期限切れ中は新しい押下だけを抑止します（押下中キーの解放は配送します）。Wayland/XWayland/Flatpakではwinitです。
診断・入力源切替・欠落復旧の詳細は[Linux遅延検証](linux-latency.md)を参照してください。
