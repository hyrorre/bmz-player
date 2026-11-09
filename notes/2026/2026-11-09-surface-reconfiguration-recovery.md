# Surface再設定失敗後のクラッシュ経路の修正

## 対象と確認済みの不備

- 開始時の最新main: `da28439a86ff51c519d4856aca8325f8eeb96eee`。
- `Cargo.lock`: wgpu 29.0.3、winit 0.30.13。依存更新は行わない。
- 報告はWindows v0.5.1で、ウィンドウモード起動後に標準タイトルバーの「□」を押す操作。
  排他フルスクリーンへの変更ではない。
- 報告者のGPU、バックエンド、ドライバ、解像度、DPI、クラッシュログは未確認。
  最初の再設定失敗がその環境で実際に起きたか、何が失敗を引き起こしたかは未確定。

現行mainにも、Resized → resize_surface → resizeでconfigを先に書き換え、
configureのエラーをログへ残した後も次のget_current_textureへ進む経路があった。
Lost / Outdated、present設定変更も再設定後のSurface利用可否を管理していなかった。

wgpuのローカルCargoソースと次の上流ソースを確認した。

- [configure前のpresentation取り外し](https://github.com/gfx-rs/wgpu/blob/v29.0.3/wgpu-core/src/device/resource.rs#L5161-L5199)
- [未設定Surfaceの取得エラー](https://github.com/gfx-rs/wgpu/blob/v29.0.3/wgpu-core/src/present.rs#L154-L163)
- [Validationを返す前のエラーハンドラ呼び出し](https://github.com/gfx-rs/wgpu/blob/v29.0.3/wgpu/src/backend/wgpu_core.rs#L3921-L3935)
- [DX12のResizeBuffers失敗](https://github.com/gfx-rs/wgpu/blob/v29.0.3/wgpu-hal/src/dx12/mod.rs#L1346-L1362)
- [Vulkanの旧swapchain取り外し](https://github.com/gfx-rs/wgpu/blob/v29.0.3/wgpu-hal/src/vulkan/instance.rs#L996-L1035)
- [新swapchain作成の結果判定前に旧swapchainを破棄](https://github.com/gfx-rs/wgpu/blob/v29.0.3/wgpu-hal/src/vulkan/swapchain/native.rs#L154-L247)
- [Outdated / Lost / SuboptimalのAPI契約](https://docs.rs/wgpu/29.0.3/wgpu/enum.CurrentSurfaceTexture.html)
- [DeviceLost callback](https://docs.rs/wgpu/29.0.3/wgpu/struct.Device.html#method.set_device_lost_callback)
- [Win32のextent要件](https://docs.vulkan.org/spec/latest/chapters/VK_KHR_surface/wsi.html#_win32_platform)

configure用error scopeの外で未設定Surfaceを取得すると既定のハンドラがpanicし得る。
configの数値を戻すだけ、Validation分岐だけではこの経路を修復できない。
特定のGPU・ドライバやスキンアップロードの並列処理を原因とは断定していない。
Vulkanでも旧swapchainの利用継続を仮定できない。通常のWin32 swapchain作成には現在の
ウィンドウ寸法が必要であり、Outdatedではappが現在の物理サイズを取得して再設定する。
報告者がVulkanを利用していたか、Resizedとの配送順の競合が発生したかは未確認。
上位層で得られないVkResultを推測して記録することはしない。

## 実装

現在の契約は[frame-pacing.md](../../docs/frame-pacing.md)に記載。
`SurfaceRecovery`は希望設定、利用可否、試行数・期限、終端エラーを持つ。
`WgpuRenderer::config`は正常適用済みの設定を保持する。各変更入口は同じ復旧処理を使い、
成功時だけconfig・サイズ依存キャッシュを更新する。appのabout_to_waitとRedrawRequestedが
期限付き再試行とscreen-size通知を進め、Windowsの移動・リサイズmodal loopにも対応する。
通常のconfigure error scopeと未捕捉GPUエラー検出は維持する。
Lostは同じSurfaceのconfigureで復旧しようとせず、Surface再作成が必要な旨を記録して終了する。
DeviceLost callbackはatomic flagとメッセージだけを公開し、renderer/app側が終了へ遷移する。
configureのscopeが空でもその通知を確認する。正常終了に伴うDestroyed通知は無視する。
Suboptimalは既存の有効画像の描画・presentを維持し、再設定の契機にしない。
取得画像、texture、viewは描画関数内に収まり、再設定は関数から戻った後で行う。
Device／Surface全体の再生成や、ドライバ内部のネイティブ例外の防止は対象外。

250msは毎フレームの失敗と大量ログを避け、ウィンドウ状態の変化を待つための間隔。
最大5回（初回＋4回）の待機は約1秒。0サイズ中は予算を消費しない。
上限では一度だけ終了要求を出し、既存のexiting処理で保存・音声・GPUを解放した後、
ネイティブダイアログと既存のcommand error経路で知らせる。
後述のレビュー対応で、smoke指定時はダイアログを省略するよう変更した。

eguiの差分は描画関数の外側でset → 描画またはスキップ → freeの順に処理する。
CPU側の更新とSurfaceへの描画は分離し、復旧中も最大30FPSでアプリ更新を継続する。
レビュー対応後は、より低い通常／バックグラウンドFPS上限を維持する。

## 回帰テスト

修正前の本番configure/acquire境界へテスト用フックを追加し、意図的なconfigure失敗の
直後に次フレームを描画するテストを実行した。取得呼び出し回数が期待0に対して1となり失敗。
修正後は同じ境界で取得0、期限到達時の再設定成功後に描画再開・取得1を確認した。

GPUを使うテストは実Surfaceの代わりにオフスクリーン描画先を使い、configure失敗と
Lost / Outdatedを注入する。実際のRenderer setter、render_last_plan、render_planを通り、
テスト専用の状態機械だけでは検証しない。時間はInstantを渡し、sleepには依存しない。
通常のGPU不要テストでも本番のSurfaceRecoveryの期限・予算・適用値を検証する。

```powershell
cargo test -p bmz-render --locked surface_recovery
$env:BMZ_TEST_SURFACE_BACKEND = 'dx12' # Vulkanは 'vulkan'
cargo test -p bmz-render --locked recovery_tests -- --ignored --nocapture --test-threads=1
```

後者はGPUが必要なため通常のworkspace testではignore。明示実行で以下を確認する。

- 正常描画 → 再設定失敗 → 次フレーム取得抑止 → 再試行成功 → 描画再開。
- resize、Outdated、present mode、frame latencyの再設定入口。
- Outdatedで保存済み寸法と異なる物理サイズを使い、0サイズを経た最新寸法で復旧。
- SurfaceLostで同じSurfaceを再設定せず終端通知すること。
- configure中（scopeにエラーなし）とフレーム間のDeviceLostで描画へ復帰しないこと。
- 最新の希望値への更新、同一setterの連続呼び出し、適用済みサイズ・present表示。
- 幅／高さ0でのconfigure・取得・予算消費の抑止と復元。
- 期限前の再試行抑止、最大5回、終端通知の一度だけの取得。
- egui全体作成 → 部分更新を保留中に適用し、復帰後の画像をreadbackで確認。
  さらに0サイズのスキップ中に解放されることを確認。

Suboptimalでは再設定を追加していないため、取得済み画像を保持したconfigure経路がないことを
コードで確認した。実SurfaceTextureのSuboptimal通知そのものをこの注入テストでは生成しない。

## 検証結果

- `cargo fmt --check`: 成功。
- `cargo check --workspace --locked`: 成功。
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: 成功。
- `cargo test --workspace --locked --no-fail-fast`: 成功。
  bmz-playerは2980 passed / 57 ignored、bmz-renderは1013 passed / 21 ignored。
- `cargo build -p bmz-player --locked`: 成功。
- GPUを使う復旧テスト10件をDX12・Vulkanでそれぞれ明示実行し、両方10 passed。
  設定数値を旧サイズへ戻しても取得が再開しないassertionを最後に追加し、両backendで再確認。
  このテスト追記後のfmtとbmz-render all-targets Clippyも成功。

最初の制限付き実行では、今回の変更箇所とは別のgilrs-core / Windows WGIスレッドで
SendError panicが発生し、bmz-playerのテストプロセスが終了した。OS機能を利用できる
実行環境でworkspace全体を再実行し成功した。制限が原因と断定する調査は行っていない。
incremental cacheのhard link失敗によるcopy fallback警告も出たが、検証は成功した。
既存の外部アセット依存テストには素材がない場合に早期returnするものがあり、workspace成功を
全外部スキンの互換確認とは扱わない。追加したGPU復旧テストは外部スキンに依存しない。

### Windows実機

- Windows 11 Pro 10.0.26300、NVIDIA GeForce RTX 5090。
- DX12ドライバ表示32.0.16.2114、Vulkan表示NVIDIA / 621.14。
- 3840×2160ディスプレイ。WindowsのAppliedDPIは168（175%）。
- debug build、独立した`BMZ_DATA_DIR`、同梱リソース、Windowed / Fifo / frame latency 1。
- `--renderer dx12` / `--renderer vulkan`をそれぞれ明示し、ログでも実際のbackendを確認。
- 両backendでプロセス開始後約12秒に標準タイトルバーの「□」を押して最大化し、描画継続。
  表示安定後にも最大化・復元を実施。最大化時の物理描画サイズは3840×2036。
- 両backendで最大化・復元、右辺のドラッグによる連続リサイズ、最小化・復帰を各2回確認。
  DX12は800×600から616×600、558×600、Vulkanは628×600、558×600へリサイズした。
  復帰後も描画が続き、閉じるボタンから通常の終了処理を通って`status="success"`となった。
- 実機操作ではconfigure失敗・SurfaceLost・DeviceLost・panicは発生しなかった。
  元のドライバエラーの再現、故障時のネイティブダイアログの実表示、初回表示前の最大化、
  他GPU・ドライバ・DPI・OSは未検証。実機の正常動作と失敗注入による復旧検証は区別する。

生ログは `.local/validation/surface-recovery-20261109/`（Git管理外）。
`check-final.log`、`clippy-final.log`、`workspace-test-final.log`、`injection-dx12.log`、
`injection-vulkan.log`、`dx12-final-stderr.log`、`dx12-startup-stderr.log`、
`vulkan-final-stderr.log`へ保存した。報告者のGPU・backend・ログおよび最初の失敗原因は
引き続き未確認であり、報告環境の原因確定・再現確認が完了したとは扱わない。

## PR #85のレビュー対応

- 復旧待ちと0サイズ時のCPUフレーム上限を、30FPS・通常の上限・バックグラウンド上限の
  最小値に変更。0（Unlimited）は比較から除外し、上限を両方指定しなくても30FPSで待機する。
  1／10FPSの設定を引き上げない。再設定期限はCPUフレーム期限と独立しており、
  低いFPSでも250msの再試行間隔を維持する。描画保留中の短時間spin抑止は維持した。
- 起動時のsmoke指定から通知方針を保存し、Surface復旧打ち切り／Lost後はモーダル通知を省略して
  エラーを返す。通常起動のネイティブ通知と、両モードでの既存の終了・ログ処理は維持する。
  実際の終了通知関数に通知先を注入し、全smokeオプションで通知なし・エラー返却、
  通常起動で一度だけ通知・エラー返却となることをテストする。
- スクリーンショット要求を描画成功まで保持する。新しいGPU回帰テストはレビュー対応前の
  コードで「復旧待ち後も要求が残る」のassertionに失敗した。修正後はconfigure失敗 →
  0サイズ → Outdatedを経ても要求を保持し、最新の24×16の描画成功後にPNG保存を確認した。
- smoke件数は元の要件どおり描画成功時だけ進める。描画不能が続くケースは呼び出し側の
  プロセスタイムアウトが必要であることを`docs/development.md`へ明記した。
  `--smoke-exit-on-result`は描画件数によらず終了する。

レビュー対応の生ログは同じ検証ディレクトリの`review-*.log`へ保存する。
スクリーンショット回帰の修正前失敗は`review-screenshot-red.log`、修正後のGPUテストは
`review-injection-dx12.log`と`review-injection-vulkan.log`（各11 passed）。

レビュー修正後の`cargo fmt --check`、workspace全体のcheck／all-targets Clippy／test、
`cargo build -p bmz-player --locked`はすべて成功。bmz-playerは2983 passed / 57 ignored、
bmz-renderは1013 passed / 22 ignoredで、追加したGPUテストは上記の別実行で検証した。
今回のworkspace testでは失敗はなく、incremental cacheのcopy fallback警告は引き続き発生した。

同じWindows実機・独立した検証データで、修正後のdebug実行ファイルを`--renderer dx12`／
`--renderer vulkan`・Windowed 800×600・3フレームsmoke・スクリーンショット指定で実行した。
両backendとも3フレーム描画、PNG保存、exit code 0を確認。
ログは`review-smoke-{dx12,vulkan}-stderr.log`。この時点ではレビュー対応後の最大化等の
手動操作は再実施していなかった。その後に行った確認は次節へ追記した。
実機での元のドライバエラー、故障時ダイアログの実表示は引き続き未確認。

### 2026-11-09 レビュー修正後のWindows操作確認

レビュー対応コミット`5075375a5d8bb400bef00dce1d081c25c2f05c4d`を
`cargo build -p bmz-player --locked`で再ビルドし、上記と同じWindows 11 / RTX 5090 / DPI 168の
実機で確認した。独立した検証データのバックグラウンドFPSを1に設定し、通常上限は60、
Windowed 800×600 / Fifo / frame latency 1とした。アプリの画面と標準タイトルバーを実際に操作し、
各状態の描画を確認した。起動時にCLIで指定したbackendと実際のadapter backendもログで照合した。

| 操作・確認 | Vulkan | DX12 |
|---|---|---|
| 起動後の標準「□」最大化 | プロセス開始約18秒後、成功 | 約11秒後、成功 |
| 最大化から元のサイズへ復元 | 3840×2036 → 800×600で描画継続 | 同左 |
| 表示安定後の2回目の標準「□」最大化 | 描画継続 | 描画継続 |
| 最小化・復帰（最大化後とリサイズ後の計2回） | 描画再開 | 描画再開 |
| 右辺ドラッグによる2回の連続リサイズ | 800×600 → 611×600 → 523×600 | 同左 |
| 最小化中のCPU更新 | ログで1FPS（約1秒間隔）を確認 | 同左 |
| 復帰後の上限とCPU更新 | 60FPSへ復帰 | 同左 |
| 閉じるボタンによる終了 | 保存・終了処理完了、`status="success"` | 同左 |

最小化はタイトルバーのボタンをクリックし、復帰は対象ウィンドウの再アクティブ化で行った。
両backendとも操作後のウィンドウが終了したことまで確認した。セッションログにconfigure失敗、
SurfaceLost／DeviceLost、panic、ERRORレベルの出力はなかった。再設定成功ログは各10件。
スキンのINFOレベルの`m-select error: nil`出力は従来どおりあり、Surfaceエラーとは区別した。

ログは同じ検証ディレクトリの`manual-review-build.log`、
`manual-review-vulkan-stderr.log`、`manual-review-dx12-stderr.log`へ保存した。
今回の追記は検証記録のみで、Rustコードは変更していない。前項のworkspace検証に加えて
対象コミットを再ビルドして実機操作を行い、文書差分を確認した。

この実機では最初のSurface再設定失敗を再現していない。故障後の復旧制御は前項の失敗注入テストで
確認した結果であり、実機で故障から復旧したと扱わない。初回表示前の最大化、故障時の
ネイティブ通知、他GPU／ドライバ／DPI／OSは未検証。報告者の環境と最初の失敗原因は未確定のまま。
