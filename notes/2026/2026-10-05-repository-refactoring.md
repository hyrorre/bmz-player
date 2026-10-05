# リポジトリ横断リファクタリング

リポジトリ全体の調査で挙げた6件を、設定編集、プレイ先読み、background job、
描画API、IR送信、IR Web履歴表示の順に実装する記録。
動作契約は [skin.md](../../docs/skin.md)、[gameplay-runtime.md](../../docs/gameplay-runtime.md)、
[ir.md](../../docs/ir.md)、検証方針は [AGENTS.md](../../AGENTS.md) を参照する。

## 1. 設定編集

- 115項目のID・値の参照先・退避/復元・変更・表示を
  `config/settings_registry/entries.rs` の項目定義へ集約した。
- `SettingsEditSession` は型付きsnapshotだけを保持する。項目IDと保存値の型を別々に
  保持する方式と、組み合わせ不一致を無視する復元分岐を廃止した。
- SessionMode/auto_play、key-mode conversion/double option、HS preset等の連動する
  値の扱いは維持した。設定の保存形式・表示文字列・操作方法は変更していない。
- 全項目に対し、既定値と変更済みの値から正負・ゼロ・大きなdeltaで編集して
  キャンセルするテストを追加した。無関係なdisplay_nameの編集は維持する。
  8K方向設定が空sectionを実体化する従来の挙動に合わせ、比較fixtureはsectionを明示する。

検証（Windows）:

- `cargo fmt --check`、`cargo check -p bmz-player --locked`、
  `cargo clippy -p bmz-player --all-targets --locked -- -D warnings`。
- `cargo test -p bmz-player --locked --no-fail-fast`: 2,193成功、19 ignored。
  bin/doc testも成功。GPUプレビュー等のignored testと実機の設定画面操作は未実施。
- 制限環境では一時ファイル/localhostを利用するテストが失敗・待機したため停止し、
  通常権限で全体を再実行して成功した。途中の実行中binaryへの再リンク失敗も再実行で解消。
- ローカルログ: `.local/refactor-01-full.log`（Git管理外）。

## 2. プレイ先読み

- 通常開始・BGA再利用・同配置リトライのworkerを `app/play_preload.rs` へ集約した。
  読み込み元をImport/Cachedで明示し、進捗・結果通知・世代IDの処理を共有する。
- 同配置キャッシュは `PreparedPlayChart` を一式で保持し、複数の独立したOptionと
  復元時のexpectを廃止した。BGAの所有権移動と音声エンジンの再生成は維持する。
- 成功/失敗結果の世代IDとchart ID、保持済み譜面によるファイル削除後の再試行、
  音声sample rate・normalization gainの引き継ぎをテストした。

検証（Windows）: fmt、対象crateのcheck・all-targets Clippy・全testが成功。
`cargo test -p bmz-player --locked --no-fail-fast`: 2,195成功、19 ignored。
実機の演奏・リトライは未実施。ログ: `.local/refactor-02-full.log`。

## 3. Background job

- 1,384行の `background_jobs.rs` をSelect更新・インポート・曲スキャン・難易度表・
  アプリ更新の5モジュールへ分割した。処理の可視範囲はapp内に維持する。
- 曲スキャンの実行中・FIFO queue・進捗を `SongScanRuntime`、更新確認・download・
  handoff・dialogを `AppUpdateRuntime` に集約した。
- 更新確認の受信口と通知意図は `PendingUpdateCheck` に束ね、完了/切断時に一緒に
  消費する。中断からの再投入も共通のqueue処理を通し、保留中の手動要求を維持する。
- Empty/Paused/Disconnectedと手動・自動要求の併合をテストした。

検証（Windows）: fmt、対象crateのcheck・all-targets Clippy・全testが成功。
全testは2,197成功、19 ignored。実際の更新適用やmacOS Sparkle操作は未実施。
ログ: `.local/refactor-03-full.log`。

## 4. 描画API

- `SkinDocumentRenderExt` の89メソッドを、公開入口14個とskin内部評価75個に分離した。
  外部利用されている描画・hit test・レーン寸法取得の入口は維持する。
- `SkinDocumentRenderInternal` はskin内だけに公開し、内部のgraph/cache/lookup型を
  公開traitから除いた。trait/implの `allow(private_interfaces)` は不要になった。
- メソッド本体を変更せず、既存のscene別macroを公開/内部のimplへ振り分けた。
  テスト専用のjudge image helperは既存のtext helper同様にcfg(test)とした。

検証（Windows）: fmt、workspace全体のcheck・all-targets Clippy・全testが成功。
bmz-renderは679成功・4 ignored、bmz-playerは2,197成功・19 ignored。
Lua数値の準備順序・stateful callback・production cacheの既存テストを含む。
GPUのignored testと実機の描画確認は未実施。ログ: `.local/refactor-04-full.log`。

## 5. IR送信結果

- `sync/process.rs` はclaim・資格情報確認・通信・結果適用・待機の制御に絞った。
  通信結果は `SubmittedIrJob`、DB/ログ/集計への反映は `JobCompletion` で扱う。
- Replay/Attestation/Score/Courseの失敗処理を共有し、Retry-After、エラーの原因連鎖、
  logのrequest/responseを維持する。送信job間のthrottleは成功・失敗共通の一か所とした。
- provider/credentialsの事前検証失敗は従来通り送信せず次へ進む。
  確認済みtokenの利用、ランキングのattempt識別、score完了とreplay登録のtransaction、
  Replay成功時にscore submission logを作らない扱いも維持した。
- 全job種別の失敗時のDB・log・Retry-After、成功時の集計/ランキング識別、
  完了処理に失敗した場合のremote response保持を追加テストした。

検証（Windows）: fmt、対象crateのcheck・all-targets Clippy・全testが成功。
全testは2,200成功、19 ignored。実サーバーへのスコア送信は未実施。
ログ: `.local/refactor-05-full.log`。

## 6. IR Web履歴表示

- 譜面・コースの履歴open/page/limit/query/watchを `useSelfScoreHistory` へ集約した。
  `useFetch` はwrapper内でawaitせず、呼び出し元のscopeにwatcherを登録する。
- 共通の `ScoreHistoryModal` がloading/error/empty・table・paginationを扱う。
  IDとBPの取得、譜面のスコア詳細リンクとARRANGE表示は型付きのprops/slotで渡す。
  コースのgauge絞り込み、譜面のself scope、50件単位、日付fallbackは維持した。
  共通モーダル内の色はNuxt UIのテーマトークンを使う。
- 開くまで取得しないこと、条件変更時のページresetと重複refresh防止、URL変更、
  譜面/コースの状態分離、scope終了後のwatch停止をVueのreactivityでテストした。
  HTTP部分はmockし、`test:ir:ui` を既存の `test:ir` に組み込んだ。

検証: 対象7ファイルのPrettier、`bunx vue-tsc --noEmit`、`bun run test:ir` が成功。
IRテストは既存94件＋履歴3件の計97件。新しい自動import型は `bunx nuxt prepare` で生成。
ブラウザーの実表示・実アカウントの履歴取得は未実施。deployは行っていない。
ログ: `.local/refactor-06-tests.log`、`.local/refactor-06-typecheck.log`。
