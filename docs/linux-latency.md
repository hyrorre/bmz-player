# Linuxの入力・音声遅延と比較検証

LinuxのPipeWire出力、gilrsのイベント待機、任意のevdevキーボードを扱う。
既定入力Auto（Linuxではwinit）、既定音声Auto（PulseAudio、利用不可ならALSA）、
既定Fixed 256 framesは維持する。PipeWireや64 framesへの自動切り替えは行わない。
macOSの[共通診断](macos-latency.md)と同じヒストグラム・JSON・比較スクリプトを使用する。
実装時の結果と未確認事項は[検証記録](../notes/2026/2026-10-06-linux-latency.md)を参照。

## 対応範囲

| 配布・環境 | 音声 | キーボード | コントローラー |
| --- | --- | --- | --- |
| 通常のCargo既定ビルド | PulseAudio / ALSA | winit | gilrsイベント待機 |
| `--features pipewire,linux-evdev` / Linux tar | 上記 + ネイティブPipeWire | winit / 条件を満たすネイティブX11のevdev | 同上 |
| Flatpak | PipeWire / PulseAudio / ALSA（実デバイスアクセスはsandbox次第） | winit | 同上、見える・openできるデバイスに限る |
| Wayland / XWayland | 各ビルドに含まれる上記音声 | evdev要求時もwinit | 同上 |

evdevはLinux専用の任意featureで、Flatpakには含めない。Waylandのwinit接続とは別の
接続から既存ウィンドウのフォーカスを確認できるとは仮定しない。
音声・コントローラーの改善はevdevの利用条件から独立している。
判定窓、入力オフセットの意味、判定補正、リプレイ形式、キー名、デバイススロットは変更しない。

## evdev以外の経路とWaylandの入力待ち

Waylandの通常winit入力では、VSync中の描画先取得が同じウィンドウスレッドを占有すると、
後続の入力配送も待つ。BMZは実ウィンドウ接続がWayland、実効present modeがFifo/FifoRelaxedのとき、
描画成功フレームの`present()`直前にwinit 0.30.13の`pre_present_notify()`を呼ぶ。
これによりcompositorのframe callbackで次のRedrawRequestedを制御し、その間のイベント配送へ戻る。
取得失敗・再構成・描画スキップ・headless出力では通知しない。
[winitの公開API](https://docs.rs/winit/0.30.13/winit/window/struct.Window.html#method.pre_present_notify)に基づく。

FPS/VSync設定やGPUのframe latency設定を変更する機能ではない。Immediate/Mailbox、X11/XWayland、
macOS/Windowsではこの通知を追加しない。既存のゲーム入力は引き続きwinit経由で、IME・focus・
入力抑止・キー割り当て・判定時計も同じである。100msのウィンドウスレッド停止から独立する機能ではない。
物理押下から発音までの改善は未測定。ソフトウェア計測と採用判断は
[追加調査記録](../notes/2026/2026-10-06-linux-input-alternatives.md)を参照。

他の候補については次の制約があり、今回別の入力backendは追加していない。

| 候補 | 調査結果・採用条件 |
| --- | --- |
| XInput2/XI2専用接続 | X11でウィンドウスレッドと独立した取得は可能。ただしrawイベントのfocus/セッション管理、時計変換、入力源の排他が必要。今回の接続先はXWaylandで、ネイティブX11の実入力比較は未測定 |
| wl_keyboard / input-timestamps | 現ウィンドウと同じWayland接続のイベント。別接続で既存ウィンドウの入力を受ける代替にはならない。高分解能の時刻だけで到着や発音が速くなるわけではない |
| hidraw / HIDAPI | 対象機器固有のreport解析と権限が必要。対応コントローラーでgilrsとの差を実測してから検討する。USBのpoll周期を自動で短縮するAPIではない |
| libinput / libevdev | 下位のevdevアクセス権限を不要にはしない。通常アプリ向けの別の低遅延入力経路としては採用しない |
| InputCapture portal / libei | compositorがcaptureを開始する仕組みで、アプリが即時にcaptureを有効化するAPIではない。通常のfocusedゲーム入力の代替には採用しない |

一次資料: [XI2](https://xorg.freedesktop.org/archive/current/doc/inputproto/XI2proto.txt)、
[wl_keyboard](https://wayland.freedesktop.org/docs/html/apa.html#protocol-spec-wl_keyboard)、
[hidraw](https://docs.kernel.org/hid/hidraw.html)、
[InputCapture portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.InputCapture.html)。

### 人工通知によるイベントループ比較

`--latency-event-loop-probe`は11ms周期で別スレッドからwinitへ人工通知を送り、
送信直前のInstantから`user_event`処理までを共通ヒストグラムで計測する。
キーボード入力を注入せず、判定・キー音も発生させない。OS→winitの区間Aや物理入力遅延ではない。
`BMZ_LATENCY_JSON`の`kind=window_event_loop_probe`として出し、既存比較スクリプトで読める。
probe自体は環境変数なしでも有効で、`BMZ_LATENCY_DIAGNOSTICS=1`は入力/音声診断を併用する指定。

scene/focus/gameplay世代の変更、resume、1秒超の配送途絶はepochを分ける。
各epochの最初の2秒はwarm-upとして除外し、異なる世代・未来時刻を0として集計しない。
分位点は共通の対数bucket上端による近似。送信側は最大32件までに制限し、
`capacity_skips_process`（満杯のため送らなかった回数）、`producer_missed_ticks_process`
（送信スレッドの予定周期欠落）、`pending_process`（未処理）、`rejected_samples`を出す。
`*_process`は起動からの累積で、初期ロード中も含む。前後の差を取り、プレイ中の欠落と区別する。
配送途絶・満杯時の未取得値を分布へ捏造しない。probeのイベントは描画要求を起こさない。

同一releaseビルド、同一検証用データ・音声・VSync/FPS設定で逐次実行する。通常データは使わず、
後述の専用`BMZ_DATA_DIR`を用意する。例では自動演奏を使い、物理入力の実験とは分けている。

```bash
BMZ_DATA_DIR="$bmz_test_data" BMZ_RESOURCE_DIR="$PWD/data" BMZ_LATENCY_DIAGNOSTICS=1 \
  target/release/bmz-player --boot-play-sample --autoplay-on-start --smoke-exit-on-result \
  --latency-event-loop-probe --latency-legacy-wayland-present > before.log 2>&1
BMZ_DATA_DIR="$bmz_test_data" BMZ_RESOURCE_DIR="$PWD/data" BMZ_LATENCY_DIAGNOSTICS=1 \
  target/release/bmz-player --boot-play-sample --autoplay-on-start --smoke-exit-on-result \
  --latency-event-loop-probe > after.log 2>&1
python3 scripts/compare-latency.py before.log after.log
```

Flatpakでは同じ引数を`flatpak run`または検証buildのラッパーへ渡す。sandbox内の検証用
`BMZ_DATA_DIR`とログ出力先を区別する。buildディレクトリからの実行には、例えば次のように
配布manifestにあるソケット等を明示する（`flatpak build`はインストール済みアプリの起動とは異なる）。

```bash
flatpak build --runtime --readonly --nofilesystem=host \
  --socket=wayland --socket=fallback-x11 --socket=pulseaudio \
  --env=WAYLAND_DISPLAY="${WAYLAND_DISPLAY:?Waylandセッションで実行してください}" \
  --share=ipc --device=dri --device=input --filesystem=xdg-run/pipewire-0 \
  --env=BMZ_DATA_DIR=/tmp/bmz-window-probe-new --env=BMZ_LATENCY_DIAGNOSTICS=1 \
  .local/latency-flatpak/build /app/bin/bmz-player-flatpak \
  --boot-play-sample --autoplay-on-start --smoke-exit-on-result --latency-event-loop-probe
```

前後で同じconfig/profileを使う場合は、新規の検証ディレクトリ1つだけを
`--filesystem=/absolute/test-directory`で公開して`BMZ_DATA_DIR`に指定できる。
通常データやホスト全体を公開しない。通常描画の比較後、両方に既存`--latency-stall-test`を
追加して停滞を比較する。frame callbackが人工的な100ms停止も解消したとは扱わない。
CPU比較ではprobeが約91回/秒の追加起床を発生させることを考慮し、両側の診断条件を揃える。

2つの新規CLIはプロセス限定で、いずれもスコア/リプレイ/IR保存と背景IR同期を抑止する。
通常利用へ戻すには両フラグを外す。描画通知だけを従来方式に戻して比較するには
`--latency-legacy-wayland-present`を付ける（この場合も検証モードで保存無効）。
依存ライブラリの追加・更新、入力デバイス権限やFlatpak権限の追加は不要。

## ビルド

[README](../README.md)のRust / FFmpeg / ALSA / udev等に加え、PipeWireとSPAの開発ファイルが必要。
CPAL 0.18.1の利用機能にはlibpipewire 0.3.53以上が必要。ネイティブの例:

```bash
# ホスト管理者が必要に応じて実行する準備。BMZは実行しない。
sudo apt-get install libpipewire-0.3-dev libspa-0.2-dev clang libclang-dev pkg-config
pkg-config --atleast-version=0.3.53 libpipewire-0.3
pkg-config --modversion libpipewire-0.3 libspa-0.2
cargo build --release --locked -p bmz-player --no-default-features --features pulseaudio,pipewire,linux-evdev
```

Ubuntu 22.04標準のPipeWire開発パッケージはこの最低版を満たさない。
[Linux tarのビルド](linux-tar.md)は同じglibc 2.35基準を維持し、固定したPipeWire 1.4.9の
クライアントライブラリ・必要なmodule/SPA・専用client.confをパッケージ内へ同梱する。
ホストのサーバー・設定・サービスを置き換えない。Windows/macOSにはLinux専用依存を追加しない。
CPAL/gilrs/winit自体は更新せず、読み取り専用のCargoソースでもbindgenが生成物を出力できる
libspa/libspa-sys/pipewire-sys 0.10.1へパッチ更新している。

ネイティブの動的リンク先が欠けると、BMZ起動前にローダーが失敗する。
この状態はアプリ内のPulseAudioフォールバックでは復旧できない。配布tarでは必ず上位の
`./bmz-player` ランチャーを使い、`bin` / `lib` / `resources` を一緒に保持する。

## Flatpakのビルド・接続

最低Flatpakは1.15.6（`--device=input`対応）。runtime/SDKはFreedesktop 25.08。
SDKのPipeWire/SPA開発ファイル、runtimeのlibpipewire・protocol-native等を使用する。
通常のbundle作成は以下。既存インストールを入れ替える `--install` / `--smoke` は比較時には付けない。

```bash
scripts/package-flatpak.sh --out-dir .local/latency-flatpak
```

既存アプリをインストールし直さずに候補を確認する例（rootから実行）:

```bash
flatpak build --runtime --readonly --nofilesystem=host \
  --socket=pulseaudio --filesystem=xdg-run/pipewire-0 \
  --env=BMZ_LATENCY_DIAGNOSTICS=1 --env=BMZ_DATA_DIR=/tmp/bmz-latency-probe \
  .local/latency-flatpak/build /app/bin/bmz-player-flatpak \
  audio-probe pipewire 128 48000 8
```

これはビルドディレクトリからのruntime確認。インストール済みbundleの確認は別に記録する。
通常の起動は `flatpak run net.hyrorre.BMZPlayer`。
manifestは `--socket=pulseaudio` を維持し、ネイティブPipeWire用には実在する通常ソケットだけを
`--filesystem=xdg-run/pipewire-0` で公開する。`--socket=pipewire` という権限を仮定しない。
managerソケット、host filesystem、全デバイス、session/system busの全公開は追加しない。
ランチャーはPulse cookie互換処理を維持し、音声バックエンドやquantumを強制しない。

生ソケットへの接続許可だけでは安全なサーバー設定を保証しない。ホストでクライアントの
`pipewire.sec.flatpak=true`、`pipewire.access.effective=flatpak`、正しいapp_idと付与権限を確認する。
以下は**ホスト側**の読み取りコマンド。`CLIENT_ID` は対象BMZのClient object IDに置き換える。

```bash
pw-dump > .local/latency-pw-dump.json
pw-cli get-permissions CLIENT_ID
```

`pw-dump`自身から見える各objectのpermissionsは、BMZに付与された権限ではない。
実装時の環境では `get-permissions` のdefaultは `r-x--`、クライアント分類はflatpakだった。
オーディオグラフの参照・実行アクセスは増える。無制限のmanager扱いになる環境では使わず、
PulseAudioを選択し、必要なら起動単位で `flatpak run --nofilesystem=xdg-run/pipewire-0 ...` とする。
別名ソケットの公開やサーバー側アクセス制御の変更を自動では行わない。
[Flatpak権限仕様](https://docs.flatpak.org/en/latest/sandbox-permissions.html)、
[PipeWire access](https://pipewire.pages.freedesktop.org/pipewire/page_access.html)、
[WirePlumberの制御](https://pipewire.pages.freedesktop.org/wireplumber/daemon/configuration/access.html)も参照。

## 設定と復旧

設定 → 音声の「バックエンド」でPipeWire / PulseAudio / ALSAを選択し、出力デバイス、
サンプルレート、バッファモード（Auto / Fixed）を選び「適用」する。
Fixedの比較値は256、128、64 frames。元の既定256を残し、一度に一項目だけ変える。
適用は既存仕様どおり選曲画面で行い、演奏中の自動バッファ・レート変更は行わない。
バックエンドを変更した時点で以前のデバイス名はクリアする。改めてそのバックエンドの
デバイスを選択する。選択値、動作中のhost/device、エラーを確認してから再生する。
明示したPipeWireが失敗した場合、勝手に別バックエンドへ切り替えない。
既存の音声設定再適用処理が以前の出力を復元する場合もエラーを残す。

未ビルド・host接続/初期化失敗・指定/既定デバイス不在・stream構築失敗は別のエラーになる。
CPAL PipeWireのhost初期化APIは詳細な失敗理由を一部失うため、サーバー不在と初期化内部故障を
常に区別できるわけではない。共有ライブラリ不足は前節の起動前エラーである。
音が出ない場合はPulseAudioまたは以前のバックエンドへ戻し、デバイスを選び直して「適用」する。
小バッファで不安定なら256またはAutoへ戻す。ALSAのbusy/未対応フォーマットのために音声サーバーを止めない。

設定 → 入力 → デバイスの「キーボード入力方式」で `Linux evdev (X11)` を明示選択できる。
同画面の「実動作」と理由を確認し、デバイス一覧を更新して対象の安定パスを選ぶ。
戻す場合は「winitに戻す」、または方式をwinit/Autoにする。
「比較用: Linuxコントローラーを従来の1ms待機にする」を有効にすると
同じビルド・設定で待機方式だけを旧方式にできる。通常は無効（イベント待機）。
設定の保存先・自動保存・適用時期は[操作仕様](controls.md)に従う。

## evdevの条件と制約

必要条件は、ネイティブのローカルX11、本人のlogind sessionがType=x11かつActive、
LockedHint=false、独立接続で確認できるBMZのX11 input focus、選んだ全デバイスのread権限。
さらにウィンドウ側で25ms以内に更新されたフォーカス・UI抑止情報を要求する。
GetSessionByPIDでセッションを確認できない起動方法も利用不可。Wayland/XWayland/Flatpakはwinitへ戻す。
フォーカス保護で条件を満たさなくなった場合、evdevがopen済みでも**ゲーム入力を抑止**する。
低FPSや100ms描画停滞では期限切れになるため、完全な描画非依存やその間のキー音応答を保証しない。

対象には `/dev/input/by-id/*-event-kbd` / `by-path/*-event-kbd` 等の安定symlinkを使う。
event番号を個体IDとして保存しない。by-pathは接続ポートを識別し、差し替えで別個体になることがある。
同名機器を製品名だけで統合しない。仮想リマッパーを使う場合はその安定symlinkを選び、
物理側と変換後を両方選ばない。カスタムsymlinkも `/dev/input` 内だけを許可する。
一覧更新は明示操作、切断時は1秒ごとに選択デバイスだけを再openする。
全選択デバイスがopenできなければwinitへ戻し、EACCES、ノード不在、時計/初期化失敗を表示する。

デバイスがsandbox内に見えること、ホストACLでopenできること、安全にフォーカス確認できることは
別条件である。`--device=input`だけで全キーボードが読めるとは限らない。
root実行、EVIOCGRAB、inputグループ追加、/dev/input全体のchmod、広いudevルール、
独自の権限昇格・seat controllerは使用しない。
管理者が特定デバイスへのアクセスを別途設計する場合は、対象の識別、現在のACL/udev設定の保存、
セッション終了時の権限回収、取り消し方法を先に決める。BMZの導入手順として一律の権限付与は推奨しない。

UI文字入力・IMEはwinitのまま。ゲーム配送は共通mutexの下でwinit/evdevを一つに選び、
切替・フォーカス喪失・route世代変更・終了時に保持を解放する。
復帰時のカーネルsnapshotで既に押されていたキーは解放まで抑止する。
repeatはPressにせず、選択した複数キーボードの同じキーは最初のPress/最後のReleaseへ集約する。
ANSI/JIS、左右修飾、テンキー、Fキーはwinit公開scancode APIと既存PhysicalControl名へ変換する。
EV_KEYとXKBのコードを混同せず、独自の+8変換を加えない。未対応キーは診断件数と設定UIの通知に残す。
同時に読める複数デバイスのバッチはイベント時刻順にまとめ、同時刻は選択順、各fd内は元の順序を維持する。

read-only/nonblocking fdにEVIOCSCLOCKID(CLOCK_MONOTONIC)を設定する。
BMZのInstant時計とclock_gettimeの読み取りを挟んで原点を対応付け、壁時計を経由しない。
設定前・初期・未来・範囲外時刻は無効とし、正常に遅れて届いたイベントは現在時刻へ丸めない。
BOOTTIMEとの差でスリープを検出し、時計の不連続では保持を解放・再同期して測定区間を分ける。
SYN_DROPPEDから次のSYN_REPORTまでは履歴を捨て、現在状態を取得する。
不明な過去の押下を新規判定として再現しない。内部キューoverflowも別カウンターで検出し、
配送済み保持を解放して、不完全な履歴を捨て、キー解放まで抑止する。
内部キューの既存イベントとoverflowを起こしたイベントのReleaseも順に反映し、
既に離されたキーの次のPressは受け付ける。合成Releaseはgameplayが受け取るまで維持し、
連続overflowでも保持解放を失わない。
[カーネル仕様](https://docs.kernel.org/input/event-codes.html)を参照。

## gilrsの待機と時計

gilrs 0.10.10 / gilrs-core 0.5.15の公開 `next_event_blocking(Some(timeout))` をLinuxだけで使う。
到着で起床し、先頭1件と蓄積済みイベントを順に処理する。1件ごとのsleepは入れない。
AnalogGamepadProcessorの既存release期限と制御応答上限50msからtimeoutを決める。
gilrs内部のepollはthread::unparkでは解除できないため、この有限timeoutが必要。
即時Noneが連続する異常時には最大10msの回復待機を挟み、spinを避ける。
キーボードだけのworkerは250ms待機し、同一routeのフレーム更新では起こさない。
診断の `gilrs.poll_cycles` と `waits` / `empty_wakes` / `early_empty_wakes` で取得ループと待機を区別できる。
ボタン名、軸折返し、方向転換、deadzone、チャタリング、スロットは既存処理を再利用する。
切断時はそのデバイスの保持を解放し、旧route以前の滞留イベントを新しいプレイへ配送しない。

gilrsのLinuxイベントはカーネルtimevalをUNIX_EPOCHからのSystemTimeへ変換している。
この版はCLOCK_MONOTONICを指定しない。既存のSystemTime→BMZ時計変換を維持し、
診断AはSystemTimeの差として測定する。Instantとの進行差、未来、測定開始前の値は除外する。
evdevのMONOTONIC測定とは時計が異なる。いずれもスイッチ接点やUSB送信開始時刻ではない。

## 音声の要求と実測

使用版はCargo.lockのCPAL 0.18.1。次はその依存ソースで確認した挙動であり、一般の全CPAL版の保証ではない。

| 経路 | Fixed(n)の意味 | 出力時刻の限界 |
| --- | --- | --- |
| PipeWire | stream property `node.latency=n/rate`。BMZ設定から直接要求 | `pw_stream_get_time`のdelay/rate等を使うが、取得失敗時は1処理周期を合成。公開APIに有効性bitがなくBMZではpredictionを集計しない |
| PulseAudio | n×channels×sample bytesをminreq、2倍をtlength/maxlengthとしてadjust_latency付き要求 | サーバーtiming情報の遅延値を補間。古い情報やゼロになり得る。ゼロ/逆転/1秒以上は無効、残りも推定に過ぎない |
| ALSA | period nとbuffer 2nをnear要求。採用値は変わり得る | hardware/system monotonic/stream creationを使うfallbackがあり、status delayを加える。実際のtimestamp modeやperiod/bufferはCPAL公開APIから取得できない |

CPALソース入口: [PipeWire](https://docs.rs/crate/cpal/0.18.1/source/src/host/pipewire/)、
[PulseAudio](https://docs.rs/crate/cpal/0.18.1/source/src/host/pulseaudio/)、
[ALSA](https://docs.rs/crate/cpal/0.18.1/source/src/host/alsa/mod.rs)。
ALSA default PCMをhw直結と扱わない。明示hw選択は上級者向け比較で、busyやフォーマット不一致はエラーのまま扱う。

PipeWireのquantum/rateは処理周期であり、押下から発音までの総遅延ではない。
pw-topのdriver行のQUANT/RATEと、BMZ follower nodeのlatency要求を分ける。
アプリのlatency要求でも共有グラフの周期や他アプリへ影響し得る。
BMZは `clock.force-quantum` / `clock.force-rate`、PIPEWIRE_QUANTUM、ホスト設定やサービスを変更しない。
CPAL/PipeWire既存のスレッド優先度処理を使い、アプリ全体のSCHED_FIFO、root、CAP_SYS_NICEは不要。
RealtimeKitがあることだけでBMZ callbackがリアルタイム動作しているとは断定しない。

環境変数比較は任意。ネイティブPipeWireで外部から `PIPEWIRE_LATENCY=128/48000` を指定し、
BMZのFixed要求との優先順位を**その環境の実node.latencyとdriver周期で**確認する。
PulseAudioクライアントには同じ効果を仮定しない。使用中のCPAL/pulseaudio-rsが
PULSE_LATENCY_MSECを読むという根拠がないため、標準手順には使用しない。

## 共通JSONと測定区間

`BMZ_LATENCY_DIAGNOSTICS=1`を起動前に指定する。音声コールバックには診断I/O、文字列整形、
待機ロック、動的確保を追加せず、既存固定ヒストグラム/atomicを使う。無効時は分布測定を省く。
初回とstream再作成・再開/1秒超の中断・時計逆転・スリープ後には音声の2秒warm-upを設ける。
同一stream id/epoch内は累積値。input generation、evdev epoch/generationも別区間で比較する。

| JSON | 内容・単位 | 比較時の注意 |
| --- | --- | --- |
| environment | commit（変更済みはdirty）、feature、debug/release、native/Flatpak、実display handle | XDG_SESSION_TYPEはhintだけ。XWayland不明はnull |
| audio.stream | 要求/実host、device名/CPAL ID、rate、要求frames、対応範囲、CPAL設定、PW latency要求 | デバイスIDはバックエンドごとの名前空間。サーバーrate/実driver quantum/bufferは不明ならnull |
| audio.frames | callbackのdata.len()/実channels、frames | Fixed要求と実測を別に保持。分位点は近似 |
| audio.interval_ns / duration_ns | callback入口間隔 / 処理時間、Instant ns | 長い間隔だけでxrunと認定しない |
| audio.prediction_ns | APIのplayback−callback等、同一時計の差、ns | 物理測定ではない。invalid/unavailable件数を別記 |
| gilrs.os_to_receive_ns / evdev.os_to_receive_ns | A: OSイベント→取得処理 | 時計は各summaryに記載。winitのAは取得不可でnull |
| input_queue.enqueue_to_drain_ns | B: キュー投入→gameplayのdrain、Instant ns | 入力オフセット適用前の観測値。後段の判定処理全体ではない |
| play_audio_commands.all_commands_enqueue_to_apply_ns | Cに関連する全コマンドの投入→音声側適用 | 手動音と自動音/BGMが混ざるため、手動キー音Cとは呼ばない |
| manual_sound_enqueue_to_render_ns / matched_event_total_ns | 未取得、null | 対応する入力→音声sampleの追跡は未実装。Dや総遅延を捏造しない |
| stream_errors / lock_misses / queue_drops / timeline_catch_ups | 別々の累積カウンター | confirmed_xrunsは専用情報が取れなければnull。ゼロと区別 |

件数、p50/p95/p99、maxを使う。分位点は対数bucketの上端（実maxで上限）、maxは実測。
件数0のp値0は「未測定」であり0ms遅延ではない。独立したp99同士を足して総遅延p99を作らない。
音声予測は音全体の出力遅延推定で、特定の手動キー音の発音時刻を示さない。

## 比較用コマンド

設定・DBを使わない無音の接続/コールバック検証:

```bash
mkdir -p .local/performance/linux-latency
BMZ_LATENCY_DIAGNOSTICS=1 target/release/bmz-player audio-probe pulse 256 48000 8 \
  > .local/performance/linux-latency/pulse-256.log 2>&1
BMZ_LATENCY_DIAGNOSTICS=1 target/release/bmz-player audio-probe pipewire 128 48000 8 \
  > .local/performance/linux-latency/pipewire-128.log 2>&1
python3 scripts/compare-latency.py .local/performance/linux-latency/pulse-256.log \
  .local/performance/linux-latency/pipewire-128.log
```

HOSTはauto/pipewire/pulse/alsa、BUFFERはautoまたはframes、RATEはHz、SECONDSは3〜60。
auto hostが従来通りPulseになることも確認する。無音probeにはmixer負荷・入力・描画がない。
要求未ビルドやサーバー不在は非ゼロ終了とaudio_open_failureになる。設定を書き換えず、勝手に別hostへ出力しない。

ゲームでの検証には**新しい専用ディレクトリ**を使う。下の変数をその端末の絶対パスへ設定し、
初回起動で設定・音量等を調整する。既存profile/DBを上書き・共有しない。

```bash
bmz_test_data=$(mktemp -d /tmp/bmz-latency-data.XXXXXX)
BMZ_DATA_DIR="$bmz_test_data" BMZ_RESOURCE_DIR="$PWD/data" BMZ_LATENCY_DIAGNOSTICS=1 \
  target/release/bmz-player --boot-play-sample --latency-stall-test \
  > .local/performance/linux-latency/play-stall.log 2>&1
```

Flatpakのインストール済み版では、以下のように起動単位でsandbox内の新規ディレクトリへ向ける。
`BMZ_DATA_DIR`はsandbox内のパス。ホスト側の通常設定とは別物である。

```bash
flatpak run --env=BMZ_DATA_DIR=/tmp/bmz-latency-new-run --env=BMZ_LATENCY_DIAGNOSTICS=1 \
  net.hyrorre.BMZPlayer --boot-play-sample --latency-stall-test
```

`--latency-stall-test` は従来機能を再利用し、window threadだけを約2秒ごとに100ms停止する。
スコア/リプレイ保存、IR送信/背景同期を抑止する。通常起動では無効。
入力キュー以降へ注入する自動テストは、物理キーボードやUSBを含む測定ではない。

**ホスト側**の並行調査（PIDはBMZ本体のhost PID。Flatpakのnamespace PIDとは区別）:

```bash
python3 scripts/linux-latency-context.py PID --seconds 20 > .local/performance/linux-latency/server.log
pw-top
pw-dump > .local/performance/linux-latency/pw-dump.json
pactl --format=json info
pactl --format=json list sink-inputs
pactl --format=json list sinks
```

companionはBMZのhost PID/security PIDに一致するClientと関連Nodeを収集する。
observerツール版とサーバーCore情報を区別し、実driver quantumは推測しない。
Pulseのsink-inputのPID、またはCPALの正確な `cpal-pulseaudio-PID` 名が一致し、接続サーバー名がPipeWireを示すなら
「PulseAudioクライアント→pipewire-pulse」とする。通常PulseAudioなら従来Pulseサーバー。
PipeWireがインストールされているだけでは経路を断定しない。FlatpakのPulse PIDがnamespace値の場合は
自動対応付けできないことがあるので、client/stream/device情報を照合し、不明なら不明とする。
`buffer_latency_usec` / `sink_latency_usec` 等はサーバー側情報で、0が返っても物理遅延ゼロとはしない。
pipewire-pulseが `pulse.attr.minreq` / `pulse.attr.tlength` を公開する場合は**bytes**として記録し、
実フォーマットのchannelsとsample bytesでframesへ換算する。128 frames要求でもminreqが256 frames相当に
制限された実例がある。要求値だけでサーバーの採用値を推測しない。
取得できないminreq/tlengthやALSA period/bufferの実採用値は未取得とする。
companionのCPUは指定窓のプロセスCPU時間/経過時間（1コア=100%）で、描画FPSやpresent待ちではない。

## 実機A/B手順

ネイティブ/Flatpakを別記録にし、同じPC・OS/session、接続ハブ・入力機器・出力機器、
譜面、描画設定/FPS、rate、音量、入力オフセット、同じrelease buildで比較する。
有線入力・内蔵/有線音声を基準にし、手動キー音では自動キー音をOFFにする。
各条件をwarm-up後に複数回、通常譜面と高密度/多重発音譜面で試す。

1. 既存入力・既存音声をベースラインとして保存。
2. 入力を固定し、PulseAudioとネイティブPipeWireを比較。
3. 同じhostでAuto/256/128/64を比較。
4. 音声を固定し、gilrsの従来1ms待機チェックのON/OFFを比較。
5. 同じ音声でwinit/利用可能なevdevを比較。利用不可・安全上の抑止は遅延改善と数えない。
6. 通常描画/停滞試験を比較。
7. 同じ条件で診断環境変数なし/有効を比較し、CPUと安定性の変化を記録。

各回の件数/p95/p99/max、CPU、callback時間と間隔、確認済みxrun、stream error、drop、
catch-up、invalid/unavailable、focus抑止を記録する。表示FPSとsurface/present待ちは別項目にする。
短いtap、連打、長押し、同時押し、左右修飾、JIS/IME、スクラッチ連続回転/反転/停止、
抜き差し/複数台、フォーカス移動/最小化/ロック/スリープ、起動/選曲/preview/開始/retry/終了を確認する。
停滞中にもフォーカス移動とキー解放を試す。他音声アプリの再生併用時も確認する。
Fedora KDE/Kinoite、Ubuntu GNOMEのWayland、可能ならnative X11/XWayland、従来Pulseサーバーで別々に報告する。

64 framesの短時間起動だけで安定動作を認定しない。VM/コンテナ/WSLgや無音probeの結果を
実機の遅延保証に使わない。物理押下からアナログ出力までの総遅延は測定機材がなければ未測定。
ループバック測定の往復遅延を出力だけの遅延と混同しない。
