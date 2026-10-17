# macOS 入力・音声遅延の検証

実装の計測結果と未検証項目は[2026-09-30の記録](../notes/2026/2026-09-30-macos-latency.md)を参照してください。
既定の入力Auto（macOSではwinit）と音声Fixed 256 framesは変更していません。
判定窓、入力オフセット、判定・ゲージ、リプレイ形式も変更していません。

## 入力の切り替え

IOHIDは`bmz-player`の`macos-iohid` featureでのみコンパイルし、default featureには含めません。
利用する場合は`cargo run -p bmz-player --features macos-iohid`で起動します。
通常ビルドの選択肢にはIOHIDを表示しません。GCKeyboard/GameControllerはこのfeatureに依存しません。

設定 → 入力デバイス → キーボードバックエンドで`winit` / `macOS IOHID`を選びます。
保存先はapp configの`[input] backend = "Winit"` / `"MacOsHid"`です。
ゲームパッドを無効にしてもIOHIDを利用できます。UIの文字入力とIMEはwinitに残ります。
ネイティブ経路の動作中はwinitから同じゲーム入力を配送しません。

IOHIDは専用スレッドのIOHIDManager / CFRunLoopからSharedInputBackendへ直接配送します。
gameplayのwakerを起こし、ウィンドウ側のイベント消費や描画を待ちません。
メニューの操作はwinitイベントを使います。AppKitのapplication/delegateを作り直しません。
入力待ちはイベント駆動です。50msの待機期限はフォーカス・権限状態の保守と終了確認用であり、
キーを50msごとにpollするものではありません。

要求バックエンドと動作状態は別です。IOHIDManagerが開いている表示は、実キーの受信確認や
遅延改善の証明ではありません。無操作だけでは故障と判断しません。
macOS以外、または`macos-iohid`が無効なビルドでMacOsHidを読んだ場合はAutoへ移行する
（WindowsではRaw Input、それ以外ではwinit）です。IOHIDを再利用する場合はfeatureを有効にして選び直します。

### 権限・失敗・復帰

設定の`GCKeyboard`はmacOS 11以降のGCKeyboardを専用の直列dispatch queueで取得します。
入力監視の許可要求は行いません。キー別pressedChangedHandlerの引数から押下・解放を配送し、
判定用timestampはコールバック受信時の単調時計とします。プロフィールの最新イベント時刻を
個別キーの発生時刻として使わないため、OS取得→受信遅延は測定不可です。
複数のキーボードはOSが統合します。物理キー変換とJIS Native:MacOS表現はIOHIDと共通です。
macOS 11未満、キーボード未接続、またはプレイに割り当てたキーがAPIから得られない場合は
winitへ戻ります。接続済み表示は実キーの受信・性能確認の証拠ではありません。
フォーカス喪失・切断・route変更・停止時には保持を解放します。
GCKeyboard復帰時はwinit側で受理した保持も解放します。両経路の受理と配送を同じロックで排他化し、
接続状態の読み取りとキュー投入の間で入力源が変わる競合を防ぎます。
route変更時は世代を進め、古い世代のコールバックを拒否し、キーを離すまで新たな押下を受け付けません。
終了時はハンドラーと通知を解除し、専用キュー上の処理終了後にRust callback storageを解放します。
非対応OSではGCKeyboard/GameControllerを設定の選択肢に出しません。
APIの利用可否だけを確認し、未接続や入力監視の未許可を理由に選択肢を隠すことはしません。
保存済みの非対応バックエンドは読み込み時にキーボードAuto/ゲームパッドgilrsへ移行します。

### GameControllerゲームパッド

設定 → 入力デバイス → ゲームパッドバックエンドで`GameController`を選びます。
macOS 11以降で使用し、それ以前とmacOS以外ではgilrsへ戻ります。Auto/Gilrsの既定動作は維持します。
任意のHID/BMSコントローラーの認識を保証しません。認識されない機器はgilrsを選びます。
ゲームパッド未接続でもGameControllerで接続を待ち、台数0だけで自動切り替えは行いません。

- macOS 14以降: `GCController.input`の`unmappedInput`が得られればそれを使用し、
  `inputStateQueueDepth=128`、専用queueの`inputStateAvailableHandler`から
  `nextInputState`をnilまで処理します。128件を待ってから配送する方式ではありません。
- macOS 11〜13: `physicalInputProfile`のキー/軸別callback引数を使います。
  判定用timestampは受信時刻です。プロフィール全体の最新時刻は使いません。
- 新APIの`lastPressedStateTimestamp` / `lastValueTimestamp`はMach host timeの秒です。
  IOHIDのticksとは区別してBMZの単調時計へ変換します。古いrouteの時刻・非数・未来・
  スリープ前の履歴は拒否し、現在時刻の新たな押下へ置き換えません。
- 履歴差分が得られない場合は欠落として記録し、保持を解放してsnapshotを基準に再同期します。
  基準時点の押下は解放まで抑制します。完全な履歴を復元したとは扱いません。
  不正時刻・履歴欠落・route変更で解放したボタンは、UI再同期用の保持一覧からも除外します。
- gameplayへ直接配送し、UIコピーは4096件までです。UIコピーの欠落数とOS履歴欠落数は区別します。
- スクラッチ停止timerは1msです。実入力は毎回前面状態を確認し、idle timerの前面照会は10msごとです。
  frameworkのdeadzoneをbackendで再適用しません。既存のscratch閾値・軸のデジタル化契約は維持します。

入力名は列挙順やgilrsのraw番号ではなく次の固定名を使います。既存ButtonN/AxisNは移行しません。

| 入力 | 名前 |
|---|---|
| A/B/X/Y | GCButtonA / GCButtonB / GCButtonX / GCButtonY |
| L/R shoulder | GCLeftShoulder / GCRightShoulder |
| L/R trigger押下 | GCLeftTrigger / GCRightTrigger |
| L/R stick押下 | GCLeftStickButton / GCRightStickButton |
| Menu/Options/Home | GCMenu / GCOptions / GCHome |
| 十字キー | GCDpadUp / GCDpadDown / GCDpadLeft / GCDpadRight |
| L/R stick軸 | GCAxisLeftX / GCAxisLeftY / GCAxisRightX / GCAxisRightY |
| L/R trigger値 | GCAxisLeftTrigger / GCAxisRightTrigger |

軸方向のbinding名は上記軸名に`+`/`-`を付けます。十字キーは4方向のpressedを個別に扱います。
標準プロフィールの入力が対象であり、追加paddleや独自の相対軸はこの実装の対象外です。
公開APIに永続的な個体IDがないため、機器名をstable IDにせず`gc-session:*`の実行時IDを使います。
これらのslotはapp config保存・読み込み時に除去します。他backendの保存済み割り当ては維持します。
複数台では未指定slotを接続順から推測しません。切断・再接続時も新しい個体として明示選択します。
1台だけなら未指定slotを自動解決できます。

診断JSONの`kind=gamecontroller`は新APIで変換できたhost入力時刻→受信の分布と、
`invalid_timestamps`、`history_gaps`、`ui_overflow`、時計epochを出します。
旧APIのhost→受信は測定不可であり、空の分布を低遅延の証拠にしません。
起動・接続表示・APIのopen/closeテストだけでは実キー/実パッドの性能を確認したことになりません。

### IOHID

macOS 10.15以降はIOHIDCheckAccess(ListenEvent)を先に呼びます。未許可・拒否時には
managerを開かずwinitへ戻ります。起動時のIOHIDRequestAccess呼び出しは行いません。
設定の「入力監視の許可を要求」ボタンだけが許可要求を行います。
拒否後にOSがダイアログを再表示しない場合は、システム設定のプライバシーとセキュリティ →
入力監視から対象アプリを許可します。許可後は一度winitへ切り替えてからIOHIDを選び直します。
必要な再起動条件は実機ごとに記録します。今回、許可の付与・取り消し操作は行っていません。
10.13/10.14では存在しない権限APIを呼びません。最低対応版はIntel 10.13 / Apple Silicon 11.0のままです。

初期化失敗、コールバックエラー、権限失効、対応できないusageを検出した場合は
保持キーを解放してmanagerを停止し、winitへ戻します。設定の要求値は保持します。
設定画面とログで理由を確認し、再試行は同じ切り替え操作で行います。
Accessibility権限、root、TCC DB編集、SIP変更、デバイス排他取得は使いません。

標準USB keyboard usageの文字キー、数字、修飾キー、ナビゲーション、F1–F24、テンキー、
JIS固有キーを物理キーとして変換します。JIS Ro/かな/英数はwinit 0.30.13の
`Native:MacOS:*`表現を維持します。独自usage・consumer page・HID rolloverは
経路全体をwinitへ戻す対象です。FnなどOS/機器が通常キーとして通知しないキーは実機確認が必要です。
未知のキーを推測して別のキーに割り当てません。

物理デバイス別の保持を集約するため、二台で同じキーを保持しても片方の解放だけでは解除されません。
切断、フォーカス喪失、route世代変更で保持を解放します。
入力ごとに前面プロセスとWindowServerの可視ウィンドウを確認し、描画停止中にも背景入力を拒否します。
この照会のコストはネイティブ経路の実機負荷測定にも含めます。
終了時はowner threadでunschedule → callback解除 → close → CFRelease → context解放の順に処理します。

### 時計

IOHIDValueGetTimeStampはMach絶対時刻のticksです。mach_timebase_infoのnumer/denomで
u128乗算・整数除算してnsへ変換し、BMZのprocess-local Instantと対で採ったanchorへ対応させます。
SystemTimeを経由しません。過去イベントは表現可能なら元の時刻を保持し、同時刻イベントは
callback到着順でキューへ入れます。複数機器の時刻を並べ替えるための上書きは行いません。
ゼロ、未来、BMZ原点以前で表現不能、timebase不正は受信時刻へfallbackし、invalid件数を増やします。
そのサンプルをAの0nsとして集計しません。時計の対応が20ms超変化したら再基準化し、
そのイベントを無効扱いにします。mach_continuous_timeとmach_absolute_timeの進み方の差でも
スリープを検出して再基準化し、前epochの古いイベントを無効にします。
スリープをまたぐ正確な物理入力遅延は測定対象外です。

## 音声設定

設定 → 音声のバッファモードでAutoまたはFixedを選びます。Fixedは16〜4096 frames、
プリセットは16/32/48/64/96/128/256です。サンプルレートも既存項目で指定します。
CPALで対応しないレートはデバイス既定へ戻り、実際の値を記録します。
動作中のデバイス・レート・CPAL設定・対応範囲を設定画面で確認できます。
CPALのFixed値は実コールバックサイズの保証ではありません。実値は診断のframesを見ます。
範囲外要求は既存処理で範囲内へclampします。要求値とCPALへ渡した値の違いをログで確認します。
生成失敗はエラーログへ出し、設定再適用に失敗した場合は既存の前設定復旧処理を使います。
プレイ中に自動でバッファやレートを変更しません。元へ戻す場合は以前の値を選択して適用します。

Cargo.lockはCPAL 0.18.1 / gilrs 0.10.10（gilrs-core 0.5.15）/ winit 0.30.13です。
CPALのCoreAudio実装が既に固定バッファを設定するため、BMZ側から重複したHAL設定をしません。
CPALはcallbackのmHostTimeからplayback予測を生成し、device buffer frames（取得失敗時は
callback frames）とdevice latency + safety offsetを加算します。後二者の問い合わせ失敗を
CPAL内部で0にするため、BMZから予測の完全性を検証できない点に注意します。
デバイス変更後の内部推定値更新、外部機器、DAC、スピーカーの物理出力は別途検証が必要です。

### Core Audio IOProc（実験用）

```sh
cargo build -p bmz-player --release --locked
```

設定のバックエンドで`Core Audio IOProc`を選び、Fixed 16等を設定します。
`experimental-coreaudio-ioproc`はdefault featureに含まれるため、通常ビルドで選択できます。
`Auto`と`Core Audio`はCPAL / Audio Unit経路を使い、IOProcは明示選択時だけ使います。
`--no-default-features`でIOProcを除外したビルドは候補を隠し、保存済みIOProc設定をAutoへ戻します。
16が機器の最小値より小さければ対応範囲内へclampします。要求値・採用値・実callback framesを
区別します。サンプルレートはHALのnominal rateへ適用し、非対応レートや変更が完了しない場合は
明確なエラーにします。レート・バッファはデバイス共有の設定なので他アプリにも影響し得ます。

入力streamがない機器だけで`AudioDeviceCreateIOProcID`を使い、既存ミキサーと有界command queueを共用します。
このAPI自体は出力専用ではありません。入出力一体のUSB機器等は、共有レート/バッファの変更や
IOProc登録より前にエラーとして拒否します。入力構成の取得失敗も拒否し、登録直前・開始前にも再確認します。
該当機器では設定の`Core Audio`を選びます。設定適用に失敗した場合は既存の前設定復旧処理を使います。
登録時点で権限要求が発生し得るため、IOProcを一度登録してから入力streamを無効化する方法は使いません。
入力構成の変更通知でもstreamを無効化します。権限付与やentitlementの追加は行いません。
HALのvirtual formatがpacked native float32/float64 PCMである機器に対応します。
interleaved / non-interleavedと、機器チャンネル順に連続する複数output streamを扱います。
HALのstream configurationとvirtual formatのbuffer/channel構成を開始前に照合し、非対応構成はエラーにします。
既定機器も開始時に解決したUIDを固定し、OS既定出力の変更には自動追従しません。
デバイス切断・レートやvirtual formatの変更はcallbackで無音化し、設定再適用で再生成します。
動作中にバッファが変わっても4096 frames以下は実bufferで処理し、それを超える場合は無音化します。

`audio.prediction_ns`はIOProcの`inOutputTime.mHostTime − inNow.mHostTime`をMach timebaseで
nsへ換算した差です。ハードウェアへバッファ先頭が渡される予定時刻までのAPI値であり、
CPALのAudioUnit推定と定義が異なります。device/stream latencyやDAC・アナログ出力は含めず、
この値の差だけで物理遅延の改善を断定しません。ゼロ・逆行・1秒以上の差は無効扱いです。
IOProcの`processor_overloads`はHALの処理超過通知の累積で、stream errorやlock missとは別に記録します。
CPALではこの通知を監視しないため0を「超過なし」と解釈しません。
同じ機器・レート・バッファ・譜面・ビルド条件で比較し、16 framesでの長時間負荷と
電気的入力→アナログ出力の実測を別途行います。
実装・短時間検証は[IOProc作業記録](../notes/2026/2026-10-01-coreaudio-ioproc.md)を参照してください。

## 診断の読み方

`BMZ_LATENCY_DIAGNOSTICS=1`で有効、未設定または0で無効です。ログレベルはinfo以上が必要です。
`BMZ_LATENCY_JSON `に続くJSONが機械可読サマリーです。音声コールバックからのファイルI/O、
文字列整形、待機ロック、診断用動的確保は追加していません。
固定容量のatomic histogramを読み手が集計します。キー名・入力文字列を記録しません。

| 出力 | 境界・単位 | 含まないもの |
|---|---|---|
| iohid.os_to_receive_ns | A: OS時刻→Rustネイティブcallback入口、nsです。経路停止時に出力 | 物理接点→OS通知です。winitでは測定不可 |
| input_queue.enqueue_to_drain_ns | Bの部分計測: SharedInputBackend投入直前→gameplay取り出し、ns | native callback内変換・foreground照会、取り出し後の判定 |
| play_audio_commands.all_commands_enqueue_to_apply_ns | Cの部分計測: コマンドキュー投入→適用直前、ns | 音のrender開始です。BGM・自動キー音・制御コマンドも含むため手動キー音の値ではない |
| audio.frames | data.len()/実channel数、frames | 要求値との同一視 |
| audio.interval_ns | Instantで測ったcallback到着間隔、ns | underrunの確定診断 |
| audio.duration_ns | callback入口→render完了付近、ns | ハードウェア再生までの待ち |
| audio.prediction_ns | CPAL playback−callback、IOProc outputTime−now、各backend内の時計差、ns | Dの対象音別測定、アナログ出力実測です。バックエンド間で定義が異なる |

**対象音を一意に追跡した手動キー音C/DとA〜D合計は未実装・未測定です。**
この実装は既存の音声バッチ配送を保った部分計測であり、手動キー音・BGM・自動キー音の
合算値を「手動キー音遅延」と表示しません。物理スイッチからの総遅延も表示しません。
既存debugログのgameplay latencyには判定時刻からのevent ageが含まれるため、入力オフセット
適用前のA/Bと混ぜません。診断用受信時刻は判定用DeviceTimestampを書き換えません。

分布はcount / p50 / p95 / p99 / maxです。quantileは既存LatencyHistogram同様、対数bucketの上端
（実測maxで制限）です。maxは実測値、count=0は未測定であり0nsの測定結果ではありません。
live atomic snapshotは厳密に同時刻の値ではありません。音声分布は最初の2秒を除外します。
stream.idごとに別集計です。CPALでは1秒超のcallback停止・時計逆行・macOSのMach時計から検出した復帰後、
IOProcでは出力host timeの1秒超の欠落・逆行、callback到着の1秒超の停止、
Mach continuous/absolute時計の差で検出したスリープ復帰後にepochを増やし、
分布をリセットして2秒のwarm-upをやり直します。エラーカウンターはstream累積です。
スリープ復帰前後は別の実行として測るのが比較上確実です。
入力キューはplayごと、IOHIDは取得経路の開始から停止まで（時計再基準化でepochを分ける）、音声コマンドはengineごとの累積です。
入力・コマンド側のウォームアップは、同じ短い予備プレイ後に本計測のplayを開始します。

負の/ゼロ/1秒以上のCPAL予測差はinvalid_predictionsへ数え、0nsへ変換しません。
長いcallback間隔、timeline catch-up、stream error、lock miss、queue dropは別の事象です。
IOProcのみOSのprocessor-overload propertyを監視します。間隔だけから音切れ確定とは判断しません。

## A/B手順

普段のデータを保護する場合は、未使用の絶対パスをBMZ_DATA_DIRへ指定します。
resourceは同じcheckoutのdataを指定できます。既存DBを動作中にコピーしません。

```sh
cargo build -p bmz-player --release --locked --features macos-iohid
BMZ_DATA_DIR=/absolute/path/to/latency-test-data \
BMZ_RESOURCE_DIR=/absolute/path/to/bmz-player/data \
BMZ_LATENCY_DIAGNOSTICS=1 RUST_LOG=info \
target/release/bmz-player --boot-play-sample --smoke-exit-on-result \
  > /tmp/winit-256.log 2>&1
python3 scripts/compare-latency.py /tmp/winit-256.log /tmp/iohid-256.log
```

ローカルdebug `.app`の検証例（正式配布物の署名・notarization検証とは別）:

```sh
cargo build -p bmz-player --locked --features macos-iohid
bash scripts/package-macos-app.sh --debug --skip-build \
  --out-dir /tmp/bmz-latency-package --skip-rust-license-report
open -n -W '/tmp/bmz-latency-package/BMZ Player.app' \
  --env BMZ_DATA_DIR=/absolute/path/to/latency-test-data \
  --env BMZ_LATENCY_DIAGNOSTICS=1 --env RUST_LOG=info \
  --stderr /tmp/iohid-app.log \
  --args --boot-play-sample --latency-stall-test --smoke-exit-on-result
```

`open --env`は利用するmacOSの`open -h`で確認します。未対応の古いOSでは通常の.app起動で
設定画面から操作し、データパスの分離は[開発手順](development.md)に従います。
権限チェックが通っても`0xe00002c5`の場合はIOKitが排他アクセスエラーを返しています。
権限の問題と区別し、キーボードを排他利用する常駐ソフト/ドライバをユーザー自身で確認します。

これは手動プレイの例です。自動演奏で音声稼働だけ調べるときは`--autoplay-on-start`を追加し、
結果を手動入力計測と分けます。比較scriptは各ログの最後の累積サマリーをJSONで返します。
stream再生成やepoch変更を含むログは区間を分けて比較します。

1. Mac型番/CPU/OS、接続ハブ、キーボード・コントローラー、出力機器、実サンプルレートを記録します。
2. 同じ譜面・seed・描画設定・FPS・入力オフセット・音量を使います。手動キー音では自動キー音をOFFです。
3. 最初に入力だけwinit→IOHIDへ変更します。音声は256等の同一値を固定します。
4. 次に入力を固定し、Auto→256→128→64を一つずつ比較します。48kHz対応時は48kHzに固定します。
5. 診断0/1を比較し、通常起動と`--latency-stall-test`を別に記録します。
6. warm-up後に各条件を複数回、通常譜面と高密度・多重発音譜面で測ります。
   10分以上の連続動作も確認します。CPUは同じ測定窓のprocess CPU時間/実時間、音声処理時間は
   callback分布、描画時間は既存frame診断として分離します。ビルドや他の重い処理を同時実行しません。

`--latency-stall-test`はウィンドウ側だけを約2秒ごとに100ms停止します。既定OFFです。
画面バナーとタイトルにスコア・IR保存無効を表示し、通常/コース結果保存、IRジョブ生成、
背景IR同期を抑止します。終了してフラグなしで起動すれば元に戻ります。
この試験ではバナーを描くegui処理も有効になるため、CPU比較は同じ試験条件同士で行います。
停滞中の短い押下/解放とフォーカス移動を実キーで試します。
既存gameplay_runtimeの人工イベント試験はSharedInputBackendから後段の独立性の検証であり、
IOHIDの取得遅延測定ではありません。

### 実機チェックリスト

- CLI起動と配布.app起動を別々に試します。許可対象・署名・bundle idを記録します。
- 未許可、拒否、許可直後、アプリ再起動後、権限取消し後です。拒否時にも起動・winit操作できます。
- 短いタップ、長押し、連打、同時押し、左右修飾キー、JIS Ro/かな/英数です。
- 二台の同一キー保持、片方の解放・切断、接続追加、バックエンド切り替え、リトライです。
- 停滞中の他アプリへのフォーカス移動、最小化、解放、復帰後の押しっぱなしです。
- スリープ復帰後は別区間です。検索欄・設定欄での文字入力とIME、オーバーレイの入力抑止です。
- 起動、選曲、プレビュー、プレイ開始、リトライ、終了時の音声です。BMSコントローラーのscratchとrelease timeoutです。
- 内蔵/有線出力を基準に、外部機器は別結果として残します。利用できないIntel/Apple Siliconは未検証と明記です。

gilrsのnext_event_blockingはmacOSで内部channelのrecv_timeoutを使うが、公開APIに
設定変更・終了から待機解除する機構がありません。scratch期限やroute変更を安全に統合する範囲を超えるため、
macOSのgilrs取得ループは1ms待機で動作します。GameControllerはイベント通知で取得し、
別途1ms周期のタイマーでスクラッチ停止などを処理します。
Linuxのgilrsイベント待機は[Linux遅延検証](linux-latency.md#gilrsの待機と時計)を参照してください。

## 物理測定

同じ入力トリガーの電気信号と実際のアナログ音声出力を、同じオシロスコープ/収録器の別channelへ
取り込み、その立ち上がり差を複数回測ります。機器・閾値・波形・試行数を保存します。
手で押したつもりの時刻や画面の変化を精密な入力基準にしません。
音声loopbackはADC・入力バッファを含む往復時間なので、出力だけの遅延と区別します。
対応機材がない場合は総遅延を「未測定」とします。

## 根拠

- [Apple: IOHIDValue timestampの単位](https://developer.apple.com/documentation/iokit/1433294-iohidvaluecreatewithintegervalue)
- [Apple: Input Monitoringの確認と要求](https://developer.apple.com/videos/play/wwdc2019/701/)
- [Apple: ListenEvent](https://developer.apple.com/documentation/iokit/iohidrequesttype/kiohidrequesttypelistenevent)
- [Apple: mach_absolute_time](https://developer.apple.com/documentation/driverkit/mach_absolute_time)
- [Apple: privacy manifestのSystemBootTime利用理由](https://developer.apple.com/documentation/bundleresources/app-privacy-configuration/nsprivacyaccessedapitypes/nsprivacyaccessedapitype)
- 実際に確認した依存ソース: Cargo registry内の`cpal-0.18.1/src/host/coreaudio/macos/device.rs`、
  `gilrs-core-0.5.15/src/platform/macos/gamepad.rs`、`winit-0.30.13/src/platform_impl/macos/event.rs`です。

共通JSON・比較スクリプトは[Linux遅延検証](linux-latency.md)でも使用します。
`compare-latency.py`はstream id、epoch、input generationごとの最後の累積値を`segments`に保持し、
再接続や復帰を一つの分布に混ぜません。macOS固有の入力・音声の既定値は変わりません。
