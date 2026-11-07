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

## #6 音声デコードの上限

### 上限を決める前の実データ計測

ローカルのlibrary DBを読み取り専用で参照して譜面を選び、FFmpeg CLIで各音源をf32 packedへデコードした。出力はパイプから数えて破棄し、PCM全体をメモリへ保持しない。全WAV宣言のファイルを重複排除して計測したため、Randomの値は選択されない分岐も含む保守的な合計である。全対象で欠損・デコード失敗は0件だった。

| 譜面 | 一意な音源 | 全体のbytes | 最大1ファイルのbytes | 最大レート・チャンネル |
|---|---:|---:|---:|---|
| 千秒女王（約1,000秒の長い譜面） | 927 | 407,061,368 | 4,153,344 | 44,100 Hz・2ch |
| Random [DP ANOTHER]（WAV宣言6,427件） | 1,645 | 1,448,320,016 | 20,526,544 | 44,100 Hz・2ch |
| STRONGER（約168秒の音源8本を共有するBMSON） | 8 | 516,787,904 | 64,598,488 | 48,000 Hz・2ch |

- 上限は目安どおり、8ch、384,000 Hz、20分、1ファイル512 MiB、譜面全体3 GiBを採用する。計測した全体の最大は3 GiBの約45%、1ファイルの最大は512 MiBの約12%に収まった。
- NOT VERIFIED — 高サンプルレートの実譜面。INSANE rootのWAVヘッダー20,057件を調べた範囲では48,000 Hzを超える音源を見つけられず、今回計測した譜面は最大48,000 Hzだった。
- 元データとDBの更新は行っていない。計測用コード、生ログ、個別パスを含むJSONはGit管理外の `.local/validation/security-medium/measure_audio.py`、`6-measure.log`、`6-audio-measurements.json` に保存した。

### 実装

- 専用WAVとFFmpegの両方でフォーマット・長さ・f32サイズを検査する。FFmpegは各フレームの追加前、WAVは確保前に検査し、WAV変換も4,096サンプルごとに共有キャンセルを確認する。
- durationから求めた確保量が1ファイル上限以内なら先に予約する。上限を超えるdurationヒントでは先行確保せず、実際のデコード中の検査で止める。
- 全ワーカー・代替候補・キャッシュが共有するatomicな譜面予算を追加した。確保容量とリサンプル中の入力・出力を含めて計上し、失敗したファイルの予約を解放する。cache hitも予算に含め、共有regionは従来の一意なdecodeで重複を避ける。
- 譜面全体の超過は `Result` で通常ロード・音声再ロード・自動試聴へ伝える。途中登録したsample bankは元へ戻し、不完全な譜面を開始しない。
- 長い無音WAVの小さい上限、上限ちょうどと1サンプル超過、実WAVの並列中断、cache hit、途中登録の復旧、リサンプルの過大確保防止と空音源の保持を回帰テストに追加した。
- [Play前のロードの仕様](../../docs/gameplay-runtime.md#play前のロード)に反映した。
- 上限検査より前の入力確保も抑えるため、16 MiBを超えるWAVはFFmpegへ回して逐次処理する。専用WAVの入力読込自体も528 MiB（512 MiBとヘッダー等の余裕16 MiB）までに制限する。対応するPCM形式は格納サイズがデコード後f32以下なので、既定の512 MiB以内のPCMと通常のメタデータを収められる。大きい正常なWAVをFFmpegで全サンプル読み込めることと、入力読込の境界もテストに加えた。

### 検証

- `cargo fmt --check`、`cargo check --workspace --all-targets --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings` は成功。
- `BMZ_REQUIRE_MZ_SELECT=1` で `cargo test --workspace --locked --no-fail-fast` は全target成功。audioは188成功・3 ignored、playerは2,918成功・58 ignored（起動テスト2件を含む）。すべて0失敗。音源境界・途中キャンセル・復旧のテストを含む。
- `bun run test:ir` と `bun x vue-tsc --noEmit` は成功。Prettierの対象となるWebファイルの変更はない。
- 初回の全体検証は `6-rust.log`、WAV入力確保の追加対策後の最終検証は `6-final-rust.log`、Webは `6-web.log` に保存した（いずれもGit管理外）。
- NOT VERIFIED — 実音声デバイスでの再生、高サンプルレートの実譜面。境界は合成WAVと純粋関数のテストで確認した。

## #18の保留と退避

- ユーザーの指示により、接続先検査の修正方針を再検討するため#18を飛ばした。コード・テスト・docs・この記録の作業中の節を含む7ファイルを、未追跡ファイルも含めてstashへ退避した。
- stashのcommitは `8ba6cf408c20232fcabd9f54c07f53e828f1c33e`、メッセージは `wip security #18 download destination policy deferred`。退避後にmainの作業ツリーがクリーンであることを確認した。
- システムプロキシの自動利用を無効化する変更は加えていない。#18は未完了で、次の#12へ進む。

## #12 7z圧縮ヘッダーの事前検査

- 現在の `sevenz-rust2 0.22.2` の公開APIを確認した。圧縮ヘッダーのdecoderは内部の無制限メモリ設定を使い、呼び出し側から上限を渡すAPIがないため、`ArchiveReader::new` より前に検査を追加した。依存関係の追加や更新はしていない。
- [7-Zip公式の形式](https://github.com/ip7z/7zip/blob/main/DOC/7zFormat.txt)と[method ID](https://github.com/ip7z/7zip/blob/main/DOC/Methods.txt)を参照し、圧縮ヘッダーのStreamsInfoに必要な範囲をパースする。可変長整数は最大8バイトの追加分をu64へ読み、件数・長さ・加算・位置を検査する。
- 辞書の合計は既存の256 MiB、展開後の各段は64 MiB（設定のheader_bytesが小さければその値）までとする。LZMA / LZMA2 / Copyと単一入力・出力のBCJ系を扱い、未対応codec、壊れた構造、重複・循環・不連続のbindingを拒否する。
- Copy等を連鎖させたdecoderの過大な入れ子も防ぐため、圧縮ヘッダーの1 folderのcoder数は32までとした。通常はcompressor 1段と任意のfilter程度であり、7-Zipが生成した検証書庫は1段だった。
- 開始ヘッダーのCRC・位置と次ヘッダーのCRCも事前検査する。開始部分が壊れていると、ライブラリが別の圧縮ヘッダーを検索して検査を迂回するため、この復旧経路は拒否する。通常の非圧縮ヘッダーは既存の処理へ渡す。
- 7-Zip 26.05で合成データだけの書庫を生成し、ヘッダー圧縮あり・なしを固定fixtureに追加した。大きな辞書と展開後サイズへ改変したfixtureはCRCを再計算し、圧縮データを読む前に拒否されることを検査する。
- [書庫の制限](../../docs/archive-songs.md#対応範囲と制限)とfixtureの再生成手順を更新した。

### 検証

- 初回の新規テスト1件は、循環bindingを作る改変位置の誤りで失敗した。テストデータの位置を訂正し、書庫関連27テストはすべて成功した。
- `cargo fmt --check`、`cargo check -p bmz-player --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings` は成功。
- `cargo test -p bmz-player --locked` は2,922成功・58 ignored（起動テスト2件を含む）、0失敗。`bun run test:ir` と `bun x vue-tsc --noEmit` も成功。Prettierの対象となるWebファイルの変更はない。
- ログはGit管理外の `.local/validation/security-medium/12-focused.log`、`12-check.log`、`12-rust.log`、`12-web.log` に保存した。
- NOT VERIFIED — macOS/Linuxでの実行。7-Zip実行ファイルによる生成とRustでの読込・スキャン・展開はWindowsで確認した。

## #18 譜面ダウンロードの接続先検査（初回案）

- HTTP APIとIPFS APIで共有するDNS resolverを追加した。公開先から始まる取得では、loopback、RFC 1918、link-local、CGNAT、未指定アドレス、multicast等への名前解決を拒否する。IPv6のULA・link-localとIPv4-mapped IPv6にも同じ判定を適用する。
- 設定された最初の接続先がローカルアドレスだけを返す場合は、ローカルAPIとして利用を許可する。公開・ローカルを混在させたDNS応答は拒否し、最初の応答は実接続にも再利用して再解決によるすり替えを防ぐ。
- IPリテラルはリクエスト前とリダイレクト時に検査する。API応答のURLも検査し、公開先同士のリダイレクトは従来の10回上限で維持する。
- 拒否ログはhostだけに限定し、リクエストエラーからURLを除いてAPIキー等がエラー表示に含まれないようにした。
- 6つの回帰テストは成功。ローカルを指すDNS・IP・mapped IPv6について、リダイレクトとAPI応答のどちらでも拒否先へのTCP接続が発生しないことを確認した。公開先同士のリダイレクトと、明示的な127.0.0.1のAPIによるダウンロードも成功した。公開DNSの判定とループバックのテスト用transportを分離して確認している。
- ログはGit管理外の `.local/validation/security-medium/18-focused.log`、`18-rust.log`、`18-web.log` に保存する。
- 初回案の未解決事項: 現行reqwestはシステムプロキシを自動利用する。プロキシ経由では接続先のDNSをプロキシが解決し、クライアントのresolverを迂回できる。譜面ダウンロードに限ってプロキシを無効化する案を確認したところ、ユーザーが方針を再検討するため保留した。本体のプロキシ設定は変更しなかった。
- 現在の差分で `cargo fmt --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings` は成功。`cargo test -p bmz-player --locked` は2,924成功・58 ignored（起動テスト2件を含む）、0失敗だった。
- `bun run test:ir` と `bun x vue-tsc --noEmit` は成功。Prettierの対象となるWebファイルの変更はない。
- NOT VERIFIED — 実際の公開配布サーバーからの取得、OSのシステムプロキシ連携。新しい接続先検査はDNSとtransportを差し替えたWindowsのローカルテストで確認した。

### ユーザー指定の方針で再開

- #12の検証・コミット後、stashを保持したまま復元した。共通notesの競合は、#12と#18の両方の記録を残して解決した。
- ユーザーの指示により、システムプロキシの自動利用は維持する。`Policy::none()` と自前の最大10回のredirectループを使い、各hopの前にURLのIPリテラル、ローカル用host名、DNS応答を非同期で検査する。直接接続時のカスタムDNS resolverの検査も維持する。
- プロキシ側の名前解決がクライアントと異なるsplit-horizon DNS等では検査をすり抜けうる。この点は残余リスクとして `docs/song-sources.md` にも記載する。
- 最初の設定先がローカルアドレスだけを返す場合の例外を維持する。公開先から始まる取得では、localhost・そのサブドメイン・`.local`を末尾ドット付きも含めて拒否し、それ以外は `tokio::net::lookup_host` を使うSystemResolverで毎hop事前確認する。
- 直接接続時は事前確認済みのtarget hostに対し、接続時のDNS結果にも検査を適用する。明示的に設定されたプロキシ自体の名前解決は区別し、ローカルのプロキシも利用できるようにした。
- 301/302/303のPOST→GET、307/308のPOST保持、同originのBasic認証、origin移動時の認証除去、Refererの扱いとリクエスト全体のtimeoutを維持する。IPFSの405に対するPOST再試行も既存経路のまま使う。
- プロキシの環境変数を子プロセス内だけに設定する回帰テストを追加した。プロキシ経由の正常取得と、ローカル宛てのredirect・API応答で2回目の通信が送られないことを検査する。テストのtransportもloopbackへ固定し、誤って直接接続する実装でも外部へは接続しない。
- ダウンロード関連67テストはすべて成功した。DNS rebinding、末尾ドット付きのローカル用host名、10回ちょうどと11回目のredirect、POSTのメソッド変換、Basic認証とReferer、IPFSの既存POST再試行も含む。

### 再開後の検証

- `cargo fmt --check`、`cargo check -p bmz-player --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings` は成功。`cargo test -p bmz-player --locked` は2,934成功・58 ignored（起動テスト2件を含む）、0失敗だった。
- `bun run test:ir` と `bun x vue-tsc --noEmit` は成功。Prettierの対象となるWebファイルの変更はない。
- ログはGit管理外の `.local/validation/security-medium/18-revised-focused.log`、`18-revised-rust.log`、`18-revised-web.log` に保存した。元のstashも保持している。
- NOT VERIFIED — 実際の外部DNS・配布サーバー、Windows設定画面で指定したプロキシ、macOS/Linuxでの実行。Windows上の環境変数によるプロキシ自動利用は、子プロセスとモックプロキシで検証した。

## #8 IR入力の上限

- 通常の要求本文を256 KiB、リプレイのPUTだけを従来の4 MiBに分けた。Worker entryとNode/Nitroのmiddlewareで共通の経路判定を使い、Content-Lengthの有無・申告値によらずストリームを制限する。handlerが読む前に413とするため、スコア検証の400への変換に巻き込まれない。
- Node側は既存のリプレイ用のbackpressure/cancel処理を共用し、検査後の本文をH3のcacheへ保持する。JSONのreadBodyとリプレイの再読込も回帰テストで確認した。
- metadata各1,024文字、URL各2,048文字、subartists 32件・各1,024文字、コース64譜面、evidence 16 KiBを名前付き定数で制限する。evidenceは後からの署名にも適用する。
- `payload.rs`、`course_payload.rs`、`backfill.rs`と`device_key.rs`を確認し、play_optionsのキーは削除しない。通常の64曲分のcourseオプションは9,984 bytesになり4 KiBでは拒否されるため、play_optionsは32 KiBへ調整した。実譜面の分岐指定も確認し、前節のRandom [DP ANOTHER]はRANDOM/SETRANDOM 28件、SWITCH 0件、両配列のJSONは保守的な値で119 bytesだった。他の計測済み2譜面は分岐なし。署名evidenceは現在のコース形式で288 bytesとなり、16 KiB以内に余裕を持って収まる。
- 計測はクライアントの現在のキー・型と最大seedを使うJSONで行った。実譜面は非選択分岐も含めて数えた。計測スクリプトはGit管理外の `.local/validation/security-medium/measure_ir_input.py`。PythonのWindowsApps aliasが使用できなかったため、`py` launcherで実行した。
- 表示名は登録・変更とも共通のrequireDisplayNameでtrim後1〜64文字・制御文字なしとする。新規APIテストの初回はNuxtのreadBody自動importがBun単体では存在せず失敗したため、既に変更対象のprofile handlerでH3関数を明示importして解消した。
- 実スコア・コースAPIの各フィールドの上限ちょうど／1超過（400）、通常の64曲コース、Node/Workerの本文境界（413と4 MiBの受理）、登録・変更APIの表示名を、loopback HTTPとメモリ内SQLで検査した。
- [入力の上限](../../docs/ir.md#入力の上限)に反映した。Cloudflareの[ストリーム処理の推奨](https://developers.cloudflare.com/workers/best-practices/workers-best-practices/#stream-request-and-response-bodies)と現行のWorkers型定義も確認した。本番へのアクセス・デプロイは行っていない。

### 検証

- `bun run test:ir`、`bun x vue-tsc --noEmit`、変更したWeb 16ファイルのPrettier確認はすべて成功。現在のWorkers型定義は `@cloudflare/workers-types 5.20261107.1` をGit管理外へ取得して参照した。
- `cargo fmt --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings` は成功。`cargo test -p bmz-player --locked` は2,934成功・58 ignored（起動テスト2件を含む）、0失敗。
- ログはGit管理外の `.local/validation/security-medium/8-submission.log`、`8-body.log`、`8-typecheck.log`、`8-web.log`、`8-rust.log` に保存した。
- NOT VERIFIED — Cloudflare実環境とNuxt開発サーバー本体での手動操作。Node HTTPアダプターとWorker entryのモックは自動テストで確認し、本番環境は確認・変更していない。

## #11 device keyと未検証リプレイの清掃

- device key登録APIへ既存のcheckUserRateLimitを追加した。鍵の作成・通常の再試行を許容する15分あたりユーザー30回・IP 120回とし、超過は429を返す。有効な鍵はアカウント10件まで。同じ有効な鍵の再登録は既存IDを返し、上限の超過は古い鍵の失効を促す409とする。
- 件数・重複の条件とinsertを1回のSQLにまとめた。10件目の競合でも上限を超えず、同じ鍵の同時登録でも1件だけ追加する。
- 90日を過ぎた失効鍵を削除できるか、schema、署名検証、score詳細・ランキングの経路を確認した。scoreから鍵へのFKはなく、新しい署名は失効鍵を既に拒否し、履歴表示は保存済みのverificationを参照する。実際のEd25519署名で保存したscoreの鍵を失効・削除し、詳細APIとbestのverified_playが維持されることも確認した。
- verifyでrejectedにしたblobは直ちに削除し、DB行のstatus・hash等を残す。削除失敗時はobject_pathを残して定期清掃で再試行する。参照解除はid・path・rejected状態を条件にし、新しい世代を消さない。
- 複数blobの削除はallSettledで全件を待ち、成功した行だけ参照を解除する。1件のストレージ障害が他の清掃対象を止めないことと、その後の再試行をテストに追加した。
- `ir:storage-cleanup`を毎分のscheduledTasksへ登録した。24時間を過ぎたpending_upload・uploadedを、状態とupdated_atを条件に1回のSQLでrejectedにする。verifiedは対象外。検証の更新にも許可状態の条件を追加し、清掃と競合した古い検証が状態を復活させないようにした。
- 各段は40件ずつに制限し、残りを次回へ回す。参照解除のSQLもD1のbind上限100以内にし、テストのSQL loggerですべてのqueryを検査した。既存のtask lockで同時実行を防ぎ、検索用の4索引をschemaと `0016_storage_cleanup_indexes.sql` に追加した。migrationの実行はメモリ内のテストDBだけで、本番D1・R2へはアクセスしていない。
- 初回の型検査では新しいrate actionのschema enumへの追加漏れと、hub:dbの生成済みschema型が古いことを検出した。sourceへenumを追加し、RateLimitActionをschemaから導出し、`bun x nuxt prepare`でローカルの型だけを再生成して解消した。
- 回帰テストでは、11件目の409、同時登録、ユーザーとIPそれぞれの429、24時間・90日の境界、40件の上限、ロック、失効鍵削除後の署名状態、rejected blobの削除と再試行、清掃と検証・PUT・新しい世代の競合を確認した。
- [リプレイと署名の仕様](../../docs/ir.md)へ反映した。scheduledTasksは[Nitroの公式説明](https://nitro.build/docs/tasks#scheduled-tasks)、D1の上限は[公式のlimits](https://developers.cloudflare.com/d1/platform/limits/)、blob削除は現在のNuxtHub実装・型と[R2のAPI](https://developers.cloudflare.com/r2/api/workers/workers-api-reference/)を確認した。

### 検証

- 最終版の `bun run test:ir` はすべて成功。リプレイ競合9テスト、storage maintenance 6テストを含む。`bun x vue-tsc --noEmit`、変更したWeb 10ファイルのPrettier確認も成功した。索引のSQLはメモリ内DBのmigration実行で検証した。
- `cargo fmt --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings` は成功。`cargo test -p bmz-player --locked` は2,934成功・58 ignored（起動テスト2件を含む）、0失敗。
- ログはGit管理外の `.local/validation/security-medium/11-maintenance.log`、`11-replay.log`、`11-nuxt-prepare.log`、`11-web.log`、`11-final-web.log`、`11-rust.log` に保存した。最終Webログは部分削除失敗への対策追加後のもの。
- NOT VERIFIED — 実際のCloudflare scheduled実行、D1・R2連携。ローカルHTTP、メモリ内SQL、blobモックで確認し、本番へのアクセス・migration適用・デプロイは実施していない。
