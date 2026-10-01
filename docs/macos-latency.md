# macOS 入力・音声遅延の検証

実装の計測結果と未検証項目は [2026-09-30の記録](../notes/2026/2026-09-30-macos-latency.md) を参照。
既定の入力Auto（macOSではwinit）と音声Fixed 256 framesは変更していない。
判定窓、入力オフセット、判定・ゲージ、リプレイ形式も変更していない。

## 入力の切り替え

設定 → 入力デバイス → キーボードバックエンドで `winit` / `macOS IOHID` を選ぶ。
保存先はapp configの `[input] backend = "Winit"` / `"MacOsHid"`。
ゲームパッドを無効にしてもIOHIDを利用できる。UIの文字入力とIMEはwinitに残る。
ネイティブ経路の動作中はwinitから同じゲーム入力を配送しない。

IOHIDは専用スレッドのIOHIDManager / CFRunLoopからSharedInputBackendへ直接配送する。
gameplayのwakerを起こし、ウィンドウ側のイベント消費や描画を待たない。
メニューの操作は従来のwinitイベントを使う。AppKitのapplication/delegateを作り直さない。
入力待ちはイベント駆動。50msの待機期限はフォーカス・権限状態の保守と終了確認用であり、
キーを50msごとにpollするものではない。

要求バックエンドと動作状態は別。IOHIDManagerが開いている表示は、実キーの受信確認や
遅延改善の証明ではない。無操作だけでは故障と判断しない。
macOS以外でMacOsHidを読んだ場合はwinitで動く。

### 権限・失敗・復帰

`macOS GameController` はmacOS 11以降のGCKeyboardを専用の直列dispatch queueで取得する。
入力監視の許可要求は行わない。キー別pressedChangedHandlerの引数から押下・解放を配送し、
判定用timestampはコールバック受信時の単調時計とする。プロフィールの最新イベント時刻を
個別キーの発生時刻として使わないため、OS取得→受信遅延は測定不可。
複数のキーボードはOSが統合する。物理キー変換とJIS Native:MacOS表現はIOHIDと共通。
macOS 11未満、キーボード未接続、またはプレイに割り当てたキーがAPIから得られない場合は
winitへ戻る。接続済み表示は実キーの受信・性能確認の証拠ではない。
フォーカス喪失・切断・route変更・停止時には保持を解放する。route変更時は世代を進め、
古い世代のコールバックを拒否し、キーを離すまで新たな押下を受け付けない。
終了時はハンドラーと通知を解除し、専用キュー上の処理終了後にRust callback storageを解放する。

macOS 10.15以降はIOHIDCheckAccess(ListenEvent)を先に呼ぶ。未許可・拒否時には
managerを開かずwinitへ戻る。起動時のIOHIDRequestAccess呼び出しは行わない。
設定の「入力監視の許可を要求」ボタンだけが許可要求を行う。
拒否後にOSがダイアログを再表示しない場合は、システム設定のプライバシーとセキュリティ →
入力監視から対象アプリを許可する。許可後は一度winitへ切り替えてからIOHIDを選び直す。
必要な再起動条件は実機ごとに記録する。今回、許可の付与・取り消し操作は行っていない。
10.13/10.14では存在しない権限APIを呼ばない。最低対応版はIntel 10.13 / Apple Silicon 11.0のまま。

初期化失敗、コールバックエラー、権限失効、対応できないusageを検出した場合は
保持キーを解放してmanagerを停止し、winitへ戻す。設定の要求値は保持する。
設定画面とログで理由を確認し、再試行は同じ切り替え操作で行う。
Accessibility権限、root、TCC DB編集、SIP変更、デバイス排他取得は使わない。

標準USB keyboard usageの文字キー、数字、修飾キー、ナビゲーション、F1–F24、テンキー、
JIS固有キーを物理キーとして変換する。JIS Ro/かな/英数はwinit 0.30.13の
`Native:MacOS:*` 表現を維持する。独自usage・consumer page・HID rolloverは
経路全体をwinitへ戻す対象。FnなどOS/機器が通常キーとして通知しないキーは実機確認が必要。
未知のキーを推測して別のキーに割り当てない。

物理デバイス別の保持を集約するため、二台で同じキーを保持しても片方の解放だけでは解除されない。
切断、フォーカス喪失、route世代変更で保持を解放する。
入力ごとに前面プロセスとWindowServerの可視ウィンドウを確認し、描画停止中にも背景入力を拒否する。
この照会のコストはネイティブ経路の実機負荷測定にも含める。
終了時はowner threadでunschedule → callback解除 → close → CFRelease → context解放の順に処理する。

### 時計

IOHIDValueGetTimeStampはMach絶対時刻のticks。mach_timebase_infoのnumer/denomで
u128乗算・整数除算してnsへ変換し、BMZのprocess-local Instantと対で採ったanchorへ対応させる。
SystemTimeを経由しない。過去イベントは表現可能なら元の時刻を保持し、同時刻イベントは
callback到着順でキューへ入れる。複数機器の時刻を並べ替えるための上書きは行わない。
ゼロ、未来、BMZ原点以前で表現不能、timebase不正は受信時刻へfallbackし、invalid件数を増やす。
そのサンプルをAの0nsとして集計しない。時計の対応が20ms超変化したら再基準化し、
そのイベントを無効扱いにする。mach_continuous_timeとmach_absolute_timeの進み方の差でも
スリープを検出して再基準化し、前epochの古いイベントを無効にする。
スリープをまたぐ正確な物理入力遅延は測定対象外。

## 音声設定

設定 → 音声のバッファモードでAutoまたはFixedを選ぶ。Fixedのプリセットには256/128/64がある。
サンプルレートも既存項目で指定する。対応しないレートはデバイス既定へ戻り、実際の値を記録する。
動作中のデバイス・レート・CPAL設定・対応範囲を設定画面で確認できる。
CPALのFixed値は実コールバックサイズの保証ではない。実値は診断のframesを見る。
範囲外要求は既存処理で範囲内へclampする。要求値とCPALへ渡した値の違いをログで確認する。
生成失敗はエラーログへ出し、設定再適用に失敗した場合は既存の前設定復旧処理を使う。
プレイ中に自動でバッファやレートを変更しない。元へ戻す場合は以前の値を選択して適用する。

Cargo.lockはCPAL 0.18.1 / gilrs 0.10.10（gilrs-core 0.5.15）/ winit 0.30.13。
CPALのCoreAudio実装が既に固定バッファを設定するため、BMZ側から重複したHAL設定をしない。
CPALはcallbackのmHostTimeからplayback予測を生成し、device buffer frames（取得失敗時は
callback frames）とdevice latency + safety offsetを加算する。後二者の問い合わせ失敗を
CPAL内部で0にするため、BMZから予測の完全性を検証できない点に注意する。
デバイス変更後の内部推定値更新、外部機器、DAC、スピーカーの物理出力は別途検証が必要。

## 診断の読み方

`BMZ_LATENCY_DIAGNOSTICS=1` で有効、未設定または0で無効。ログレベルはinfo以上が必要。
`BMZ_LATENCY_JSON ` に続くJSONが機械可読サマリー。音声コールバックからのファイルI/O、
文字列整形、待機ロック、診断用動的確保は追加していない。
固定容量のatomic histogramを読み手が集計する。キー名・入力文字列を記録しない。

| 出力 | 境界・単位 | 含まないもの |
|---|---|---|
| iohid.os_to_receive_ns | A: OS時刻→Rustネイティブcallback入口、ns。経路停止時に出力 | 物理接点→OS通知。winitでは測定不可 |
| input_queue.enqueue_to_drain_ns | Bの部分計測: SharedInputBackend投入直前→gameplay取り出し、ns | native callback内変換・foreground照会、取り出し後の判定 |
| play_audio_commands.all_commands_enqueue_to_apply_ns | Cの部分計測: コマンドキュー投入→適用直前、ns | 音のrender開始。BGM・自動キー音・制御コマンドも含むため手動キー音の値ではない |
| audio.frames | data.len()/実channel数、frames | 要求値との同一視 |
| audio.interval_ns | Instantで測ったcallback到着間隔、ns | underrunの確定診断 |
| audio.duration_ns | callback入口→render完了付近、ns | ハードウェア再生までの待ち |
| audio.prediction_ns | CPAL playback−callback、同一stream時計の差、ns | Dの対象音別測定、アナログ出力実測 |

**対象音を一意に追跡した手動キー音C/DとA〜D合計は未実装・未測定。**
この実装は既存の音声バッチ配送を保った部分計測であり、手動キー音・BGM・自動キー音の
合算値を「手動キー音遅延」と表示しない。物理スイッチからの総遅延も表示しない。
既存debugログのgameplay latencyには判定時刻からのevent ageが含まれるため、入力オフセット
適用前のA/Bと混ぜない。診断用受信時刻は判定用DeviceTimestampを書き換えない。

分布はcount / p50 / p95 / p99 / max。quantileは既存LatencyHistogram同様、対数bucketの上端
（実測maxで制限）。maxは実測値、count=0は未測定であり0nsの測定結果ではない。
live atomic snapshotは厳密に同時刻の値ではない。音声分布は最初の2秒を除外する。
stream.idごとに別集計。1秒超のcallback停止、CPAL時計逆行、macOSのMach時計から検出した復帰後はepochを増やし、
分布をリセットして2秒のwarm-upをやり直す。エラーカウンターはstream累積。
スリープ復帰前後は別の実行として測るのが比較上確実。
入力キューはplayごと、IOHIDは取得経路の開始から停止まで（時計再基準化でepochを分ける）、音声コマンドはengineごとの累積。
入力・コマンド側のウォームアップは、同じ短い予備プレイ後に本計測のplayを開始する。

負の/ゼロ/1秒以上のCPAL予測差はinvalid_predictionsへ数え、0nsへ変換しない。
長いcallback間隔、timeline catch-up、stream error、lock miss、queue dropは別の事象。
今回、OSのprocessor-overload property監視は追加していない。間隔だけから音切れ確定とは判断しない。

## A/B手順

普段のデータを保護する場合は、未使用の絶対パスをBMZ_DATA_DIRへ指定する。
resourceは同じcheckoutのdataを指定できる。既存DBを動作中にコピーしない。

```sh
cargo build -p bmz-player --release --locked
BMZ_DATA_DIR=/absolute/path/to/latency-test-data \
BMZ_RESOURCE_DIR=/absolute/path/to/bmz-player/data \
BMZ_LATENCY_DIAGNOSTICS=1 RUST_LOG=info \
target/release/bmz-player --boot-play-sample --smoke-exit-on-result \
  > /tmp/winit-256.log 2>&1
python3 scripts/compare-latency.py /tmp/winit-256.log /tmp/iohid-256.log
```

ローカルdebug `.app` の検証例（正式配布物の署名・notarization検証とは別）:

```sh
cargo build -p bmz-player --locked
bash scripts/package-macos-app.sh --debug --skip-build \
  --out-dir /tmp/bmz-latency-package --skip-rust-license-report
open -n -W '/tmp/bmz-latency-package/BMZ Player.app' \
  --env BMZ_DATA_DIR=/absolute/path/to/latency-test-data \
  --env BMZ_LATENCY_DIAGNOSTICS=1 --env RUST_LOG=info \
  --stderr /tmp/iohid-app.log \
  --args --boot-play-sample --latency-stall-test --smoke-exit-on-result
```

`open --env` は利用するmacOSの `open -h` で確認する。未対応の古いOSでは通常の.app起動で
設定画面から操作し、データパスの分離は [開発手順](development.md) に従う。
権限チェックが通っても `0xe00002c5` の場合はIOKitが排他アクセスエラーを返している。
権限の問題と区別し、キーボードを排他利用する常駐ソフト/ドライバをユーザー自身で確認する。

これは手動プレイの例。自動演奏で音声稼働だけ調べるときは `--autoplay-on-start` を追加し、
結果を手動入力計測と分ける。比較scriptは各ログの最後の累積サマリーをJSONで返す。
stream再生成やepoch変更を含むログは区間を分けて比較する。

1. Mac型番/CPU/OS、接続ハブ、キーボード・コントローラー、出力機器、実サンプルレートを記録する。
2. 同じ譜面・seed・描画設定・FPS・入力オフセット・音量を使う。手動キー音では自動キー音をOFF。
3. 最初に入力だけwinit→IOHIDへ変更する。音声は256等の同一値を固定する。
4. 次に入力を固定し、Auto→256→128→64を一つずつ比較する。48kHz対応時は48kHzに固定。
5. 診断0/1を比較し、通常起動と `--latency-stall-test` を別に記録する。
6. warm-up後に各条件を複数回、通常譜面と高密度・多重発音譜面で測る。
   10分以上の連続動作も確認する。CPUは同じ測定窓のprocess CPU時間/実時間、音声処理時間は
   callback分布、描画時間は既存frame診断として分離する。ビルドや他の重い処理を同時実行しない。

`--latency-stall-test` はウィンドウ側だけを約2秒ごとに100ms停止する。既定OFF。
画面バナーとタイトルにスコア・IR保存無効を表示し、通常/コース結果保存、IRジョブ生成、
背景IR同期を抑止する。終了してフラグなしで起動すれば元に戻る。
この試験ではバナーを描くegui処理も有効になるため、CPU比較は同じ試験条件同士で行う。
停滞中の短い押下/解放とフォーカス移動を実キーで試す。
既存gameplay_runtimeの人工イベント試験はSharedInputBackendから後段の独立性の検証であり、
IOHIDの取得遅延測定ではない。

### 実機チェックリスト

- CLI起動と配布.app起動を別々に試す。許可対象・署名・bundle idを記録する。
- 未許可、拒否、許可直後、アプリ再起動後、権限取消し後。拒否時にも起動・winit操作できること。
- 短いタップ、長押し、連打、同時押し、左右修飾キー、JIS Ro/かな/英数。
- 二台の同一キー保持、片方の解放・切断、接続追加、バックエンド切り替え、リトライ。
- 停滞中の他アプリへのフォーカス移動、最小化、解放、復帰後の押しっぱなし。
- スリープ復帰後は別区間。検索欄・設定欄での文字入力とIME、オーバーレイの入力抑止。
- 起動、選曲、プレビュー、プレイ開始、リトライ、終了時の音声。BMSコントローラーのscratchとrelease timeout。
- 内蔵/有線出力を基準に、外部機器は別結果として残す。利用できないIntel/Apple Siliconは未検証と明記。

gilrsのnext_event_blockingはmacOSで内部channelのrecv_timeoutを使うが、公開APIに
設定変更・終了から待機解除する機構がない。scratch期限やroute変更を安全に統合する範囲を超えるため、
コントローラーは従来の1ms待機を維持する。Linuxは変更しない。

## 物理測定

同じ入力トリガーの電気信号と実際のアナログ音声出力を、同じオシロスコープ/収録器の別channelへ
取り込み、その立ち上がり差を複数回測る。機器・閾値・波形・試行数を保存する。
手で押したつもりの時刻や画面の変化を精密な入力基準にしない。
音声loopbackはADC・入力バッファを含む往復時間なので、出力だけの遅延と区別する。
対応機材がない場合は総遅延を「未測定」とする。

## 根拠

- [Apple: IOHIDValue timestampの単位](https://developer.apple.com/documentation/iokit/1433294-iohidvaluecreatewithintegervalue)
- [Apple: Input Monitoringの確認と要求](https://developer.apple.com/videos/play/wwdc2019/701/)
- [Apple: ListenEvent](https://developer.apple.com/documentation/iokit/iohidrequesttype/kiohidrequesttypelistenevent)
- [Apple: mach_absolute_time](https://developer.apple.com/documentation/driverkit/mach_absolute_time)
- [Apple: privacy manifestのSystemBootTime利用理由](https://developer.apple.com/documentation/bundleresources/app-privacy-configuration/nsprivacyaccessedapitypes/nsprivacyaccessedapitype)
- 実際に確認した依存ソース: Cargo registry内の `cpal-0.18.1/src/host/coreaudio/macos/device.rs`、
  `gilrs-core-0.5.15/src/platform/macos/gamepad.rs`、`winit-0.30.13/src/platform_impl/macos/event.rs`。
