# Linux遅延診断・PipeWire・入力取得の実装と検証

実施日: 2026-10-06 JST。基準HEAD: `225d30e96109984960677d9f42092592f3ccadc4`。
作業ブランチ: `feat/linux-latency`。開始時から変更済みだったRmz-skin/mz-selectのgitlinkは保持した。
現在の仕様・ビルド・設定・復旧・実機確認手順は [Linux遅延](../../docs/linux-latency.md) を参照。

## 実装と判断

- macOSで導入済みの固定ヒストグラム、JSON、入力配送、描画停滞試験を拡張した。
  要求/実host・device ID・実callback frames・不明値・測定世代を区別し、比較スクリプトを更新。
  設定/DB/ウィンドウを使わない `audio-probe` と、読み取り専用のホスト調査スクリプトを追加。
- CPALのPipeWire featureを配布に組み込み、明示選択・初期化エラー・実動作を表示。
  AutoはPulseAudio→ALSA、音声バッファはFixed 256、Linuxの入力Autoはwinitのまま。
  CPAL feature追加だけではAutoがPipeWire優先へ変わるため、従来順をBMZ側で明示した。
- Linux gilrsだけ公開blocking APIへ変更。最大50msとスクラッチrelease期限で起床し、
  即時Noneループは有限backoff。既存方式の比較設定を残した。
- evdevは任意feature。明示選択した安定symlink、read-only/nonblocking、MONOTONIC対応、
  SYN_DROPPED/内部overflowの解放と再同期、複数キーボード集約・時刻順mergeを実装。
  winitと配送を直列化する。利用条件はローカルX11、本人のactive/unlocked logind session、
  X11 focusと25ms以内のwindow側情報。Wayland/XWayland/Flatpakではwinitへ戻す。
  100ms停滞中のevdev抑止は安全上の制限であり、入力遅延改善とは扱わない。
- Flatpakに通常のPipeWireソケットだけを追加公開。tarはglibc基準を維持し、
  PipeWire 1.4.9クライアントと限定module/SPA、専用client.confを同梱する構成に変更。
  ホストの設定・ACL・サービス・インストール済みアプリを変更していない。

実際の依存ソースでCPAL 0.18.1、gilrs 0.10.10/core 0.5.15、winit 0.30.13を確認した。
これら本体の版は維持。libspa/libspa-sys/pipewire-sysを0.10.1へパッチ更新した。
0.10.0のbindgen補助生成がread-only registry内へ書き込む問題を避けるためである。
Linux限定evdev 0.13.2、x11rb/x11rb-protocol 0.13.2、zbus 5.16とcoreのlibcを追加。
evdev由来のbitvec/funty/radium/tap/wyz以外の無関係なlock更新は行っていない。

CPAL PipeWireのFixedはnode.latency要求で、出力時刻APIは失敗時の合成値を識別できない。
そのpredictionは未取得扱いにする。PulseAudioのFixedはminreq=n、tlength/maxlength=2n相当の
byte要求で、サーバーが変更できる。ALSAはperiod n、buffer 2nのnear要求であり、defaultの
最終plugin経路は不明とする。gilrsのSystemTimeをMONOTONICへ読み替えていない。

## 検証環境と成果物

Ubuntu 26.04.1、kernel 7.0.0-34-generic、Waylandウィンドウ、48kHz、2ch、
出力は内蔵Digital Stereo (IEC958)。PipeWireサーバー1.6.2、PulseAudio互換サーバー名は
`PulseAudio (on PipeWire 1.6.2)`。従来PulseAudioサーバーは未検証。
ネイティブはdebug、Flatpakはrelease（Freedesktop 25.08、PipeWireクライアント1.4.9、
Flatpak 1.16.6）。ネイティブとFlatpakのCPU/処理時間を直接比較しない。
音声試験を実行したホスト側の `systemd-detect-virt` / `--vm` はともにnoneだった。
ツールの通常sandbox内ではcontainer-otherになるため、sandbox内の判定とホストを区別した。

開発パッケージ・pactlは一時ディレクトリへ展開しただけで、apt installは実行していない。
ホストのデスクトップ/音声ソケット接続を許可したローカル試験であり、入力権限を拡大していない。
Flatpakは隔離したsource/build/stateディレクトリでビルドし、`flatpak build --runtime --readonly`
から実行した。インストール済みbundleの入れ替え・検証とは区別する。

| 成果物 | SHA256 |
| --- | --- |
| 正確な基準HEADのdebug実行ファイル | `a4f28b3eb1b7b463d8497d68f91458222b99f2e66324a5ea5f1fe8fd3292c4ca` |
| 下記baseline/matrix測定時の中間debug候補 | `58fcda5fedac9609469ed741b6fab20e86793e905f3e08ac369e342be01fa534` |
| 最後の実行測定に用いたdebug候補 | `655594b36c5719db3dfbc74afa7fe0ab6a594a68e3b7a25c36cdcde62074ada1` |
| 最終Flatpak release本体 | `3ee7cb80e0fdf1ebd9bcac3b07e8ee097b042ef8abaa58c728cfba2bdf29e6b0` |

中間候補の一部JSONは基準hashだけを記録している。未コミット候補であることは上のartifact hashで区別する。
最終実装ではdirty表示とsource archiveのBUILD-COMMIT/overrideに対応した。
実行測定後、evdev権限喪失の復帰テストとfatal poll失敗時の状態表示を補完した。
成功時の入力・音声経路は変えず、自動テストを再実行した。上のhashは測定成果物の識別用である。
生ログは `.local/performance/linux-latency-2026-10-06/` にローカル保存し、Gitには含めない。

## 変更前後の同条件smoke

正確な基準HEADと中間候補を、別々の新規config/profile/DBで逐次起動した。
同梱sample、自動演奏、音声Auto→PulseAudio、Fixed 256、48kHz、1280×720/VSync、
100ms/2秒の描画停滞、result到達で終了。各1回だけで、物理入力は0件。

| 項目 | 基準HEAD | 中間候補 |
| --- | ---: | ---: |
| warm-up後callback件数 | 2008 | 2009 |
| callback frames p50 / p95 / p99 / max | 287 / 575 / 575 / 768 | 287 / 512 / 512 / 512 |
| callback処理p99 / max (µs) | 61.439 / 84.032 | 81.919 / 82.804 |
| callback間隔p99 / max (ms) | 9.437 / 14.597 | 8.871 / 8.871 |
| API出力推定p99 (ms) | 18.606 | 18.614 |
| stream error / command drop | 0 / 0 | 0 / 0 |
| timeline catch-up | 1 | 0 |

p値は対数bucket上端を実maxで制限した近似。287や575は実際の固定callbackサイズを意味しない。
条件内のばらつきとサンプル数が不足しており、これを遅延改善の証拠にはしない。
自動演奏コマンドと手動キー音を混同せず、Bは入力0件、手動C/D・物理総遅延は未測定。

## 音声backend/bufferの逐次比較

各条件8秒×2回、最初の2秒を分布から除外。32回のprobeは相互に重複していない。
無音でmixer・描画・入力負荷なし。下表は1回目、件数だけ1回目/2回目を併記。
間隔はms、処理はµs。分位点の近似性は前節と同じ。

| 実行 / 経路 | 要求frames | 件数 | 実frames p50/p95/p99/max | 間隔 p50/p95/p99/max | 処理 p50/p95/p99/max |
| --- | --- | --- | --- | --- | --- |
| native Pulse | Auto | 142/141 | 2048/2048/2048/2048 | 46.137/50.332/52.943/52.943 | 163.839/212.991/245.759/279.077 |
| native Pulse | 256 | 954/951 | 287/512/512/512 | 6.291/8.849/8.849/8.849 | 18.431/40.959/45.055/59.736 |
| native Pulse | 128 | 946/956 | 287/512/512/512 | 6.291/8.958/8.958/8.958 | 18.431/40.959/49.151/78.869 |
| native Pulse | 64 | 962/978 | 287/512/512/512 | 5.767/9.129/9.129/9.129 | 18.431/40.959/45.055/69.518 |
| native PipeWire | Auto | 281/282 | 1024/1024/1024/1024 | 21.584/21.584/21.584/21.584 | 327.679/458.751/524.287/553.563 |
| native PipeWire | 256 | 1126/1126 | 256/256/256/256 | 5.604/5.604/5.604/5.604 | 90.111/147.455/245.759/270.533 |
| native PipeWire | 128 | 2252/2253 | 128/128/128/128 | 2.884/2.884/2.884/2.907 | 49.151/81.919/147.455/215.418 |
| native PipeWire | 64 | 4505/4504 | 64/64/64/64 | 1.442/1.442/1.442/1.461 | 26.623/49.151/57.343/176.409 |
| Flatpak Pulse | Auto | 141/141 | 2048/2048/2048/2048 | 43.095/43.095/43.095/43.095 | 61.439/90.111/108.362/108.362 |
| Flatpak Pulse | 256 | 1125/1127 | 256/256/256/256 | 5.767/5.767/5.767/5.797 | 8.191/15.359/18.431/48.780 |
| Flatpak Pulse | 128 | 1126/1126 | 256/256/256/256 | 5.767/5.767/5.767/5.911 | 7.679/15.359/18.431/57.819 |
| Flatpak Pulse | 64 | 1127/1127 | 256/256/256/256 | 5.694/5.694/5.694/5.694 | 8.191/15.359/22.527/106.756 |
| Flatpak PipeWire | Auto | 282/282 | 1024/1024/1024/1024 | 21.587/21.587/21.587/21.587 | 36.863/49.151/147.455/162.170 |
| Flatpak PipeWire | 256 | 1126/1125 | 256/256/256/256 | 5.537/5.537/5.537/5.537 | 11.263/18.431/18.431/71.466 |
| Flatpak PipeWire | 128 | 2250/2251 | 128/128/128/128 | 2.884/2.884/2.884/3.414 | 5.631/12.287/15.359/128.624 |
| Flatpak PipeWire | 64 | 4501/4503 | 64/64/64/64 | 1.442/1.442/1.442/1.463 | 3.327/4.607/7.167/16.511 |

全32回でstream error・command drop・lock missは0。Pulse 256の1回目はnative/Flatpakとも
timeline catch-upが1、他は0。確認済みxrunはAPIがないため**不明**。長いcallback間隔と同一視しない。
PipeWireのAPI出力推定は全件unavailableで、0msとしていない。
要求に対応するcallback処理単位の変化は確認したが、音の物理出力遅延を測定したものではない。
64 framesの長時間・高密度譜面安定性は未認定。

別途pactlで128 frames要求のnative Pulse streamを調べた。
実フォーマットs32le/2ch/48kHz、`pulse.attr.minreq=2048`、tlength/maxlengthも2048 bytes、
node.latency=256/48000だった。要求minreq 1024 bytesが256 frames相当へ制限されている。
buffer/sink latencyは0を返したため推定値として採用しない。

最終候補の別のPipeWire 128試験中に、ホストの `pw-top -b -n 3` を観測した。
動作中driver（内蔵IEC958）のQUANT=128、RATE=48000、BMZ followerも128、両方のERR=0。
これは観測した短い区間のグラフ周期とカウンターであり、32回全体のxrun測定ではない。

## 最終候補の負荷・入力待機・フォールバック

無音PipeWire 128/48kHzを9秒ずつ逐次起動し、起動3秒後から4秒間のprocess CPUを測定した。
BMZの他試験やビルドと重ならない条件。診断OFF/ONそれぞれ2回、1コア=100%。

| 実行 | 診断OFF (%, 2回) | 診断ON (%, 2回) |
| --- | --- | --- |
| native debug | 3.50 / 3.50 | 3.75 / 4.00 |
| Flatpak release | 1.75 / 1.00 | 1.25 / 2.00 |

短い窓とCPU tick分解能、通常デスクトップ負荷によるばらつきがある。
これを診断の正確な負荷率や性能改善と断定しない。初期の別CPU試験は他probeと重複したため除外した。
再測定ではstream error/queue dropなし。診断OFFは分布件数0となり測定処理が省略される。

同じ最終native候補・同梱sample・音声Auto/256・描画停滞で、物理コントローラー入力なしの
取得ループを比較した。blockingはpoll_cycles=351、waits=351、即時空復帰0。
legacyはpoll_cycles=14713（設定適用前の起動中はblocking waits=35を含む）。
無入力時の高頻度起床を減らせたことは確認したが、物理入力Aのp95/p99や押下応答は未測定。

WaylandでLinuxEvdevを要求した試験もresultまで完走し、実route=winit、native=false、
evdev_status=2（対応外window環境）だった。機能を追加したことと、当環境で利用可能なことを分ける。
3回のsample試験すべてで入力queue drop=0、score_history/score_best/course_scoresは0件のままだった。
手動キー音・フォーカス切替の実入力は行っていない。最後にnative/FlatpakのPulse/PipeWireを各4秒再確認した。

## 配布と接続の確認

- native/FlatpakのPipeWire、PulseAudioに実callbackが到着することを確認。
  存在しないサーバーを起動単位の環境変数で指定した負例は非ゼロ終了し、別hostへ出力しなかった。
  ALSA defaultでもcallback到着を確認したが、plugin/実hardware経路やperiod/bufferは未取得。
- FlatpakのPipeWire Clientは`pipewire.access.effective=flatpak`、`pipewire.sec.flatpak=true`、
  app_id=`net.hyrorre.BMZPlayer`。host security PIDとnamespace PID=2を区別できた。
  `pw-cli get-permissions`のdefaultは`r-x--`。managerソケットや全bus公開は追加していない。
- 固定SHA256のPipeWire 1.4.9ソースからprivate client library/module/SPAをビルドできた。
  一時ディレクトリの同ライブラリ・module/SPA・配布client.confで、既存ホストへ接続して128 framesの
  実callbackが到着することも確認した（4秒、stream error/drop 0）。
  Docker/Podman不在のためUbuntu 22.04のtar全体、全依存収集、offline再ビルドは未実行。
  その検証はCIのcontainer内へ追加した。ホスト上で音声サーバーを起動/停止する検証は実行していない。
- Flatpak SDK/runtimeでreleaseビルドとruntime起動を確認。ローカルLuxez-Flat素材がないため
  配布bundle一式の生成・インストールは未実行。既存Flatpak起動ラッパーは変更していない。

## 自動検証

次のコマンドを実行した。PipeWire開発ファイルは `/tmp` へ展開したpkg-config検索先を指定。

```bash
cargo fmt --check
cargo check --workspace --locked --features bmz-player/pipewire,bmz-player/linux-evdev
cargo clippy --workspace --all-targets --locked --features bmz-player/pipewire,bmz-player/linux-evdev -- -D warnings
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked --features bmz-player/pipewire,bmz-player/linux-evdev --no-fail-fast
cargo test -p bmz-player --locked --features pipewire,linux-evdev --lib -- --skip skin_loader::tests
cargo test -p bmz-player --locked --lib -- --skip skin_loader::tests
python3 -m unittest discover -s scripts -p 'test_*latency*.py'
python3 -m unittest discover -s installer/flatpak -p 'test_*.py'
python3 -m unittest discover -s installer/linux-tar -p 'test_*.py'
```

fmt/check、両feature構成のall-targets Clippyは成功。最後のplayer検証は2027成功/2ignore、
PipeWire/evdevなしの既定feature構成は2016成功/2ignore。
Pythonは診断5、Flatpak2、tar15成功。入力の時計/変換/再同期/世代/二重配送防止、
gilrs期限/早期復帰、バッファ実測、不正時刻、設定後方互換、6言語のキー整合等を含む。
既存の同一イベント列・判定再現、描画/consumer停滞とPCM/判定独立性のテストも成功。

workspace全体は成功していない。playerの外部スキン関連23件、skinの4件が失敗した。
Luxez-Flat欠落、Rmz-skin/mz-select等の現チェックアウトと期待素材の不一致を確認しており、
今回の対象外のスキンやユーザーのgitlink変更は修正していない。全体実行ではplayer 2195成功、
skin 246成功、audio 117/core 13/gameplay 235/render 679等は成功。
最初のsandbox内実行ではloopbackネットワークの制限でも失敗したため、許可された再実行の結果を採用した。
外部素材テストのskip/ignoreを互換確認済みと数えない。

Windows/macOS向けCargo依存treeでLinux限定evdevが入らないことと、既存設定のテストは確認。
各OSのコンパイル・音声・実入力は未実行。macOS/Windowsの待機backend仕様は変更していない。

## 未測定・次の実機確認

物理押下→アナログ出力、手動キー音の対応C/D、入力A/Bの物理デバイス比較、確認済みxrunは未測定。
native X11でのevdev実デバイス、EACCES実機復旧、JIS/IME、複数台保持、抜き差し、
フォーカス移動/最小化/画面ロック/スリープと100ms停滞の組み合わせを優先する。
コントローラーの実回転/方向転換/停止、他音声アプリ併用、通常/高密度譜面の長時間64/128 framesも必要。
Fedora/Kinoite、X11/XWayland、従来PulseAudio、installed Flatpak bundle、macOS/Windowsは未検証。
VM/コンテナ/WSLgの測定を実機保証へ読み替えない。

## 2026-10-06追記: PipeWireの再確認とdefault featureへの復帰

ユーザーから以前のクラッシュ懸念について確認を依頼され、`fedf33a2`を基準に再検証した。
履歴上、`47c1e7c7`でPipeWireをopt-inにした直接の理由は、libspa 0.10.0の
`SPA_ID_INVALID`参照が解決できないビルド失敗だった。`75b8f6e5`に記録された終了時の
native abortはPulseAudio経路であり、その終了時回避処理は維持している。
過去のPipeWireクラッシュと同一条件で比較したものではなく、原因修正の断定はしない。

再確認時はUbuntu 26.04.1、Plasma / Wayland、PipeWire 1.6.2、
既定出力Babyface Pro (Class Compliant Mode) Analog Stereo、48kHzを使用した。
CPAL 0.18.1、libspa / libspa-sys / pipewire-sys 0.10.1のまま、
`cargo build -p bmz-player --release --locked --offline --features pipewire`が成功した。
開発パッケージはインストール済みruntimeと一致する1.6.2-1ubuntu1.2を専用ディレクトリへ
展開し、`PKG_CONFIG_PATH`で参照した。システムへのapt installは行っていない。

| 検証 | 結果 |
| --- | --- |
| ネイティブPipeWire無音probe | buffer Auto: 8秒、256: 30秒、128: 15秒、64: 10秒。全て実callback到着・正常終了 |
| 音声Autoの無音probe | 256 framesで5秒。PipeWire feature有効でもPulseAudioを選択し正常終了 |
| サンプル曲の自動演奏 | PipeWire 256 / 128 framesで各1回、分離したconfig / profile / DBから結果画面まで進み正常終了 |
| エラー | 全7回でstream error / command dropは0。自動演奏のWARN / ERRORも0 |
| 接続経路 | `pw-dump`でネイティブPipeWireの出力streamからBabyface Proへのactive linkを確認 |

長時間・高密度譜面、実行中のバックエンド切替、デバイス抜き差し、物理発音遅延は未検証。
この短い試験で全環境の安定性や音切れの不在を保証しない。
生ログと集計は`.local/performance/pipewire-recheck-2026-10-06/`に保存した。

この結果を確認したユーザーの指示により、`bmz-player`のdefault featureへ`pipewire`を追加した。
依存の版と音声Auto（PulseAudio→ALSA）、Fixed 256の既定値は変更しない。
Cargo既定ビルドもPipeWire / SPA開発ファイルを必要とするため、READMEと
[Linuxビルド手順](../../docs/linux-latency.md#ビルド)を更新した。
Ubuntuでは`libpipewire-0.3-dev` / `libspa-0.2-dev`、Fedoraでは`pipewire-devel`が必要。
Linuxで除外する場合は`--no-default-features --features pulseaudio`を指定する。
tar / Flatpakは既にPipeWireを明示してビルドしており、同梱物の変更はない。

default変更後の検証（開発ファイルは上記の展開先を利用）:

- `cargo fmt --check`、`cargo check --workspace --locked --offline`、
  `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`は成功。
- `cargo test --workspace --locked --offline --no-fail-fast`は3753成功・6失敗・26 ignored。
  テスト内部の子プロセスで再実行された1成功は全体件数から除外した。
  失敗は既存のLuxez-Flat未配置による6件で、直前のplayerテストと失敗名が全て一致した。
  PipeWire featureとバックエンド既定値の既存テストも成功した。
- Linuxの`--no-default-features --features pulseaudio`でのplayer checkも成功。
- `cargo build -p bmz-player --release --locked --offline`は追加feature指定なしで成功。
  その実行ファイルで音声Autoの無音probe（256 / 48kHz / 4秒）がPulseAudioを選び、
  PipeWireを明示したサンプル自動演奏（256 / 48kHz）が結果画面まで進み正常終了した。
  実行時のfeature一覧に`PIPEWIRE`を確認。両方でstream error / command dropは0、WARN / ERRORも0。
- Windows x86_64 / macOS arm64の通常依存tree、およびLinuxの上記除外構成には
  PipeWire / SPAのcrateが含まれないことを確認。Windows/macOSでのコンパイル・実機確認は未実施。

default変更後の生ログは`.local/performance/pipewire-default-2026-10-06/`に保存した。
この環境のシステムには`libpipewire-0.3-dev` / `libspa-0.2-dev`が未導入で、clang /
libclang-dev / pkg-configは導入済み。通常のシェルからビルドするための追加導入コマンドをユーザーへ案内した。

## 2026-10-06追記: Linuxコントローラーの比較用1ms待機を撤去

基準HEADは`18950962`。ユーザーから、通常のイベント待機方式でコントローラーの動作に
問題がなかったとの報告と、比較用機能の削除指示を受けた。
設定チェックボックス、6言語の翻訳、`linux_gamepad_legacy_poll`、入力threadの切替stateと
setter、旧`park_timeout(1ms)`分岐、appからの切替呼び出しを撤去した。
診断JSONの`linux_gamepad_wait`はLinuxで`blocking_deadline_max_50ms`を報告する。
操作仕様と現在の比較手順を更新し、過去のA/B計測記録は履歴として保持した。

旧TOMLに`linux_gamepad_legacy_poll = true`または`false`が残っていても読み込みを継続し、
次回保存時に旧キーが消える。キーボード有効値やデバイススロットなど他の設定を保持する
回帰テストを追加した。ユーザーのconfig / profile / DBは直接編集していない。
Linuxの通常イベント待機、スクラッチrelease期限、即時空復帰時のbackoffは変更していない。

1ms待機・周期が残る箇所（今回の変更対象外）:

- `src/input/capture.rs`: macOSでgilrsのbackendがある場合の`park_timeout(1ms)`と、
  Windows / macOS / Linux以外向けの共通取得ループの`park_timeout(1ms)`。
- `native/gamecontroller.m`: macOS GameControllerの1ms周期timer。
  入力はイベント通知で受け取り、timerはRust側`PadState`のスクラッチ停止判定などを進める。
  この経路が有効ならgilrs取得ループは使わない。
- `src/input/native_capture.rs`: Windowsの`MsgWaitForMultipleObjectsEx`のtimeoutが1ms。
  WM_INPUT到着時は即時起床し、timeoutでゲームパッド取得とアナログ停止判定も進める。
- `src/input/gilrs.rs`: Linuxのイベント待機timeoutの下限は1ms、上限は50ms。
  固定1ms sleepではなく、イベント到着で解除される待機の期限である。

上記パスは`crates/bmz-player/`からの相対パス。macOS / Windows / その他OSの実機確認は未実施。

検証:

- `cargo fmt --check`、playerの`check` / `all-targets Clippy -D warnings`は成功。
- playerテストは2230成功・0失敗・19 ignored。旧設定互換、翻訳キー整合、
  スクラッチ停止・切断と待機期限の既存テストを含む。子プロセス内の再実行1件は加算していない。
- 通常のrelease build成功。旧キーをtrue、gamepadを有効にした分離データで
  Plasma / Wayland上の同梱sampleを自動演奏し、結果画面・正常終了まで確認した。
  `linux_gamepad_wait=blocking_deadline_max_50ms`、最終poll_cycles / blocking_waitsは317 / 317、
  early_empty_wakes・入力drop・WARN / ERRORは全て0。実キー/軸イベントは0件で、
  今回は手動コントローラー操作を再実施していない。性能比較や物理入力遅延の測定ではない。
- workspace全体と追加featureの再検証は未実施。
- 生ログは`.local/performance/linux-gamepad-poll-removal-2026-10-06/`に保存した。
