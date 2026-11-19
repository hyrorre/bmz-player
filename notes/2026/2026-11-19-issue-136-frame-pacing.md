# FIFO系でHz以上のFPS上限の周期待機を省く（#136）

- 対象: [Issue #136](https://github.com/hyrorre/bmz-player/issues/136)。ユーザーの指示により案Aだけを通常動作へ組み込む。
- 起点: 最新main `ab1808788f5d6dcc639cd6013b370463bccdda3e`。
- 実装ブランチ: `codex/fix-fifo-frame-limit-136`。
- 以前の比較試作と記録はローカルの`codex/issue-136-frame-pacing` / `db89f451`に保持した。
  今回のPRには他案、比較用CLI、計測スクリプト、追加TRACEを含めない。
- 現在の仕様: [frame-pacing](../../docs/frame-pacing.md)、[設定画面](../../docs/settings.md)。

## 採用条件と変更

実効present modeがFifo / FifoRelaxed、focused、surfaceが正常で、設定上限が現在の表示Hz以上の場合に、
内部の上限を0としてアプリ側の周期待機を省く。保存する`target_fps`と排他全画面のmode選択に使う値は変更しない。
設定値ではなく、rendererで正常適用されたfallback後のmodeを見る。

Mailbox / Immediate、background、無制限設定、0サイズ・surface復旧中では表示情報の新しい取得を要求しない。
復旧中は既存の最大30FPSとより低いユーザー上限を維持する。Hz未満や取得不能時は設定した上限のまま待つ。
判定は分子・分母を保持したu64の比較で、丸めや許容幅を追加しない。
59.94系では60FPS、240.084Hzでは241FPS以上が対象となり、240FPSは維持する。

Windowsは既存の`window_config::windows_display::active_paths`を再利用し、QueryDisplayConfigのrefreshRateを取得する。
winit 0.30.13のWindows実装の`dmDisplayFrequency * 1000`は小数Hzを失うためfallbackに使わない。
同じGDI出力に異なるHzの複製先がある場合も、表示周期が一つに定まらないので上限を維持する。
非WindowsではwinitのmHzを使い、排他全画面は現在のVideoModeHandleを使う。
ただしX11（Xlib / Xcb）は取得不能として扱い、Hzを問い合わせずに設定上限を維持する（後述の計測を参照）。

現在のmonitorは対象フレームごとに照合する。OS問い合わせは専用workerで行い、開始間隔は最短1秒、同時実行は1件までに制限する。
Moved / Resized / ScaleFactorChanged / Focusedとwindow mode・表示先の設定変更では再取得要求の世代を更新し、古い結果を捨てる。
初回・monitor変更後は取得待ちの間も設定上限を維持する。同じmonitorの再取得中は直前の有効値を使う。
表示世代はmonitorまたは取得Hzが実際に変わった場合だけ進め、FramePacingStateへ含める。
待機予約の前にも状態を同期して古いdeadline・wakeとFPS統計を捨てる。
Waylandの既存callback、入力・判定・音声の独立した処理、Windows Immediateの短時間待機は変更しない。

## PRレビューでの修正（2026-11-19）

初版は描画・イベント経路で1秒ごとにQueryDisplayConfigを同期実行していた。
また、invalidate後はHzが同じでも表示世代が進み、Moved / Resized等で統計・deadlineを不要にリセットしていた。
どちらもコード上で確認できる問題であり、先行15秒計測のp99だけでは1秒ごとのスパイクを否定できない。

OS問い合わせをworkerへ移し、描画側は`try_recv` / `try_send`だけを使う。
イベントの連続で開始間隔を短縮せず、実行中の1件が終了するまで追加要求を出さない。
再取得要求の世代と公開する表示世代を分け、同じmonitor・同じHzでは統計・deadlineをリセットしない。
worker起動・通信失敗は上限維持とし、同期問い合わせへfallbackしない。
問い合わせ全体の所要時間をworker内のdebugログ`query_us`へ記録する。
worker状態の追加でWinitAppが65,536 bytesとなり、既存のサイズ制約テストが失敗したため、
workerを含むcacheをヒープに置いた。64KiB未満というサイズ制約も再確認した。

外部mode変更はイベントがなくても定期的に再確認するが、取得時間や対象フレームの間隔を含めた反映時間の上限は保証しない。
同じmonitorの再取得中にmodeが変わった場合、完了結果を適用するまでは直前のHzが残る。
これは描画経路でOS問い合わせを待たないこととの交換であり、表示の改善を実測済みとするものではない。

## 非WindowsでのHz取得コストとX11の追従不良（2026-11-19）

PRレビューで、Windows以外で対象フレームごとに呼ぶ`window.current_monitor()`のコストを指摘された。
PR初版`0d41cb8a`の`DisplayTimingCache`とFIFO判定を組み込んだ計測用プログラム（winit 0.30.13、release）で測った。
詳細は[Linuxの計測](https://github.com/hyrorre/bmz-player/pull/139#issuecomment-7010923817)と
[macOSの計測](https://github.com/hyrorre/bmz-player/pull/139#issuecomment-7010959276)のPRコメントにある。

Linux（Debian 13 x86_64 VM、約60Hzの単一仮想出力）。2万反復のバッチ41本、3実行の中央値。

| 測定対象 | X11（Xorg 21.1.16 / dummy） | Wayland（Weston 14.0.2 / headless） |
|---|---:|---:|
| `current_monitor()` 1回 | 0.29 µs | 0.031 µs |
| FIFO判定（cache有効）1回 | 0.32 µs | 0.076 µs |
| 同じFIFO判定を2回 | 0.63 µs | 0.15 µs |

`strace`では、計測区間に表示サーバーとの通信はなかった。X11で2回判定するコストは、240FPSの予算の約0.015%にあたる。

macOS（27.2.1 / arm64、内蔵ディスプレイ1台、120,000 mHz、focused）。

| 測定対象 | サンプル数 | 平均 | p99 | 最大 |
|---|---:|---:|---:|---:|
| `current_monitor()`単体・連続 | 10,000回 × 3 | 1.951〜3.989 µs | 2.042〜10.917 µs | 61.000 µs |
| cache経由の取得3回合計・連続 | 5,000組 × 3 | 5.770〜6.006 µs | 6.667〜7.292 µs | 39.500 µs |
| 毎回invalidateして取得 | 200回 × 3 | 1.879〜2.048 µs | 2.250〜2.666 µs | 10.042 µs |
| 約8.33msの待機を挟みRedrawRequested内で3回 | 720組 | 75.684 µs | 162.333 µs | 217.042 µs |

待機を挟んだ条件の平均は、120Hzの予算の約0.9%だった。ms単位の停止は観測していない。
「3回」は評価用の条件で、BMZ本体のフレーム当たりの呼び出し回数ではない。
どちらもAPIとcache処理の計測で、BMZ本体の描画・present待ち・実表示は含まない。
外部ディスプレイ、monitor移動、全画面切替は未検証。以上から、取得コストを理由にした追加対策は行わない。

一方、X11ではHz変更への追従不良を再現した。ウィンドウを開いたまま`xrandr`で約60Hzから75Hzへ変えると、
新規プロセスは75.001Hzを取得したが、既存ウィンドウの`current_monitor()`は2.5秒後も60.011Hzを返した。
設定上限70FPSのFIFO判定は上限を外したままで、設定を超える75FPSで動き続ける。
workerも同じmonitor handleを読むため、`51e5ebe8`の時点でも解消していなかった。
そこで、display handleがXlib / Xcbの場合は取得不能として扱い、問い合わせを行わず設定上限を維持する。
XWayland上で動く場合もX11として扱う。XRandRから直接取得する方法は、必要になったときに検討する。

この修正をLinux（クラウド環境）で確認した際、`51e5ebe8`の`try_send`がclippyの`result_large_err`で失敗した。
Linuxのwinitのmonitor / video mode handleが大きく、`TrySendError<RefreshRequest>`が280 bytes以上になるため。
Windowsでは発生しない。worker宛ての要求を`Box`で送るように変更した。
Linuxで`cargo clippy -p bmz-player --all-targets --locked -- -D warnings`と
`cargo test -p bmz-player --locked --lib frame_runtime`（24件）が成功した。この時点ではWindowsでの再確認は未実施だった。
統合後のWindows検証は末尾の記録を参照。

## Windowsの予約済みmonitor切断（2026-11-19）

`51e5ebe8`ではworkerのHz取得で`monitor.name()`を使っていた。
winit 0.30.13のWindows実装は`GetMonitorInfoW`の結果をunwrapするため、予約後に切断されたHMONITORではpanicし得る。
BMZ側もworker切断後は取得を停止するため、Hz不明として上限を維持したまま復帰しない経路が残っていた。
[winitの実装](https://github.com/rust-windowing/winit/blob/v0.30.13/src/platform_impl/windows/monitor.rs#L135-L156)と、
[HMONITORの有効期間](https://learn.microsoft.com/en-us/windows/win32/gdi/hmonitor-and-the-device-context)を確認した。

WindowsのHz取得ではraw HMONITORから`GetMonitorInfoW`を直接呼び、失敗・空のdevice名は`None`を返す。
この失敗はworkerを終了させず、同じworkerが次の問い合わせを受け付ける。問い合わせは従来どおりworker内で行う。
有効なdevice名を取得できた後のQueryDisplayConfig失敗・対象path消失も既存のHz不明の経路で扱う。

回帰テストは、実際のWindows APIへ無効なnull HMONITORを渡してpanicせずHz不明となることと、
同じworkerへの「失敗 → monitor変更 → 有効なHz」という模擬応答で、設定上限の維持から待機省略へ戻ることを確認する。
実機での抜き差し・再接続は未検証。再確認時はプレイ中に表示先を切断・再接続し、
`display timing worker stopped`が出ないこと、取得不能時は設定上限、有効なHzの再取得後は条件に応じて実効上限0へ戻ることを確認する。

## 先行調査と限界

Windows 11 / RTX 5090 / DX12 / Windowed 1280×720 / 既定7K skin / 同一譜面の定常15秒で探索した。
実際の表示先の取得値は240.084Hzだった。現行241FPSのCPU frame間隔p99は4.634ms、案Aの2回は4.563 / 4.517ms。
480FPSでは現行でもアプリ待機が0回だったため、その区間の改善は確認できなかった。
これらはアプリ側の間隔であり、実表示のカクつき改善を示す値ではない。

PresentMon 2.3.1 / 2.6.0ではイベント欠落が生じ、実表示間隔・表示遅延・GPU実行時間は取得できていない。
NVIDIA側はG-SYNC互換、垂直同期はアプリ設定、ドライバーFPS上限・低遅延・スムーズモーションはOFF。
モニター側のG-SYNC Compatible / FreeSync PremiumProはONとの申告があるが、各runのVRR実動作は未確認。
設定名や取得HzからVRR作動を推定しない。今回も実表示が改善したと主張しない。

## 検証

### 非同期取得への変更

- Windowsで`cargo fmt --check`、`cargo check -p bmz-player --locked`、
  `cargo clippy -p bmz-player --all-targets --locked -- -D warnings`が成功。
- レビュー修正後の`cargo test -p bmz-player --locked -- --test-threads=1`はlib 3,364成功・94 ignored。
  bin / doctestも成功し、startup integrationの2件は既存のignored。
- `cargo build -p bmz-player --release --locked`が成功。
- Luxez-Flat / Rmz-skin / mz-selectを初期化した状態でテストした。外部素材のない早期returnと
  GPU / Winitのignoredテストを、実機確認や互換確認に数えない。
- 生ログはmain checkoutの`.local/issue-136/A-*.log`に保存し、コミットしない。
  レビュー修正後は`A-review-check-final.log`、`A-review-clippy-final.log`、`A-review-test-final.log`を参照する。
  リリースビルドの結果は`A-review-release.log`に保存した。
- 初回の制約下の全体テストでは通信・directory link系と上記サイズ制約の失敗を検出し、途中で停止した。
  cacheをヒープに移し、ローカル通信・ファイル参照の実行制約を外した再実行で上記の全体テストが成功した。
  問い合わせを意図的に止めたworker、イベント連続時の取得頻度と同一Hzの世代維持、古い結果の破棄を含む。

### Windowsの切断対応

- `cargo fmt --check`、`cargo check -p bmz-player --locked`、all-targets Clippy（`-D warnings`）が成功。
- 全体libテストは3,365成功・1失敗・94 ignored。追加した無効HMONITORの実APIテストと、worker復帰の模擬テストは成功。
  失敗した既存の`system_proxy_is_preserved_but_local_redirects_and_metadata_never_reach_it`は、
  子プロセスによる試験用一時フォルダーへの保存で「アクセスが拒否されました」(os error 5)となった。
  同じTEMP/TMP・実行条件でその1件だけを再実行すると成功した。ダウンロード処理への修正は行っていない。
- libの失敗で未実行だったbin / startup integration / doctestを別途実行した。bin / doctestは成功、startupの2件は既存のignored。
- `cargo build -p bmz-player --release --locked`が成功。
- ログはmain checkoutの`.local/issue-136/A-hotplug-check.log`、`A-hotplug-clippy.log`、
  `A-hotplug-test.log`、`A-hotplug-retry.log`、`A-hotplug-remaining.log`、`A-hotplug-doctest.log`、`A-hotplug-release.log`に保存した。
- その後、先行したX11修正`0278ac54`を保持して統合し、追加テストもBox形式の要求へ合わせた。
  統合後のWindowsでfmt / check / all-targets Clippy、`--lib frame_runtime`（25件）、`--lib window_config`（16件）が成功した。
  統合後のrelease buildも成功した。
  全体libテストの上記結果は統合前の実行であり、統合後は変更が重なる2モジュールを再確認した。
  統合後のログは`.local/issue-136/A-hotplug-integrated-*.log`に保存した。

再確認は`cargo test -p bmz-player --locked frame_runtime -- --test-threads=1`を使う。
境界条件、取得不能、モード除外、background・suspend、無制限時の取得省略、表示世代変更時のdeadline/wake破棄を確認する。
実機では設定変更、monitor移動、Hz変更、全画面切替、最小化・復旧を確認し、保存値が変わらないことと
`frame pacing state changed`の実効上限・表示世代を確認する。表示情報の詳細は`bmz_player::app::frame_runtime::display=debug`で記録できる。

レビュー修正の実機比較では、同一の曲・skin・画面条件でwarmup後の同一区間を30秒以上、複数回採る。
`bmz_player::app::frame_runtime::display=debug`の`query_us`の最大値と問い合わせ時刻を確認し、
`bmz_player::frame_pacing=trace`の`frame cadence sample` / `interval_us`からp99.9・最大値を算出する。
1秒間隔の問い合わせと大きなframe間隔が重なるかを時系列でも照合する。
詳細ログなしのrunも比較し、ログ負荷を分ける。同一monitor内の移動・resize後にHzが変わらない場合は、
`display_revision`が不要に進まないことを確認する（focusやwindow modeの変更自体による状態同期とは区別する）。

実表示間隔の比較、VRRの実動作、CPU/GPU負荷、他OS/backendの確認は残る。以前の操作ツール停止後に再計測は行っていない。
FPS上限を下げたときに固定Hzで必要となる繰り返し表示と、不規則な待機による表示落ちは別に評価する。
