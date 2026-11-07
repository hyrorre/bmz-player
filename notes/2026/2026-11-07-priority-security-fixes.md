# Codex Securityの優先指摘への対応

基準は `main` の `85dc3346`。Windows（`x86_64-pc-windows-msvc`）で検証した。

## 前提となるスキンテストの修正

- #2 の初回検証で失敗した13件を `85dc3346` のソースから再ビルドし、同じWindows環境・スキン素材で `cargo test -p bmz-player --locked --lib -- --exact` により再実行した。13件すべてが初回と同じ箇所・内容で失敗した。
- LR2の12件は実際のアプリと同じ許可ルートをテストに渡し、テーマの別シーンやフォントへの参照を解決できるようにした。外部beatoraja JSONの1件は、設定からの未承認読み込みが拒否され、明示選択した場合に読み込めることを確認する。
- 初回の比較では `data/skins` のジャンクションが失敗箇所に影響したため、対象LR2スキンの実体コピーで比較し直した。生ログはローカルの `.local/validation/security-85dc3346/baseline-real-assets.log` に保存した。
- 修正した13件は実素材を使ってすべて成功した。テスト修正と #2 の作業差分を含む状態で `cargo fmt --check`、`cargo check -p bmz-player --locked`、workspace Clippy、`cargo test -p bmz-player --locked` が成功した。全体は2,906成功、0失敗、58 ignored（起動テスト2件を含む）。

## #2 Windowsで曲テキストやURLを開く処理

- `ShellExecuteW` の `open` に、NUL終端のUTF-16でファイル名やURLを渡す。ファイルパスはExplorer用の既存の変換を共用し、`\\?\` と `\\?\UNC\` のプレフィックスを通常の形式に戻す。
- 書庫の曲テキストは既存のワーカーで開く。通常フォルダの曲テキストとURLもWindowsではワーカーで開き、結果をウィンドウのイベントループへ返す。呼び出し元スレッドでSTAを初期化し、APIの戻り値が32以下ならエラーとして扱う。
- UTF-16変換のテストで、`&`、`^`、`%`、空白、日本語、拡張パスとUNCパスを確認する。
- 操作仕様は [操作ガイド](../../docs/controls.md#f3と曲テキスト) を参照。
- STAの初期化とエラー判定は [Microsoftの `ShellExecuteW` 仕様](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shellexecutew) に従う。

### 検証

- `cargo fmt --check`、`cargo check -p bmz-player --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings` は成功。
- 初回の `cargo test -p bmz-player --locked` は2,893成功、13失敗、56 ignored。追加したUTF-16変換テストと既存のExplorer用パス変換テストは成功した。失敗13件の再現と修正は上の節に記録した。
- `bun run test:ir` は成功。HTTPテストにはループバック通信を許可した。
- `bun x vue-tsc --noEmit` は、ローカルで `nuxt prepare` を実行して生成型を更新した後に成功した。Prettierの対象となるWebファイルの変更はない。
- NOT VERIFIED — Windowsで関連付けられたアプリが実際に起動すること。

## #17 BMSONの小節数上限

- 明示された `lines` の件数と、最終pulseを標準小節長で割った位置を検査する。さらに明示小節と自動補完分の合計を計算し、10,000小節を超える場合は境界の補完前に譜面の読み込みエラーを返す。
- 最大pulseはロングノートの終端も含めて飽和演算で求める。解像度が0の場合は補完しない。
- 上限直前のノーツの読み込み、`u64::MAX / 2` のpulseの拒否、明示小節線と補完の合計、解像度0をテストする。曲スキャンの回帰テストは、上限超過のBMSONを失敗として記録し、同じフォルダのBMSを取り込むことを確認する。
- スキャン時の扱いは [曲の取得元とスキャン](../../docs/song-sources.md) に記載した。

### 検証

- `cargo fmt --check`、`cargo check --workspace --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings` は成功。
- `cargo test -p bmz-chart -p bmz-player --locked` は `bmz-chart` が189成功・2 ignored、`bmz-player` が2,907成功・58 ignored（起動テスト2件を含む）。失敗は0件。
- `bun run test:ir` と `bun x vue-tsc --noEmit` は成功。Prettierの対象となるWebファイルの変更はない。

## #9 IR認証情報と接続先origin

- scheme、host、portから求めたoriginをログイン時とrefresh時に保存する。設定と一致する認証情報だけを同期・設定UI・CLIで使い、ログアウトも一致する認証情報だけを削除する。
- 組み込みキーを既定外のoriginへ割り当てるログインと、有効な他の設定行とのキー衝突を拒否する。大文字小文字が異なるキーもファイル名が衝突する環境を考慮する。GUIではログインの応答を反映する時点の設定を検査する。
- originのない旧データは、組み込みキーと既定originが一致する場合に限って保存し直す。カスタムIRの旧データは再ログインまで使わない。
- rianIR互換の応答にキーがないため、カスタムoriginではクライアントがoriginごとのキーを生成する。ファイル名とキーチェーンの命名形式は従来どおり。旧キーの送信ジョブは移さない。
- IR HTTPクライアントは別originへのリダイレクトを拒否する。Bearer tokenを付けるリプレイのアップロード先URLもoriginを照合する。
- 移行と再ログインの条件は [IR仕様](../../docs/ir.md) に記載した。

### 検証

- `cargo fmt --check`、`cargo check -p bmz-player --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings` は成功。
- `cargo test -p bmz-player --locked` は2,913成功、0失敗、58 ignored（起動テスト2件を含む）。ポート・schemeの違い、組み込みキーの拒否、旧データの移行、カスタムrianIRのキーを確認した。ローカルHTTPモックで、公式キーの上書き拒否とrefreshが別originへ転送されないことを確認した。
- `bun run test:ir` と `bun x vue-tsc --noEmit` は成功。Prettierの対象となるWebファイルの変更はない。
- NOT VERIFIED — Windows Credential Manager、macOS Keychain、Linux Secret Serviceとの実機連携と実IRへのログイン。

## #4 スコア送信で共有メタデータを保護

- 譜面・コースは初回に登録する。既存行は `NULL` または空文字だけを補完し、非空のメタデータ、数値・フラグの `0` / `false`、登録済みの更新時刻を保持する。既存の `local_backfill` とDPのBATTLE系の空欄補完は行わない。
- `hasRandom` は既存値と送信値のORを取る。保存後も `false` の譜面で、送信された数値やフラグが保存値と異なる場合だけ、譜面ハッシュと項目名をログに出す。スコアの整合性検査は各送信の `result.notes` を基準とする。
- 各upsertは1文。`excluded` を使いINSERTの値を再利用する。ローカルSQLiteのテストに [D1のバインド上限](https://developers.cloudflare.com/d1/platform/limits/) と同じ100個の制約を適用した。
- 現行の `course_hash` は譜面の順序に加えて制約も検証する。難易度表同期は別テーブルを更新するため、同期の処理は変更していない。
- 送信時の扱いは [IR仕様](../../docs/ir.md) に記載した。

### 検証

- `bun run test:ir` は201成功、0失敗。追加の7件で別のプレイヤーからの上書き、空欄補完、BATTLEと取り込みの空欄保持、RANDOM分岐とフラグ保持、コース情報の保護を確認した。
- `bun x vue-tsc --noEmit` と変更したTypeScript 6ファイルのPrettierは成功。
- 追加のESLintでは `course_ir.ts`、`ir.test.ts`、`ir.ts` に7件の指摘が出た。直前のコミット `d397fd2b` の同ファイルでもすべて再現した。`metadata.ts`、`submission.ts`、`submission.test.ts` のESLintは成功。
- `cargo fmt --check` と `cargo clippy --workspace --all-targets --locked -- -D warnings` は成功。最初の `cargo test -p bmz-player --locked` ではViewer IPCテスト1件が、名前付きパイプでACKを受信する前に接続が閉じられて失敗した（Windows error 233）。同テストは #17 と #9 の検証で成功しており、#4 にRustコードの差分はない。
- 同じ条件で `cargo test -p bmz-player --locked` を再実行し、Viewer IPCを含む2,913件が成功した。失敗0件、58 ignored（起動テスト2件を含む）。初回と再実行のログはローカルの `.local/validation/security-85dc3346/4-rust-test.log` と `4-rust-retry.log` に保存した。
- NOT VERIFIED — 本番D1での実行。検証はローカルのみで、本番データへのアクセスやデプロイは行っていない。
