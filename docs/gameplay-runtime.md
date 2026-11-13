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

## Play前のロード

Play前のキー音は、同じ実ファイル・出力レート・VOLWAVのデコード済みPCMを再利用します。
通常ファイルのサイズ・更新日時・作成日時（Unixではinode / ctimeも）、書庫で読み込んだbytesの
同一性を確認し、変更・削除・読み込み失敗を次のロードへ反映します。
PCMとキャッシュが保持する書庫bytesは合計512MiB・4,096件までで、古いものから退避します。
再生中のsample bankはArcで所有し、退避しても影響しません。リトライでも新しいaudio engineを作り、
region・scheduler・判定状態は引き継ぎません。cacheはプロセス内だけで、音声callbackは操作しません。

音源1ファイルは8チャンネル・384,000 Hz・20分・デコード後512 MiBまでです。
専用WAVローダーとFFmpegの両方で、PCM確保前とデコード中に検査します。
出力レートへの変換後も同じ上限を適用し、超えたファイルだけを読み込み失敗として扱います。
譜面全体ではPCMの確保容量を合計3 GiBまでとし、キャッシュで共有する音源も数えます。
リサンプル中の入力と出力も予算に含め、複数のworkerで同じ予算を共有します。
合計を超えると残りのデコードを中止して、その譜面のロードをエラーにします。
同一pathの音源を複数のSoundIdやBMSONのregionで使う場合は重複して数えません。

## 入力と時計

ロード時に変換済み譜面のノートID・HCN区間索引と判定map容量を準備します。
入力候補とMineは時刻窓から調べ、HCNは現在時刻を含み得る区間だけ更新します。
別の譜面に差し替わった場合は索引を使わず、通常の探索へ戻ります。

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
- window/event-loopスレッドのUIコピーは、`ControlInputEvent`から`input_resolve::resolve_input`だけを通して
  機器・スロット・設定対象モードを解決し、解決済みの`ResolvedInput`から操作を決めます。
  進行中のプレー（読み込み中・プラクティスを含む）では、UIコピーもノーツ入力と同じく開始時の1P / 2Pの割り当てで解決します。
  解決の規則と、入力名を直接見る残りの箇所は[操作方法](controls.md#1p--2pの入力の解決)と
  [作業記録](../notes/2026/2026-11-11-input-resolution-pipeline.md)を参照してください。
- macOS / Linuxのゲームパッドも独立取得します。macOSキーボードは`macos-iohid` featureで有効化したIOHID専用run loop、
  またはmacOS 11以降のGCKeyboard専用dispatch queueで独立取得できます。
  winitを選んだ場合とLinuxではwindow/event-loop停止中のキーボード取得遅延が残ります。
- 判定時刻の変換は既存の`AudioClock`、`InputTimestampAnchor`、input offsetを使います。
  wall clockの時刻補正に依存しません。GameInput等の既存 バックエンドtimestamp変換は維持します。

## Wake と audio

入力到着とcontrol commandはgameplay threadを`unpark`します。
次のノート、見逃し期限、LN終端、auto key release、replay、BGM・自動キー音のschedule-aheadの期限と
2msのsafety wakeのうち早いものまで`park_timeout`で待機します。
safety wakeはHCNや時計の進行を補完するもので、rendererのtickに依存しません。

既存の`advance_session_frame`が入力、判定、Mine、見逃し、LN/CN/HCN、ゲージ、score、
replay recording、autoplay、終了判定を処理します。keysoundとBGMは`ScheduledSound`を
AudioEngineの有界command queueへ送ります。
送信できなかったkeysoundとHCN音量更新は次のwakeで再送します（途中FAILED時は破棄）。
判定ガイド音、既定の地雷SE、失敗SEもgameplayが直接送信します。
callbackには判定・入力処理を移していません。command scratchと通常再生用queue / voice領域は
callback接続前に事前確保し、callbackは
queueとengineの`try_lock`に失敗した場合も待機しません。

CPAL callbackとnative backendが共有するmix・PCM変換・buffer・診断は
`bmz-audio/src/backend/output.rs`に置きます。デバイス固有のstream操作と時刻変換は各backendが担当します。

### 音声予約と時計

判定・描画・BGAの譜面時刻は `AudioClock` の `Instant` で進めます。デバイスの進行速度や
タイムスタンプを判定時計へ戻しません。入力の `InputTimestampAnchor` も同じ1回の
`Instant` 観測から単調時刻と譜面時刻を求め、別々の取得間に入るスレッド停止を除きます。

BGM・自動キー音（フルオートを含む）は、常に実時間100ms先までシステム時刻のdeadlineで予約します。
押鍵に応じたキー音・判定ガイド音は処理後に即時要求します。旧設定 `audio.schedule_ahead` は無視します。

予約は `ScheduledSound.start_time` に `Instant` のまま保持します。各callbackで
バッファ先頭の時刻と `current_frame` を対応付け、そのバッファに入る予約だけを
frameへ変換します。曲開始時のframeと公称レートによる固定変換をライブ予約には使いません。

バッファ先頭の時刻は、frameをデバイスの進行に沿って時刻へ写した値です。callbackの到着は揺れますが、
frameはデバイスの速度で進みます。「callback時刻から半周期を引いた時刻」と「frameの公称時刻」の差を
時定数2秒で平滑化して公称時刻へ足すので、到着の揺れは消え、時計速度の差は追従残差になります
（200ppm・2秒で約0.4ms）。半周期を引くのは、即時要求の音が次のcallbackまで平均で半周期待つためです。
これで予約音の平均の遅れが押鍵音・判定ガイド音に揃います。
各バッファは開始時刻と終了時刻の範囲を持ちます。平滑値の更新は終了時刻へ反映し、次のバッファはそこから始まるので、
frameが連続している間は境界に隙間ができず、deadlineを読み飛ばしません。
範囲内のdeadlineは、開始～終了時刻に対する割合で、そのバッファ内のframeへ変換します。
平滑化で範囲の長さが変わっても予約が次のバッファへはみ出さず、境界を挟む発声順を維持します。
この変換は発声開始位置だけに使い、開始時刻より前の遅刻量は公称レートで計算します。

停止直後に長いバッファが来ると、次のバッファで平滑値が大きく戻り、範囲の終了が開始より前になることがあります。
そのバッファに範囲内のdeadlineはなく、遅刻した予約だけをそのバッファの先頭から扱います。
次のバッファは手前へ戻った終了時刻から始まりますが、予約は一度しか解決しないので二重には鳴りません。

APIの出力時刻予測は使いません。実機の記録では、予測を使う方式のほうが揺れが大きくなりました
（[検証記録](../notes/2026/2026-11-06-audio-system-clock-scheduling.md)）。

callback間隔が供給範囲を許容幅より超えたら、停止として平滑値を初期化し、時刻を現在のcallbackへ合わせます。
供給範囲は前回・今回のバッファの長い方、許容幅は短い方のバッファから求めた20msかつ2バッファ分です。
初期化の後は単純平均から始め、1回のcallback遅配の影響を抑えます。

遅れたBGMは既存のcatch-up、キー音はサンプル先頭を保持します。
即時要求は次に描画する音声バッファの先頭で発声し、絶対frame 0への予約とは区別します。
同じ音源は解決後の開始frame順に処理し、同じ開始位置の要求はバッチをまたいでも投入順を維持します。
先読み幅は25～300%の再生速度を考慮し、wakeの期限も同じ幅を使います。

出力が止まった区間は、既存voiceと予約音へ同じframeの前進として一度だけ反映します。欠落量の求め方はbackendごとに異なり、
どれも許容幅を超えたときだけ補正します。CPALは、APIのplayback値の進みが前回バッファの長さを超えた分と、
callback間隔が供給範囲を超えた分の小さい方です。API値を比較できないときはcallback間隔だけを使います。
IOProcはMach host timeとcallback間隔の小さい方、WASAPI排他はcallbackが周期より遅れた分です。
これらはAPIとcallback間隔による推定で、物理出力の測定ではありません。

再生要求の後に始まったcallbackを制御側で確認してから曲の原点を確定します。2秒待っても来ない場合は
警告を記録して開始します。WASAPI排他の開始前prefillだけで準備完了とはしません。callback内で開始待ちはしません。
pause/resumeでは未発声deadlineも停止時間だけ移動し、速度変更時は残り時間を再計算します。
seek/retry/終了取消は既存の予約・voice破棄を維持します。オフライン出力・テストの仮想時計は
`start_time = None` のframe予約を使い、実時間やデバイスに依存しません。

開始済みWAVの進みはデバイスのサンプル進行のままです。ASRCによる連続的な速度補正は含みません。
長いWAVは、判定・ノーツ表示と、後から予約する音の両方に対して「時計差×そのWAVの経過時間」だけずれます
（40ppmで5分なら約12ms）。旧方式の音同士は同じframe座標で揃っていたので、長いWAVと後続の音のズレは
この方式で生じるものです。確認したbeatorajaのPortAudio経路も同じ関係です。短いWAVだけの譜面には
影響しません。区間分割のBGMは区間ごとに戻るので、継ぎ目に「時計差×区間長」の隙間か重なりが出ます。

即時要求の音には次のcallbackまでの0〜1周期の揺れがあります。先読み音との差は平均でほぼ0ですが、
1音ごとには残ります。ここで揃えるのは音声バッファ待ちと出力先行量であり、入力受信・判定処理・
gameplayのwakeからコマンド投入までの遅れは別に加わります。比較時は開始時の位相差と時間経過に伴う傾きを分け、PCM書込・API予測・
物理発声を同一視しないでください。

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

KEYON / KEYOFF・HOLD・HCN・ゲージ増加 / 最大・FULLCOMBOの演出は、publicationへ保持した
開始時刻からconsumerの譜面時刻で経過時間を求め直します。相手側のゲージ・FULLCOMBOも同様です。
RHYTHM・4分拍・ENDOFNOTEは現在の譜面位置から再計算し、同じsnapshotを再利用しても進めます。
押下 / 解放の識別には元の開始時刻を使い、画面時計と譜面時計の差や再生速度、ms丸めによる
開始時刻の揺れで新しい入力と判断しません。準備中は画面時計を使い、開始前のキャンセルでも
終了演出へ経過時間と入力状態を引き継ぎます。

PMS見逃しノーツの落下計算は、譜面準備時に作る`MissedNoteFallCache`を共有します。
BPMごとの累積落下量（STOP中は一定）と各ノーツのSTOP終了時刻を保持し、
現在時刻と落下開始時刻の累積値の差で位置を求めます。判定幅は現在のwindowから適用します。
レーンごとのSTOP終了・予定時刻のprefix最大値を二分探索し、8小節の保持上限を超えた範囲を
走査前に除きます。`note_retention`でも期限切れの範囲を広げません。
cacheは不変で、時刻の巻き戻しにも対応し、フレームごとの履歴追加や再積分は行いません。

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

### 採点の確定とResultのゲージ履歴

採点は1プレイにつき一度だけ閉じます。`advance_session_frame`がその更新で取得した入力・
見逃し・地雷・LN/CN/HCNの処理を終えた後に`GameSession.scoring_closed_at`を設定し、
所有スレッドは同じ状態からスコア、クリア種別、全ゲージ、コンボ、判定詳細、記録用リプレイ、
コース引継ぎ値を取得します。確定時刻は以下のどちらかです。

- 通常完走（ゲージ不足のFAILEDランプを含む）は、`natural_result_settled`が真になった更新。
  全ノーツが判定済み（`judge.is_exhausted()`）で、`E = judge.result_end_time(chart)`、
  `W = window_set.result_settle_margin_us()`（ルール・キー種別・ランク・再生速度を反映した実効窓）、
  `O = offsets.input_offset_us`について次を満たした時点です。
  - 人間入力が採点対象に残る場合（部分オート・部分リプレイを含む）: `audio_now > E + W + max(0, -O)`。
    取得時刻にOを加えた判定時刻が`E + W`以下なら空POOR等になり得るためです。
  - フルリプレイ: 実際に処理を終えた1ms tick（`ReplayPlayer::scored_until`）が`E + W`を超えたとき。
    記録済み入力に現在のOを再適用しないため、負オフセット分は待ちません。
  - フルオート: `audio_now > E + W`。
  - 独立した対戦相手がいる場合は、その判定も同じ条件で終わっていること。
  判定窓の境界（`delta == W`）は受け付けるため比較は厳密な`>`です。
  `E`はTap・LN始終端・地雷の最大時刻で、キー音切替だけを行う不可視ノーツは除きます。
  早離し・始端見逃し後にもゲージが変わるHCNを考慮し、LNペアの終端も含めます。
  判定対象が無い譜面だけは従来の`chart.end_time`を使います。本人と独立した対戦相手に同じ条件を適用します。
  `chart.end_time`自体は変えず、不可視ノーツのキー音、BGM、画面の長さと通常の退出待機は維持します。
- 途中FAILED（HARD等のゲージ枯渇）・手動中断・全ノーツ処理後の明示的な退出は、
  `Finished` / `Failed`へ入った停止境界の更新。自然確定の判定窓は待ちません。

確定後の入力はスコア・ゲージ・判定詳細・記録用リプレイへ渡さず、キービーム、skin入力イベント、
押鍵時のfallbackキー音だけを進めます。BSS終端のRelease直後に設定した逆方向Pressの抑止
（[ln.md](ln.md)）は確定後も同じ判定処理で消費し、fallbackキー音へ流しません。確定後に古いタイムスタンプの入力が届いた場合や、
プレー中にオフセットを変更した場合も確定済みの結果は変わりません。任意の入力遅配を待つ固定の待機は
設けていません。BGM・自動キー音、音声末尾、BGA、スキンtimer、オプション操作は採点と独立して続き、
Play画面の退出は従来どおり5秒の余白・BGMスケジューリング完了・`wait_all_notes_result`に従います。

手動中断（`request_manual_stop`）と全ノーツ処理後の退出（`request_finish`）は、UIの観測が古い場合に備えて
gameplayの所有スレッドで再確認します。全ノーツ処理後・確定後の中断要求は無視し、
途中FAILEDの後に届いた退出要求で`Finished`へ書き換えません。Practiceは要求の直後に時計を止めて
終了演出へ進むため、`request_practice_round_end`を使います。ノーツが残る間は手動中断、
所有スレッドで既に全ノーツを処理していた場合は明示的な退出として`Finished`にし、
そのラウンドの結果を停止境界で取得できるようにします。

Resultのゲージ履歴は500ms周期の定期記録に加え、結果を確定した譜面時刻に全ゲージの
`value / max / border / gauge_type`を終端点として一度だけ記録します。同じゲージ・同じ時刻の
定期点があれば最新値へ更新し、時刻順を保ちます。LR2 / beatoraja JSON / LuaのResultは
選択ゲージの履歴末尾を表示値として使うため、確定結果と表示が一致します。表示の丸め
（LR2の2%刻み、beatorajaの切り捨て）は従来どおりです。`Finished`への遷移では結果を取り直さず、
退出時のプレー時間だけを終端時点の値に更新します。`PlayState::Failed`の0埋めは終端点より後の
定期サンプル位置から始まり、停止時の実値の後にゲージが戻ることはありません。
gameplay workerを使わず`GameSession`を直接保存する経路（同期・非同期の保存）も、
同じ確定時刻と終端点を使います。経緯と検証は
[作業記録](../notes/2026/2026-11-11-result-gauge-settlement.md)を参照してください。

### 音声・動画資源の後片付け

audio callbackから退避したsourceは、出力streamごとのworkerが回収します。
事前確保したbufferを短いlock内で交換し、大きなPCM bankの解放はlock外で行います。
window threadの通知はtry_lockだけで、空のqueueではworkerを起こしません。
描画停止中も20ms周期で回収し、stream停止後の残りもworkerで解放します。

BGA動画は表示中の各channelに加え、最後に使った非表示10本までdecoderを保持します。
1プレイで十数本以上を切り替える譜面だけ、古いものから閉じます。
プレイ中の譜面BGAは、デコードスレッドで表示に必要な寸法まで縮小してからGPUへ転送します。
初回は実効Playキャンバスの大きさを使い、描画計画を作った後はBGAの表示先とUVから必要な寸法を求めます。
同じ動画の表示先が複数ある場合は軸ごとの最大値を使い、ウィンドウのリサイズや表示先の変化は
新しくデコードしたフレームから反映します。スキンの配置とクロップには元動画の寸法を使います。
プレイ用デコーダは使い終えた画素バッファを1本まで受け取り、後続フレームに再利用します。
縮小不要の大きな8 bit YUV420動画のうち、BT.601のリミテッドレンジで扱える素材は
Y・U・Vの各面をGPUへ送り、描画前にRGBAへ変換します。他の画素形式や色空間は従来のRGBA変換を使います。
動画書き出しとスキン内の動画は原寸のRGBAでデコードします。
動画を開くときやデコーダを別のイベントへ割り当てるときは、イベントから250ms未満なら先頭から
デコードします。250ms以上進んだ位置から再生する場合は、その再生位置へのseekを試みます。
リトライでは静止画と動画デコーダを再利用し、動画は今回のフレームを取得してから表示します。
チャネル方式では、VC-1デコーダがパケット投入時に`EPERM`を返した場合、そのパケットを飛ばして
後続のフレームを取得します。この処理はプレイ中のBGAと動画書き出しに適用します。
channel方式は先読みqueue満杯・EOFでparkし、frame取得・restart・停止で起こします。
skin動画のclock方式は従来の短い待機上限を保ち、停止通知で早く復帰できます。

動画のDropは停止を要求し、FFmpeg workerのjoinとRGBA解放を共通の退避workerへ渡します。
ファイルが閉じるのはこの処理完了時です。プロセス内のdecoderは退避待ちを含め64本までで、
上限到達時は新規openをエラーにします。native I/Oが停止してもthreadやframeが無制限に
蓄積しないための上限です。退避worker自体が異常終了した場合だけ同期cleanupへ戻ります。

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
