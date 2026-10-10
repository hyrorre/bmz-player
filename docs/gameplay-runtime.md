# Gameplay runtime

Play 開始後の `GameSession` は `bmz-gameplay::runtime::GameplayRuntime` が所有し、
`bmz-player::gameplay_runtime` の専用スレッドだけが変更する。
window/event-loop スレッドは入力設定、開始前準備、画面遷移、永続化、GPU、BGA upload、
skin、egui、surface acquire / submit / present を担当する。

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

- Windows キーボードは入力専用スレッドの message-only window で Raw Input を受け取る。
  message 取得時に既存 `monotonic_timestamp_ns()` で timestamp を確定する。
  foreground window を入力スレッドでも確認する。キーの repeat / release と scancode は
  winit の PhysicalKey と既存の共通 `DeviceInputEvent` へ正規化する。
- Windows HID Raw Input は同じ message thread が取得する。
- gilrs / experimental GameInput と analog scratch の polling・停止検知も入力スレッドで行う。
  GameInput の構築、COM 使用、破棄は同じ owner thread 上に保つ。
- macOS GameControllerは専用の直列dispatch queueから直接SharedInputBackendへ配送する。
  macOS 14以降は入力履歴を通知時に全件drainし、11〜13は要素別callbackを使う。
  scratch停止は同じqueueの1ms timerで確認する。履歴の軸処理は入力サンプル時刻で進める。
- Gameplay に送る入力と、メニュー・キー設定用の UI コピーは別キューにする。
  UI のコピーが滞留しても gameplay の配送には影響しない。
- macOS / Linux のゲームパッドも独立取得する。macOSキーボードは`macos-iohid` featureで有効化したIOHID専用run loop、
  またはmacOS 11以降のGCKeyboard専用dispatch queueで独立取得できる。
  winitを選んだ場合とLinuxではwindow/event-loop停止中のキーボード取得遅延が残る。
- 判定時刻の変換は既存の `AudioClock`、`InputTimestampAnchor`、input offset を使う。
  wall clock の時刻補正に依存しない。GameInput 等の既存 backend timestamp 変換は維持する。

## Wake と audio

入力到着と control command は gameplay thread を `unpark` する。
次のノート、見逃し期限、LN 終端、auto key release、replay、BGM schedule-ahead の期限と
2ms の safety wake のうち早いものまで `park_timeout` で待機する。
safety wake は HCN や時計の進行を補完するもので、renderer の tick に依存しない。

既存の `advance_session_frame` が入力、判定、Mine、見逃し、LN/CN/HCN、ゲージ、score、
replay recording、autoplay、終了判定を処理する。keysound と BGM は `ScheduledSound` の
既存 frame 座標を保って AudioEngine の有界 command queue へ送る。
送信できなかった keysound と HCN 音量更新は次の wake で再送する（途中 FAILED 時は破棄）。
判定ガイド音、既定の地雷 SE、失敗 SE も gameplay が直接送信する。
callback には判定・入力処理を移していない。command scratch と通常再生用 queue / voice 領域は
callback 接続前に事前確保し、callback は
queue と engine の `try_lock` に失敗した場合も待機しない。

macOSの実験用Core Audio IOProcも同じNativeOutputRenderer・source command queue・
AudioEngineを使う。HALの出力バッファへfloat PCMを書き、出力時刻の大きな欠落は既存の
frame座標を前進させて処理する。取得・判定・音声コマンドの所有権と旧playの取消を維持する。
IOProcの登録・開始・停止・解除とcontext破棄はapp側で行い、停止・解除後にcontextを解放する。
出力形式やレートが変わった場合は無音化し、設定再適用による再生成を必要とする。

## Snapshot とイベント

window thread は live GameSession を参照せず、detached `PlaySessionObservation` と
immutable publication を受け取る。publication は最新の 1 件だけを保持する。
交換区間だけで双方が `try_lock` を使い、古い publication の破棄、snapshot 構築、GPU 処理は
ロック外で行う。renderer が交換 lock を保持しても gameplay は進行する。

snapshot は consumer の要求時と最大 8ms 間隔で生成し、入力のない safety wake ごとの
snapshot allocation を避ける。score と result graph は snapshot の生成頻度から独立して記録する。

ノーツ座標は gameplay thread では確定しない。publication に付随する `PlayfieldProjection`
は譜面・TimingMap・scroll cache と、判定済みノーツ・LN/HCN状態・表示オプションの
読み取り専用コピーを持つ。chart-sized の判定mapを事前確保した3つのArcバッファを再利用し、
consumerが参照していないバッファだけを更新する。GameSessionや判定engineは渡さない。

consumerは取得したAudioClockの現在時刻で、ノーツ・Mine・LN端点・ガイド線・表示BPMを
既存のスクロール計算から投影する。同じpublicationを再利用しても表示座標は進む。
可視ノーツだけを外挿する方式ではないため、stall中に新しく可視範囲へ入ったノーツも描画できる。
STOP、SCROLL/SPEED、CONSTANT、note retention、PMS見逃し表示の計算は準備画面と共通。
consumerの投影はgameplayのadvance、入力判定、audio enqueueを呼ばず、交換lockの外で行う。

入力・判定の presentation event は既存 sequence と game time を保持する。
consumer が受領した sequence までを acknowledgement で破棄する。
skin runtime は既読 sequence を再処理せず、timer はイベントの元時刻から評価する。
これにより、古い bomb / keybeam を復帰時点から新しく再生しない。
累積状態を使う observer には、受領まで保持した全イベントを渡す。
極端に長い描画停止へのメモリ上限は 8,192 events。上限を超える演出履歴は古いものから捨てる。
判定、replay、結果、発音にはこの破棄を適用しない。

判定画像・コンボ・ボムに渡す `DisplayJudgementEvent.display_time` は、実入力では
入力オフセット補正前の譜面時刻を使う。正のオフセットでも打鍵直後から表示を開始し、
負のオフセットでもアニメーションを途中から始めない。入力配送・描画が遅れた場合は
打鍵からの経過時間を維持し、受領時刻から再生し直さない。見逃し・autoplay・replay・
対戦相手は元の判定時刻を使い、現在のprofileの入力オフセットを差し引かない。
表示履歴の800ms保持期間もこの表示時刻で計算する。採点用の `JudgementEvent`、
FAST/SLOW、保存する結果・replay、キー音、順序付き `SkinRuntimeEvent` の時刻は変更しない。

自動 timing 調整で負方向へ visual offset が変わる場合は、描画投影の時刻だけを前回値以上に
抑える。判定用 offset、保存値、FAST/SLOW、replay の時刻は変更しない。
seek / retry は新しい generation なので、この visual floor もリセットする。

## Lifecycle

READY 前は準備用 runtime をローカルに保持し、音声時計の開始後に worker へ所有権を移す。
各 play は単調増加する generation、入力 queue、snapshot、command channel を持つ。
Viewer seek でも入力 queue を作り直す。入力ルートの切り替え時は旧 sink へ release を送り、
新しい play に旧押下状態を持ち越さない。

windowの実効focus変更は、そのイベント処理中に入力captureのrouteへ反映する。
Waylandの非表示等でRedrawRequestedが止まっていても、喪失時の保持解放と入力抑止、
復帰時の配送再開を次の描画まで待たない。gilrs workerの反映待ちは既存の最大50ms以内。

worker の停止は atomic flag と unpark で要求する。UI は終了済みの worker だけ join し、
まだ終了していない worker の後始末を待たない。audio command も停止 flag を保持し、
enqueue 時と callback の適用時に検査する。旧 worker の遅い送信を次の play に適用しない。

worker はスコア確定時・終端遷移時に immutable `FinishSessionSnapshot` と graph を発行する。
通常 play、course、replay、abort、結果画面への遷移はこの結果から従来の storage / IR 経路を呼ぶ。
UI は終了要求を送った直後の古い観測値を保存せず、worker の確定結果を待つ。

### 途中FAILED時の譜面音声

手動中断（Esc / E1+E2 等）またはゲージ枯渇によって `PlayState::Failed` が確定すると、
`GameplayRuntime` はそのプレイの譜面音声を停止する。閉店演出開始時の要求であり、
描画更新・RESULT入場・システム効果音の初期化を待たない。Practiceで中断と同時に
時計が停止しても、停止要求と未送信要求の破棄を行う。

- `flush_audio()` はFAILEDフレーム内で生成済みのBGM・キー音、先行予約、再送待ちの
  発音・HCN音量変更を破棄する。通常の発音時刻・入力／音声オフセットは変更しない。
- プレイ別の `AudioEngineHandle::stop_playback()` は、有界キューとは独立したatomicの
  停止フラグを保持する。送信時とcallback適用時に要求を拒否し、遅着・並行送信からの
  再発音を防ぐ。workerの既存取消フラグとは別なので、worker破棄で停止自体は取り消されない。
- callbackは停止フラグを観測した時点で `clear_playback()` により全voiceと予約を消去する。
  キューの `try_lock` が失敗しても消去する。engineを取得できないcallbackは従来どおり無音で返し、
  停止フラグは次回以降まで保持する。新しいロック待ち・I/O・ビジーループを追加しない。
- デコード済みsample bankを保持し、`clear_playback()` に伴うgainリセットは元の値へ戻す。
  pause状態、playback rate、音声時計は変えない。既にデバイスへ渡したバッファの巻き戻しは行わない。
- 停止は当該sourceの寿命中は解除しない。同じ停止の再要求は安全で、`mark_draining()`でも
  復活しない。再プレイは既存経路の新しいengine / handleを使うため、古い要求は作用しない。
  Practice等が共有するデコード済みPCMは再利用できる。シーク用の `replace_playback()` は
  終端停止の解除APIではなく、途中FAILEDしていないViewer sourceの差し替えに使う。

判定条件は最終結果のクリアランプではない。`PlayState::Finished` の通常完走と
全ノーツ処理後の終了操作は余韻をRESULTのdrainingへ引き継ぐ。完走時のFAILEDランプも同じ扱いで、
RESULT退出時の既存フェードを維持する。GASで別ゲージへ移って続行する場合も停止しない。
PlayStop、RESULTのBGM / SE、スキン音声は別のsystem sourceで再生し、この停止の対象外とする。

AUTOPLAY / REPLAY / PRACTICE / コースも実際に `Failed` へ入る場合にだけ共通処理を使い、
終了理由・画面遷移は変更しない。Viewerの終了・待機・一時停止・シークは従来の専用経路を維持する。
スコア保存・IR・リプレイ記録の条件も変更しない。

調査と回帰テスト、実機確認手順は
[issue #26の作業記録](../notes/2026/2026-10-05-failed-chart-audio.md)を参照。

## 任意の終了・発音設定

プロファイルの `play.wait_all_notes_result` は既定OFF。ONでは通常終了に使う
`GameSession.result_wait_end_time` を、譜面末尾とノート・不可視ノート・BGM・通常BGA
レイヤー（Base / Layer / Layer2）の最終配置時刻の最大値にします。その時刻から
従来どおり譜面時刻で5秒の余白を経て終了します。PoorだけのBGA・BPM等の制御イベント・未使用音声定義・
PCM長は延長に使いません。`chart.end_time`、結果確定の `result_is_settled`、保存用の
譜面長・identityを変えず、終了時の確定snapshotから既存Result遷移を通します。
Practiceは設定の対象外で、Replay / Autoplay / Courseは通常終了時に適用します。
手動終了と途中FAILEDには待機を追加せず、上記のFAILED音声停止を維持します。

`play.play_keysound_on_miss` も既定OFF。ONでは見逃し処理から発音対象だけを
`KeySoundEvent` として出し、既存schedulerのキー音音量・正規化・多重定義・同一音の
再発音規則を使います。音はPOOR確定時刻で予約し、判定オフセットを再適用しません。
通常の自動キー音が有効なら追加分を抑止します。Replayでも現在のプロファイル設定を
使い、判定・ゲージ・保存される入力列やスコア区分には含めません。
見逃したHCN始端は上流の処理順に合わせて発音した更新ではミュートせず、次回更新から
通常のpassing音量制御を適用します。押し直すと既存のHCN音量復帰が働きます。

`play.hide_misslayer_on_good` は既定OFF。ONの通常snapshot経路では表示判定履歴の
発生順と `DisplayJudgementEvent.display_time` を使ってPGREAT / GREAT / GOOD時に
ミスBGAを解除するため、正負の入力オフセットで解除を遅らせません。
ミス画像は従来どおり判定の譜面時刻から選び、OFFでは従来の表示条件を維持します。

## Diagnostics と検証

INFO に gameplay / input の専用 thread、audio scheduling の担当、play generation を記録する。
renderer の既存初期化ログが backend、実効 present mode、maximum frame latency を記録する。

DEBUG では 5 秒ごとに gameplay advance interval、input event age、collect → judgement、
judgement → enqueue の avg / p95 / p99 / max を集計する。histogram の percentile は
bucket 上限による近似値。audio scheduling lateness は callback 到着時の scheduled frame 差を
frame / us で出す。render の acquire / encode / submit / present / pacing wake は既存
frame profiler を使い、present 間隔の分布を追加している。

```powershell
cargo test -p bmz-player gameplay_runtime::tests
cargo test -p bmz-player input::
cargo test -p bmz-audio command::tests
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt --check
```

stall harness は実際の専用 worker と 240fps 相当 consumer を使い、consumer を
0 / 16 / 33 / 100 / 250ms 停止する。交換 mutex の保持中にも進行し発音すること、
旧 advance との score / gauge / LN state / replay / result 一致、autoplay / replay の一致、
callback 到着遅延が render stall によって増えないこと、snapshot 時計の非逆行を検証する。
物理デバイス、driver、DWM、実 GPU の遅延はこの harness の測定対象ではない。
## macOS入力・音声診断への案内

macOSの独立IOHID入力、時計変換、診断の測定境界と100ms停滞試験は
[macOS遅延検証](macos-latency.md) を参照。winit経路のOSイベント時刻は測定不可として扱い、
入力キューの観測時刻と判定用タイムスタンプを分離する。

Linuxのgilrsは公開blocking APIで入力到着またはスクラッチ期限まで待機します（制御応答上限50ms）。
任意evdevはnative X11/logind/デバイス権限に加え250msのwindow側フォーカス確認期限を要求し、
期限切れ中は新しい押下だけを抑止します（押下中キーの解放は配送します）。Wayland/XWayland/Flatpakではwinitです。
診断・入力源切替・欠落復旧の詳細は[Linux遅延検証](linux-latency.md)を参照してください。
