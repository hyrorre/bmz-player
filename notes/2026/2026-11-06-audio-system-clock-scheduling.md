# システム時計に基づく音声予約と都度要求（#72）

実施日: 2026-11-06。基準HEAD: `fd94c20990f6546f48a874db2d67c2f31ef571f1`。
関連: [Issue #72](https://github.com/hyrorre/bmz-player/issues/72)、[音声設定](../../docs/settings.md#音声)、[gameplay runtime](../../docs/gameplay-runtime.md#音声予約と時計)。

## 背景と修正範囲

従来は判定を `Instant` で進める一方、BGM・自動キー音を曲開始時の出力frameと公称sample rateで予約していた。
40ppmの差なら固定対応の誤差は1分で2.4ms、3分で7.2ms、5分で12ms、10分で24msになる。
IssueのASIO・256 frames・360FPS/VSyncなしという報告と、このセッションのWASAPI排他Autoの報告は別の条件として扱った。

判定・ノーツ表示・BGAはシステム時計のままとし、本体共通 `audio.schedule_ahead` で2方式を選べるようにした。既定・旧設定読込時はON。

- ON: BGM・自動キー音を100ms先まで `Instant` のdeadlineで保持し、出力バッファを作る時点の時刻対応でframeへ変換する。フルオートも同じ経路。
- OFF: 期日到来後に即時再生を要求する。BGMもキー音も初期catch-upなしで先頭を保持する。
- 押鍵に応じたキー音は両方式で即時要求する。入力オフセットや旧frame原点から余分な待ちを作らない。音量、pan、同じ音の再発声、layer、バッチ・再送・取消は維持。
- 入力anchorは同じInstant観測から単調時刻と譜面時刻を求め、取得間のpreemptionを判定オフセットへ入れない。
- callback開始を制御側で確認してから曲の原点を確定する。WASAPI排他は開始前prefillを除く。最大2秒の失敗期限を設け、固定2秒sleepにはしない。
- pause/resume・速度変更で未発声deadlineを更新する。seek/retryの破棄、オフラインframe予約、リプレイの固定1ms採点と保存形式を維持。
- CPALとIOProcの出力予測が飛んでも、callbackの停止を伴わなければPCM欠落と決めつけて既存voiceをスキップしない。

CPALはAPI時計とInstantの明示的な対応、WASAPI排他はIAudioClockのposition/frequencyとQPC、IOProcはMach host timeを使う。
不正・欠落した予測にはcallback近傍へのフォールバックがある。予測のないPipeWireではcallback時刻を使う。
VSync、FPS上限、WASAPIのAutoバッファ選択は変更していない。過去の計測で同期設定を変えても時計差が残り、publication負荷を主因とする証拠は得られていないため。
波形自体を伸縮するASRCは今回の2方式とは別の設計であり、導入していない。

## 計測環境・方法

- Windows 11 Pro / build 26300、Core i9-12900K。
- WASAPI: スピーカー（RME Babyface Pro）、48kHz。排他Autoは実480 frames、共有は480/960 frames、低遅延共有は480 framesを観測。
- ASIO: ASIO4ALL v2、48kHz、要求・観測256 frames。背後のWDM endpointは未特定で、Issue報告者のASIO機器とは同一視しない。
- releaseの独立した計測器。設定・スコアDB・IR・リプレイを開かず、AudioEngineと実ストリームを直接使用。
- 337.013ms間隔で1サンプルの小振幅pulseを左右別にBGM（catch-upあり）と自動キー音（catch-upなし）として要求。OFFは両方catch-upなし。
- callbackでは事前確保したAtomic配列へPCM内の開始位置、API予測時刻、frameを記録。ファイル出力はcallback外。実入力・実判定の測定ではない。
- 本体テスト・ビルドが一部で並走した。CPU負荷を統制した性能ベンチマークでも、360FPSの実画面検証でもない。

生データ・解析・図は `.local/latency-investigation/fix-runs/`、計測器・解析は同階層の `fix-probe.rs` / `analyze-fix.py` に保存（Git管理外）。
最初の5分計測後、時刻変換の補助関数抽出・先の予約を走査しない最適化・outage判定を追加し、更新後のソースでも追加計測した。
計測後の最終レビューでは同時刻・同一WAVの要求順序保持を追加し、回帰テストとworkspace一式で検証した。この最終調整後の実機再計測は行っていない。
計測用hookは `bmz-audio/Cargo.toml`、`src/lib.rs`、`src/engine.rs`、`src/backend/cpal/callback.rs` に一時追加し、すべて撤去した。

## 結果

以下はPCM開始位置から計算した **API予測上の開始誤差**。物理的な発声誤差ではない。
符号は `予測開始時刻 − システム時計の予定時刻`。負の約0.01msはframeへの切り捨てを含む。

| 条件 | 時間 | 自動キー音平均 / 中央値 (ms) | p99 / 最大 (ms) | 誤差の傾き (ppm) |
|---|---:|---:|---:|---:|
| 排他ON #1 | 300秒 | −0.0104 / −0.0103 | −0.0002 / 0.0570 | −0.0018 |
| 排他ON #2 | 300秒 | −0.0105 / −0.0106 | −0.0003 / 0.0000 | −0.0012 |
| 排他ON #3 | 300秒 | −0.0104 / −0.0104 | −0.0004 / −0.00002 | +0.0030 |
| 排他OFF #1 | 60秒 | 15.723 / 15.686 | 20.909 / 20.960 | −5.0 |
| 排他OFF #2 | 60秒 | 15.653 / 15.569 | 20.735 / 21.074 | +1.8 |
| 排他OFF #3 | 60秒 | 17.091 / 16.288 | 43.200 / 52.030 | +102.2 |
| 共有ON | 60秒 | 0.0280 / −0.0090 | 0.3894 / 5.5502 | +3.57 |
| 低遅延共有ON | 60秒 | 0.0081 / −0.0094 | 0.5395 / 0.8750 | −0.40 |

排他ONは3回とも自動キー音889/889をPCMで観測した。API出力時刻に対応したframeの進行差は39.59 / 40.02 / 39.87ppmだったが、予約音の開始誤差には蓄積しなかった。
1秒ごとの単純なcurrent_frame観測には10msバッファの量子化があり、回帰は27.29 / 30.89 / 31.74ppmになった。精密なdevice周波数や物理遅延とは解釈しない。

排他ONのBGMは888/889、889/889、889/889。#1で1サンプルpulseがcatch-upにより消費され、欠落1件となった。
共有ONは173/177、低遅延共有ONは169/177。自動キー音は各177/177で、遅れた場合も先頭を保持した。
短いpulseの欠落を隠して全発声成功とはしていない。遅れたBGMのcatch-upとAPI timestamp精度は残る性質である。

排他OFF #3の最大52.030msは、要求生成の遅れ32.264ms、要求からPCM書込まで9.776ms、書込からAPI予測まで9.990msに分かれた。
52msのdevice遅延変化や102ppmの定常ドリフトとは判断しない。都度要求がworkerの起床遅延を受ける例である。
排他・共有の測定ではstream error、timeline catch-up、engine lock missは0。trace overflowも0。

ASIOは制限付き実行では初期化中にaccess violationとなり、通常ユーザー環境でASIO4ALLを明示すると測定できた。
管理者権限の導入やドライバ設定変更は行っていない。最初の通常環境60秒は最初の1秒までにstream errorが1件あり、その後増加しなかった。無異常計測とは分ける。

| ASIO4ALL条件（各60秒、256 frames） | 自動キー音平均 / 中央値 (ms) | p99 / 最大 (ms) | BGM観測数 / 予定数 | stream error |
|---|---:|---:|---:|---:|
| ON #1 | 0.0428 / −0.0099 | 1.4516 / 1.5371 | 165 / 177 | 1 |
| ON #2（追加計測） | 0.0235 / −0.0098 | 0.7318 / 1.3178 | 166 / 177 | 1 |
| ON #3（追加計測） | 0.0334 / −0.0089 | 0.9356 / 1.5018 | 164 / 177 | 0 |
| OFF（追加計測） | 10.1290 / 10.1379 | 13.2864 / 13.6039 | 177 / 177 | 0 |

自動キー音は全条件177/177。ON #2のerrorは開始0.306秒の `StreamInvalidated` で、最初の予約音0.5秒より前だった。
error種別を記録していない#1と、errorなしの#3を分けて保存した。reset/resync等の詳細と背後のdeviceは未特定であり、このerrorへの自動再初期化を今回追加したとは扱わない。
ASIOでもAPI時刻の粒度・揺れと遅延時catch-upにより1サンプルBGMが消費されるケースがある。API予測上の残差が1.5ms程度ある結果を、完全に安定した物理出力とは表現しない。
WASAPI排他10秒追加確認は両音29/29、stream error 0。これは5分計測の代用ではなく、時刻変換経路の接続確認。

全条件の平均・中央値・p95・p99・最大、絶対誤差、API予測lead、callback処理時間はローカル `summary.json` に保存した。
初期位相を除いた傾きと、要求遅延による外れ値を分離する。60秒の条件差は長期ドリフトの証明には使わない。

## 自動検証

`cargo fmt --check`、`cargo check --workspace --locked`、
`cargo clippy --workspace --all-targets --locked -- -D warnings`、
`cargo test --workspace --locked --no-fail-fast`、
`cargo build -p bmz-player --release --locked`（Windows、ASIOを含む既定feature）が成功。

音声146件、gameplay261件、player2885件成功。その他のworkspace crate・doc testも成功。
GPU/実機用ignoreや、外部アセットがない場合の既存テストの早期returnを手動確認済みとは数えない。
最初の制限付きテストはlocalhost通信・リンク作成で失敗し、待機したテストだけを停止して通常環境で全体を再実行した。
incremental cacheのhard link失敗はcopy fallbackの環境warning。コードのlint抑制は追加していない。

新規回帰は44.1/48/96kHz、±40/±200ppm、0/1/3/5/10分後のPCM位置、遅延BGMとキー音のattack差、
pause中の速度変更とresume、API時計reset・callback delivery jitter・出力遅延だけのjump、
両設定と25/100/300%のfull-auto、入力anchor、app設定の既定・旧形式・明示OFFを含む。

## 残る制約と手動確認

音の開始予約を直す変更であり、長いWAVの連続再生速度、入力/バッファ待ち、GPU/scanout/物理発声の遅延は消さない。
GUIDE SEは判定後の即時要求を維持するため、先読み音との固定差は残る。開始位相を除いた傾きで比較する。
macOS Core Audio / IOProcとLinux ALSA / PulseAudio / PipeWireの実機、報告者のASIO機器、実打鍵・聴感・ループバックは未検証。

既存データを保護する実画面確認は、プロセスごとに専用の保存先を設定する（パスは例）。

```powershell
$env:BMZ_DATA_DIR = 'C:/Users/hyrorre/private/bmz-player/.local/latency-investigation/manual-data'
$env:BMZ_CACHE_DIR = 'C:/Users/hyrorre/private/bmz-player/.local/latency-investigation/manual-cache'
$env:BMZ_LOGS_DIR = 'C:/Users/hyrorre/private/bmz-player/.local/latency-investigation/manual-logs'
$env:BMZ_RESOURCE_DIR = 'C:/Users/hyrorre/private/bmz-player/data'
& ./target/release/bmz-player.exe
```

1. 専用設定で360FPS/VSyncなし、報告機器のASIO/256 frames、GUIDE SEを選ぶ。実効バッファも診断で確認する。
2. 同じ長尺譜面をON/OFFでフルオート再生し、0/1/3/5/10分の音同士の差を記録する。各条件3回、冷起動・リトライ・連続曲を区別する。
3. 短いWAVを順次鳴らす譜面と、1本の長いWAVに周期markerが入る譜面を分ける。後者だけの傾きは今回の予約修正で消えるとは限らない。
4. Fast/Adaptive、同一バックエンドのバッファ変更を比較する。実出音を確かめる場合は独立したループバック録音を使う。
5. 手動入力も専用データで確認し、普段のoffsetやスコアを変更しない。環境変数はそのPowerShellを閉じれば残らない。

固定frame対応を修正し、都度要求を比較手段として提供した。Issue全体の聴感上の解決や、すべてのdeviceの正常動作は断定しない。
