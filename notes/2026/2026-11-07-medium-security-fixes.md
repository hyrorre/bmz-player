# Codex Securityの優先度中の指摘への対応

基準は `main` の `9ffbfbda`。Windows（`x86_64-pc-windows-msvc`）で検証した。

## #13・#21 IRのリプレイ取得先とサイズ上限

- IRが返したダウンロードURLのoriginを設定されたIRのoriginと照合し、別originなら接続前に拒否する。
- CLIとG-BATTLEは、サーバーの `MAX_REPLAY_BYTES` と同じ4 MiBを共通の上限とする。`Content-Length` が分かる場合は事前に検査し、本文は受信中に検査する。
- ローカルHTTPのテストで、別ポートへの接続が発生しないこと、上限超過の宣言長とchunked本文を拒否すること、上限ちょうどの本文を受理することを確認する。
- [IRリプレイの仕様](../../docs/ir.md#リプレイ)に反映した。

### 検証

- `cargo fmt --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings` は成功。
- `cargo test -p bmz-player --locked` は2,916成功、0失敗、58 ignored（起動テスト2件を含む）。
- `bun run test:ir` と `bun x vue-tsc --noEmit` は成功。Prettierの対象となるWebファイルの変更はない。
- NOT VERIFIED — 実IRからのダウンロード。HTTPの検証はローカルの模擬サーバーを使用した。

## #7 OBSの録画削除の制限

- 接続ごとにBMZが送信した `StartRecord`、成功応答、`STARTED` イベントを追跡する。再接続した場合や手動開始した録画は削除対象にしない。
- 接続時と録画開始時に `GetRecordDirectory` を要求する。停止時に記録したフォルダとファイルの実体パスを削除時に照合し、フォルダ配下の動画の通常ファイルだけを削除する。取得失敗、対象外の拡張子、リンクやフォルダ外のパスは警告を記録して残す。
- Keep All、保存要求後の保持、次回録画開始時の削除、リトライ時の削除の条件を維持する。
- モックOBSで、正当な削除、フォルダ外、`..`、動画以外、手動録画、フォルダ取得失敗、Keep Allを確認した。既存の保存モードとリトライのテストも所有権と録画フォルダを設定して検証した。
- [OBSの仕様](../../docs/obs.md#自動録画)に反映した。

### 検証

- OBS関連のテストは41成功、0失敗。
- `cargo fmt --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings` は成功。
- `cargo test -p bmz-player --locked` は2,917成功、0失敗、58 ignored（起動テスト2件を含む）。
- `bun run test:ir` と `bun x vue-tsc --noEmit` は成功。Prettierの対象となるWebファイルの変更はない。
- NOT VERIFIED — OBS実機との接続。Linux専用のシンボリックリンク回帰テストは追加したが、このWindowsでは未実行。WSLのUbuntu、Ubuntu-26.04、FedoraLinux-44はいずれも仮想ディスクが見つからず起動できなかった。

## #3再開前のLR2テスト9件の修正

- #3の未コミット差分を `.local/validation/security-medium/3-pending.patch` に退避し、作業ツリーが `206eb050` と一致する状態で、同じWindows・外部スキン・`BMZ_REQUIRE_MZ_SELECT=1` により `cargo test -p bmz-skin --lib --locked lr2::tests::` を実行した。107成功、9失敗で、#3修正後の失敗9件とテスト名と内容が一致した。
- LITONE5、LR2、WMIX_HD、WMII_FHD_LR2を読むテストが、明示的なスキン選択用の入口フォルダだけをrootとするAPIを使っていた。パッケージ基準のincludeを読むため、既存のpath-context付きAPIにテスト用の `data/skins` を明示して渡す。runtime optionと検証内容は維持する。
- 修正後、同じLR2テストは116成功、0失敗。元の9件がすべて実ファイルの読み込みと既存のassertを通ることを確認した。
- 再現ログはGit管理外の `.local/validation/security-medium/3-baseline-206eb050.log`、修正後は `lr2-test-fix-focused.log` に保存した。
- `cargo fmt --check`、`cargo check -p bmz-skin --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings` は成功。`BMZ_REQUIRE_MZ_SELECT=1` で `cargo test -p bmz-skin --locked` は411成功、0失敗、2 ignored。
- `bun run test:ir` と `bun x vue-tsc --noEmit` は成功。Prettierの対象となるWebファイルの変更はない。検証ログは `lr2-test-fix-rust.log` と `lr2-test-fix-web.log` に保存した。

## #3 JSONスキンのinclude展開予算

- 展開全体で共有する予算を追加した。元の文書を含む4,096ファイルの読み込みと、展開後の1,000,000ノードを上限にし、重複includeの読み込みも数える。
- 配列のincludeを平坦化するときは、消える配列のラッパーをノード数に含めず、保持される配列・オブジェクト・スカラーをそれぞれ1と数える。既存の循環検出と深さ64の制限は維持する。
- 30段の二重includeによるDAGを有限の読み込み回数で拒否するテスト、ファイル数の境界、100万ノードちょうどと1ノード超過のテストを追加した。同じファイルを2回includeする既存テストも成功した。
- 初回の `bmz-skin-document` の31テストは1.66秒で成功した。利用側では `bmz-player` の2,917テストと `bmz-render` の1,002テストが成功し、`bmz-skin` のLR2テスト9件が失敗したため一度中断した。上の節のとおり `206eb050` で再現し、テスト修正を先にコミットした。
- 初回のログはGit管理外の `.local/validation/security-medium/3-focused.log`、`3-rust.log`、`3-web.log`、再開後は `3-resumed-rust.log` と `3-resumed-web.log` に保存する。
- [JSON includeの仕様](../../docs/skin.md#jsonのinclude展開)に反映した。

### 再開後の検証

- `cargo fmt --check`、`cargo check -p bmz-skin-document --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings` は成功。
- `BMZ_REQUIRE_MZ_SELECT=1` で関連crateの `cargo test --locked` は成功。`bmz-skin-document` は31成功、`bmz-skin` は411成功・2 ignored、`bmz-render` は1,002成功・11 ignored、`bmz-player` は2,917成功・58 ignored（起動テスト2件を含む）。すべて0失敗。
- 同梱default、mz-select、Rmz-skin、Luxez-Flatの実ファイルを使う読み込みテストも成功した。3つのsubmoduleが初期化済みであることを確認した。
- `bun run test:ir` と `bun x vue-tsc --noEmit` は成功。Prettierの対象となるWebファイルの変更はない。

## #5再開前のViewer IPC修正

- テスト失敗で停止せず、原因調査と修正を行って継続する方針へ変更した。
- WindowsのlistenerがACK書き込み直後に `DisconnectNamedPipe` を呼び、クライアントが読む前のACKを破棄していた。読み取りを20 ms遅らせる回帰テストを追加し、#5を退避した `32e7c69f` 上でもWindows error 233を再現した。
- 切断前に `FlushFileBuffers` でACKが読み取られるまで待ち、同じパイプを次の接続へ再利用する。クライアントが切断した場合は警告を記録して次へ進む。
- [Microsoftの名前付きパイプ仕様](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-operations)に記載された切断前の処理に合わせた。`docs/viewer.md` のエンドポイント表記も実装と同じパイプv3・TCPポート39078へ訂正した。
- 再現ログはGit管理外の `.local/validation/security-medium/viewer-ipc-before.log`、修正後は `viewer-ipc-focused.log` に保存する。#5は `5-pending.patch` に退避して先にこの修正を検証する。
- 修正後のViewer IPC 3テストは成功。Cargo経由で50回繰り返し、150件すべて成功した（`viewer-ipc-stress.log`）。実行ファイルの直接起動はDLL検索設定が足りなかったため、検証にはCargoを使用した。
- `cargo fmt --check` と `cargo clippy --workspace --all-targets --locked -- -D warnings` は成功。`cargo test -p bmz-player --locked` は2,918成功、0失敗、58 ignored（起動テスト2件を含む）。
- `bun run test:ir` と `bun x vue-tsc --noEmit` は成功。Prettierの対象となるWebファイルの変更はない。ログは `viewer-ipc-rust.log`、`viewer-ipc-web.log` に保存した。

## #5 動画の解像度上限

- 幅・高さ各8,192 pixel、画素数16,588,800（3840×2160×2）を名前付き定数で制限する。ストリーム選択直後のパラメータと、channel・clocked両経路の各フレームで検査する。パラメータの0は未確定として扱い、RGBA変換時の0はエラーにする。
- ストリーム候補のbit rateはパラメータから直接取得する。上限検査より前に候補のdecoderを開く処理をなくした。
- RGBA scalerは入力の解像度またはpixel formatが変わると再作成する。再作成とRGBA確保の前にも上限を検査する。
- 超過時は既存のdecodeエラー経路で警告を出し、動画のデコードを中断する。譜面の読み込みとプレイは続く。
- 上限ちょうどと超過・不明値の純粋関数テスト、未確保フレームの拒否、解像度・pixel format変更によるscaler再作成を確認した。実データのY4M（8194×2、約24 KiB）でも、decoderを開く前の上限エラーを確認した。
- `cargo test -p bmz-video --locked` は40成功、0失敗。
- [動画の解像度の仕様](../../docs/chart-images.md#動画の解像度)に反映した。ログはGit管理外の `.local/validation/security-medium/5-focused.log`、`5-rust.log`、`5-web.log` に保存する。
- NOT VERIFIED — 実プレイでの動画BGA表示。自動テストはWindowsでFFmpegのデコードとフレーム変換を確認した。

### 初回の検証と中断

- `cargo fmt --check`、`cargo check -p bmz-video --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings` は成功。
- `cargo test -p bmz-video -p bmz-ffmpeg -p bmz-player --locked` は `bmz-player` のViewer IPCテスト1件で失敗した。2,916成功、1失敗、56 ignored。`viewer_ipc::tests::play_stop_and_quit_requests_round_trip_to_persistent_listener` が `viewer_ipc.rs:367` で、名前付きパイプのACKを受信する前に相手が閉じたためWindows error 233になった。
- 同じIPCテスト・同じエラーは[優先度高の作業記録](2026-11-07-priority-security-fixes.md)にも記録されている。今回の#3再検証では成功していた。#5を外した状態での今回の再現は未確認。
- `bmz-video` 単独の40テストは成功。一括実行はplayerの失敗で終了した。`bmz-ffmpeg` は0テストで成功。
- `bun run test:ir` と `bun x vue-tsc --noEmit` は成功。Prettierの対象となるWebファイルの変更はない。
- 初回は指定された停止条件に従って#5を未コミットで保持した。その後、Viewer IPCの原因を再現・修正して先にコミットし、#5の検証を再開した。

### 再開後の検証

- `cargo fmt --check`、`cargo check -p bmz-video --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings` は成功。
- `cargo test -p bmz-video -p bmz-ffmpeg -p bmz-player --locked` は成功。videoは40成功、playerは2,918成功・58 ignored（起動テスト2件を含む）、ffmpegは0テスト。すべて0失敗で、Viewer IPCの3テストも成功した。
- `bun run test:ir` と `bun x vue-tsc --noEmit` は成功。Prettierの対象となるWebファイルの変更はない。
- ログはGit管理外の `.local/validation/security-medium/5-resumed-rust.log` と `5-resumed-web.log` に保存した。
