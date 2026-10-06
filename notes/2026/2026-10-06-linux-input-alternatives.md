# evdev以外のLinux入力経路とWaylandイベント配送の比較

実施日: 2026-10-06 JST。基準HEAD: `a7fb29f5619e0da3d6f94c8b4e623c9ae4724b30`。
ブランチ: `feat/linux-latency`。既存のRmz-skin/mz-selectのgitlink差分は保持。
実装commit: `cbfe6d67`（比較診断）、`3ef405f7`（Wayland描画待機）。
前回の[Linux音声・入力調査](2026-10-06-linux-latency.md)を受け、evdev以外の候補を調べ、
計測で効果を確認できた範囲を実装した。現在の手順は[Linux遅延検証](../../docs/linux-latency.md)、
描画の契約は[frame-pacing](../../docs/frame-pacing.md)を参照。

## 調査と採用判断

使用中のwinit 0.30.13とwgpu 29.0.3の依存ソースを確認した。

- winitの公開KeyEventにはOSイベント時刻がない。Waylandのwl_keyboard受信処理でもtimeを
  公開イベントへ渡していない。DeviceEventへ変更するだけでも同じevent loopを待つ。
- LinuxのFPS待機は既にControlFlow::WaitUntilへ返す実装であり、待機中の入力は即時処理できる。
  固定sleep短縮やbusy loopを追加する理由はなかった。
- 現BMZはwinitの`Window::pre_present_notify()`を呼んでいなかった。
  winitのWayland実装はこの通知でsurfaceのframe callbackを要求し、返答までRedrawRequestedだけを
  抑止する。入力やuser eventは処理できる。wgpuの公開surface取得APIに非同期の取得通知や
  呼び出し単位のtimeout指定はなく、描画先取得待ちをウィンドウスレッドから切り離す別設計は大きくなる。
- 実際のWayland/Vulkan/VSyncで、通常描画のsurface取得待ちがdebug約5ms、release約6msを占めた。
  通知追加後は約0.04〜0.06msまで減り、人工イベント配送のp95も下がったため、この局所修正を採用した。
- XI2公開APIは別X接続/専用threadで待てるが、現在接続できるXサーバーはXWAYLAND extensionあり、
  XIQueryVersion成功（2.4）。ネイティブX11の物理入力比較はできていない。入力源排他・focus・
  session・時計変換を伴う別backendは追加しなかった。
- hidrawノード28件中、現在ユーザーがread可能なものは2件だった。reportは読んでいない。
  測定対象の物理コントローラーがないため、HIDAPIの高速化効果は未測定。
- libinput/libevdevはevdev権限の代替ではない。Waylandの別接続は既存ウィンドウのwl_keyboardを
  引き継げない。InputCapture portalはアプリによる即時capture開始を保証せず、通常ゲーム入力の
  差し替えには採用しなかった。

一次資料はdocsのリンクに加え、ローカルCargo registryの
`winit-0.30.13/src/window.rs`、`platform_impl/linux/wayland/window/mod.rs`、
`platform_impl/linux/wayland/event_loop/mod.rs`、`seat/keyboard/mod.rs`、
`platform_impl/linux/x11/event_processor.rs`、`wgpu-29.0.3/src/api/surface.rs`を確認した。
依存ライブラリの更新・追加はない。

## 実装

- bmz-renderは取得済みsurfaceのpresent直前だけ通知callbackを呼ぶ入口を追加。
  従来の`render_last_plan()`も維持する。取得失敗・再構成・skipped/headlessでは通知しない。
  appだけがwinitを操作し、rendererへwinit依存を追加していない。
- 実際のdisplay handleがWaylandかつ実効Fifo/FifoRelaxedのときだけ通知する。
  XDG_SESSION_TYPEだけでは判断しない。Immediate/Mailboxと他OS/backendは従来通り。
  このcallbackで描画を待つ場合はUnlimited FPSでもControlFlow::Waitに戻し、Pollの空回りを防ぐ。
- `--latency-event-loop-probe`で11ms周期の人工通知を送る。送信直前のInstant→user_event処理を
  既存LatencyHistogram/JSONへ出力する。入力・判定・音声コマンドは注入しない。
  最大32件、warm-up 2秒、scene/focus/gameplay世代別epoch、旧epoch/未来時刻の除外、
  未送信/未処理/送信周期欠落の件数を持つ。終了はchannelで起床してjoinする。
  Linux/macOSの共通SuspendMonitorを再利用し、スリープ復帰も新しいepochにする。
- `--latency-legacy-wayland-present`は同じ実行ファイルで通知を外す比較用フラグ。
  いずれの新規フラグも停滞試験と同じ保存/IR抑止を使い、6言語の検証表示を共通化した。
- 入力backend、音声backend、バッファ256、VSync/FPS/frame latencyの設定既定値は変更していない。
  判定/キーバインド/スクラッチ/リプレイの意味、ウィンドウfocusによる配送条件も変更していない。

## ネイティブrelease比較

前回と同じUbuntu 26.04.1実ホスト、Wayland/Vulkan、同梱sample自動演奏、1280×720、
VSync/FPS設定240、音声Auto→PulseAudio→pipewire-pulse、48kHz/Fixed256。
条件ごとに新規のconfig/profile/DBを作り、IR/OBS/起動scan/表取得/更新確認を無効化。
検証中はビルドや別のBMZを走らせなかった。各約16.5秒、Play epochの最初2秒を分布から除外。
診断とprobeは両側ON。1回目は従来→変更後、2回目は変更後→従来の順。
CPUは起動5秒後から5秒間のprocess ticks/実経過時間、1コア=100%。

測定artifact SHA256:
`314d81983760c21145ae8cab1608ce9382c4c666e2e5265b096dbef1005ea60a`。
この後にprobeのスリープ検出を共通SuspendMonitorへ接続し、Clippy指摘とテスト配置を修正した。
通知の成功時経路は同じである。最終候補の確認は後述。

人工通知の送信→処理（ms）。p値は対数bucket上端による近似、maxは観測値。

| 描画 | 方式 / 回 | 件数 | p50 | p95 | p99 | max | CPU % |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 通常 | 従来 / 1 | 1212 | 3.670 | 7.340 | 67.109 | 142.687 | 34.00 |
| 通常 | 変更後 / 1 | 1207 | 0.037 | 0.852 | 1.311 | 12.093 | 31.20 |
| 通常 | 変更後 / 2 | 1206 | 0.033 | 0.852 | 1.442 | 6.714 | 35.00 |
| 通常 | 従来 / 2 | 1206 | 3.670 | 6.816 | 7.340 | 25.607 | 35.00 |
| 100ms停滞 | 従来 / 1 | 1205 | 3.932 | 7.864 | 83.886 | 99.868 | 31.80 |
| 100ms停滞 | 変更後 / 1 | 1206 | 0.037 | 1.835 | 83.886 | 101.131 | 33.99 |
| 100ms停滞 | 変更後 / 2 | 1207 | 0.031 | 1.442 | 83.886 | 100.088 | 26.40 |
| 100ms停滞 | 従来 / 2 | 1206 | 3.670 | 7.340 | 83.886 | 99.140 | 15.20 |

通常時のsurface取得平均は従来6.026/5.920ms、変更後0.056/0.060ms。
最後の5秒窓のCPU描画開始間隔平均は従来6.945/6.965ms、変更後6.975/6.965msで、
双方約144fps。これはCPUの描画開始周期であり、物理表示時刻を測ったものではない。

従来1回目の通常試験には142.7msの外れ値があった。原因を特定できていないため除外しなかった。
CPUは短い窓でばらつきがあり、CPU使用率の改善とは結論づけない。
100ms停滞時のp99は前後とも83.9msであり、ウィンドウスレッド停止からの独立性は得られていない。

全8回で音声stream error/command drop/input queue dropは0。物理入力件数は0。
timeline catch-upは通常従来1回目のみ1、他は0。確認済みxrunは未取得で、0としない。
probeのPlay中capacity skipは全回0。初期ロード中の満杯による未送信はprocess累積値に含まれる。
各score_historyは0件。APIの音声出力推定をこの人工通知の分布へ加算していない。
物理押下→アナログ出力、手動キー音の発音遅延、USB/スイッチの遅延は未測定。

## 補助debug比較

最初に元のHEADで既存frame profilerを使い、通常/停滞を各2回測った。
その後、同じprobeを含むdebugの前後をそれぞれ通常/停滞2回ずつ測った。
通常2回目の配送p95/p99は従来6.816/7.340ms、変更後1.835/2.884ms。
従来1回目には287.8msの外れ値（p99 184.5ms）があり、こちらも削除していない。
主たる採用判断とCPU比較は上の同一releaseビルドによる結果を使った。

debug artifactのSHA256は、変更前
`d9e970c8efbb76a57accb7969607c03653b64a18f8266de555a78e1682f768d3`、変更後
`6c19d14e586bc69ea5f155f01746ecbd02b9e29497f25b8eee147e7669f990ea`。

生ログ、試験用config/DB、CPU、実行スクリプト、JSON集約は
`.local/performance/linux-input-alternatives-2026-10-06/`へ保存した（Git対象外）。
通常のconfig/DB、ホストの権限・サービス・設定、インストール済みFlatpakは変更していない。

## Flatpak release比較

Freedesktop SDK/runtime 25.08、`--no-default-features --features pulseaudio,pipewire`でビルドし、
`flatpak build --runtime --readonly`から実行した。元のmanifestの権限に加え、検証config/DBの
一時ディレクトリだけを公開した。配布権限は変更していない。新規XDG_CONFIG_HOMEでwrapperの
Pulse cookieも隔離した。同梱default skin/sampleだけを確認し、未取得のLuxez-Flatを含む
bundle一式の完全性やインストール後の挙動は今回も確認していない。

最初の起動では`flatpak build`内にDISPLAY/WAYLAND_DISPLAYがなく終了した。ソケット公開と
環境変数の引き渡しは別であり、`--socket=wayland`とホストの実際の`WAYLAND_DISPLAY`を
明示して解決した。この失敗ログも保存した。ホストのWaylandや音声サービスは変更していない。

同じsample/描画/音声条件、通常2回（順序を反転）と停滞1回ずつ。CPU窓もnativeと同じ。
この比較のSHA256は`ebbdb6f4d2c83cd4da7c6eff5b0dd60fc2f501bf605ab47c6b58a3d00276593d`。
Unlimited FPS待機の補完前のartifactだが、この表は全てFPS設定240で、変更対象の分岐を通らない。

| 描画 | 方式 / 回 | 件数 | p50 ms | p95 ms | p99 ms | max ms | CPU % |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 通常 | 従来 / 1 | 1207 | 3.670 | 6.816 | 7.340 | 34.456 | 33.80 |
| 通常 | 変更後 / 1 | 1206 | 0.037 | 0.918 | 1.704 | 2.208 | 34.40 |
| 通常 | 変更後 / 2 | 1206 | 0.037 | 0.983 | 1.573 | 2.242 | 33.20 |
| 通常 | 従来 / 2 | 1206 | 3.932 | 41.943 | 201.327 | 313.966 | 30.60 |
| 100ms停滞 | 従来 / 1 | 1206 | 3.670 | 7.340 | 83.886 | 98.826 | 32.40 |
| 100ms停滞 | 変更後 / 1 | 1205 | 0.045 | 1.704 | 83.886 | 97.989 | 34.60 |

通常時のsurface取得平均は従来5.947/6.330ms、変更後0.065/0.066ms。
従来2回目の314ms外れ値は原因未特定のまま表に含めた。全6回で音声stream error/command drop/
input queue dropとPlay中のprobe capacity skipは0。timeline catch-upは通常従来1回目に1、他は0。
確認済みxrun・物理入力/手動発音遅延は未測定。nativeとFlatpakのCPU差からsandbox性能を結論づけない。

## 自動検証

```bash
cargo fmt --check
cargo check --workspace --locked --features bmz-player/pipewire,bmz-player/linux-evdev
cargo clippy --workspace --all-targets --locked --features bmz-player/pipewire,bmz-player/linux-evdev -- -D warnings
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked --features bmz-player/pipewire,bmz-player/linux-evdev --no-fail-fast
cargo test -p bmz-player --locked --features pipewire,linux-evdev --lib -- --skip skin_loader::tests
cargo test -p bmz-player --locked --lib -- --skip skin_loader::tests
cargo test -p bmz-player --locked --features pipewire,linux-evdev frame_flow
python3 -m unittest discover -s scripts -p 'test_*latency*.py'
```

fmt/checkと両feature構成のall-targets Clippyは成功。Unlimited待機の追加後もworkspace check/
feature有効Clippyとframe_flowテストを再実行した。probeの上限・warm-up・古い世代・未来時刻、
CLI既定OFF/停滞との独立性、Wayland/実効mode選択、surface未取得時の通知抑止、JSON分離を検証した。
機能有効player（skin_loader除外）は2032成功/2ignore、既定featureは2021成功/2ignore。
Unlimited待機の追加前の全体実行はplayer2202成功/23失敗、render680成功、audio117成功、
gameplay235成功、core13成功等。同じ入力列の判定/PCM・描画consumer停滞の既存テストも成功。
Pythonは6成功。

workspace全体は、前回と同じ外部スキン由来のplayer23件/skin4件が失敗した。
Luxez-Flat欠落やRmz-skin/mz-selectのチェックアウトとの不一致で、ユーザーのgitlinkは修正していない。
素材依存テストのskip/ignoreは互換確認済みとは数えない。
macOS/Windowsの実ビルド・実入力は未実施。依存差分なし、Waylandだけの動作条件、
既存の設定・描画・gameplay回帰テストで確認できる範囲を確認した。

## 最終候補の確認

Unlimited FPSでの空回り対策後にnative/Flatpakを再ビルドした。Flatpakは保持したSDK build treeで
オフラインの差分ビルドを行い、検証用/appだけを更新した。最終artifactのSHA256:

- native: `7e8d0aacc028e56a41b97af601bc99751d3775e79efe498692b3a5dbb848d219`
- Flatpak: `e074d5604bbc22b36ef25b62bffdd7e297ed7f749f4ffbadc376a7bbb22e47c7`

同じsampleで逐次1回ずつ起動〜Result〜終了を確認した。

| 最終確認 | 件数 | 人工配送p95 / p99 / max ms | CPU % |
| --- | ---: | --- | ---: |
| native Wayland / FPS設定240 | 1206 | 0.852 / 1.442 / 7.305 | 33.00 |
| native Wayland / Unlimited | 1206 | 0.852 / 1.704 / 2.263 | 34.19 |
| native XWayland / FPS設定240 | 1206 | 12.583 / 13.631 / 16.468 | 29.40 |
| native Wayland / FPS設定240 / probe・latency診断OFF | 未取得 | 未取得 | 29.20 |
| Flatpak Wayland / FPS設定240 | 1206 | 1.049 / 1.573 / 2.072 | 33.80 |
| Flatpak Wayland / Unlimited | 1205 | 0.918 / 1.573 / 2.305 | 29.80 |

UnlimitedでCPUが1コアを使い切る空回りは観測しなかった。XWaylandの実display handleはx11で、
Wayland通知は適用していない。この1回からX11専用入力APIの性能を結論づけない。
診断OFFではJSON/probe sampleを出さず終了した。frame profilerは両側で有効で、診断OFFは
検証bannerも出ないため、上のCPU差をprobe単独の正確なoverheadとして扱わない。

診断ONの5回すべてで音声stream error/command drop/input queue dropは0。
timeline catch-upはnative Wayland FPS240に1、他は0。xrunは未取得。
全6回のscore_historyは0。ただし自動演奏なので、このDB確認だけで新CLI単独の保存抑止を
検証したとは扱わない。新CLIも既存の保存抑止分岐へ接続した。
実キー、focus移動/ロック/スリープや画面resizeを伴う手動確認は未実施。

## 次の実機確認

- 実キーの短打/連打/長押し/同時押し、JIS/IME、focus切替/最小化/ロック/復帰で既存配送を確認する。
- 通常/高密度スキン、異なるcompositor、Vulkan/GL、60/144/240Hz、低FPS/Unlimitedで比較する。
  同じ描画設定でprobeなしの長時間安定性も確認する。
- ウィンドウresize/表面再構成や非表示後に描画が復帰することを確認する。
- XI2はネイティブX11実機と同じ物理入力で測定してから判断する。XWaylandの人工通知を
  ネイティブX11/物理キーボードの計測結果として扱わない。
- 物理押下から発音までの総遅延は測定機材で別途測る。今回の人工通知p95の差を代入しない。
