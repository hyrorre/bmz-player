# レビュー指摘5件の回帰修正

2026-11-07。レビュー基準と作業開始時の `main` は `9aa12fbd9506f49b2e6a625160cca049bcabc91a`。
`origin/main` を取得した後もSHAは一致していた。同じコミットから5件を順に修正した。

| 修正 | コミット | 再現と確認 |
| --- | --- | --- |
| Luaの `java.io.File` | `75039850` | 修正前は過大パスを受理した。4 KiBのパス制限をUTF-8変換前に検査し、Lua VMごとにRust側の保持量を32 MiBに制限した。小さい予算で `listFiles`、割り当て失敗、GC、未作成ディレクトリのoverlayを確認した。 |
| 音声の再生順序 | `551fd61b` | 48 kHzのframe 48,000で60 ms遅れのBGMが押鍵音を消し、PCMが無音になった。即時要求を次のバッファ先頭に割り当て、絶対frame 0予約と区別した。複数batch、同一締切、後続の再発音、catch-up、先読みOFF、pauseと速度変更をPCMで確認した。 |
| IRのプレイヤー一覧 | `b13b713e` | Drizzleが生成したSQLは `limit=99` で101個、`limit=100` で102個のバインドを使った。公開条件の2個を含めて100個に収め、通常・コースの統計をプレイヤーIDで統合した。実SQLを実行するSQLiteテストで上限、公開条件、末尾のチャンクとページングを確認した。 |
| OBS再接続 | `5c94f860` | 切断中の遷移と終了通知の欠落をWebSocketで再現した。切断中もプレイ試行を更新し、再接続時に現在の表示を同期する。切断中に開始・終了・リトライした場合と、録画再開待ちで切断した場合の送信順と不要な録画要求の不在を確認した。 |
| JSON PMchara | `072a5b62` | 大小文字を区別するボリュームで直接指定とwildcardの展開失敗を再現した。許可root内で各階層のASCII大小文字差を補正し、各リンクの実体を検査した。保存済み選択、入れ子のsuffix、Unicode、root alias、外向きリンクを含めてruntimeと画像sourceを確認した。 |

関連仕様は [スキン](../../docs/skin.md)、[音声予約と時計](../../docs/gameplay-runtime.md#音声予約と時計)、
[IR](../../docs/ir.md)、[OBS](../../docs/obs.md) に記載する。
D1の上限は [Cloudflare D1 Limits](https://developers.cloudflare.com/d1/platform/limits/) を参照した。

## 検証環境

macOS arm64、Rust 1.99.1、Bun 1.4.4。Rustの依存取得後は `--locked --offline` を使用した。
PMcharaは一時的な大文字・小文字を区別するHFS+ボリュームを `TMPDIR` に指定した。実行後に取り外した。
WebSocketとHTTPのテストにはローカル待ち受けを許可した。
詳細ログは `.local/review-fix-*.log` にあり、Gitには含めない。

| 検証 | 結果 |
| --- | --- |
| `cargo test -p bmz-skin --locked --offline java_file_` | 5成功。小さいLuaメモリ上限を使って生成途中の失敗を確認した。 |
| `cargo test -p bmz-audio -p bmz-gameplay --locked --offline` | 音声161成功・1 ignored、ゲームプレイ263成功。プレイヤーの音声再送・中断12件と動画シミュレーション6件も成功。 |
| `bun run test:ir` | 197成功。Nuxtの生成型を更新してから `vue-tsc --noEmit` が成功。変更したTypeScriptのPrettierとESLintも成功。 |
| `cargo test -p bmz-player --locked --offline --lib obs::` | 40成功。実OBSの代わりにローカルのモックWebSocketを使用した。 |
| `cargo test -p bmz-player --locked --offline --lib skin_loader::tests::pm_chara` | 大小文字を区別するボリュームで6成功。うちSimple Playの実素材に依存する1件は素材不在で早期return。 |
| `cargo fmt --check`、`cargo check --workspace --locked --offline`、`cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | 成功。 |
| `cargo build --workspace --locked --offline` | 成功。 |
| `cargo test --workspace --locked --offline --no-fail-fast -- --test-threads=4` | 5,103成功、6失敗、59 ignored。テスト内の子プロセスの成功1件は重複計上していない。今回の回帰テストはすべて成功。 |
| `cargo test -p bmz-player --locked --offline --lib video_export::tests::generates_complete_mp4_and_metadata_without_persistence -- --ignored` | macOSのGPUと `BMZ_TEST_FFMPEG` に指定したFFmpegで461フレームのMP4とメタデータを生成した。保存データの非更新を確認した。 |

workspaceテストではmacOSの一時ディレクトリを実体パスで指定し、`BMZ_RESOURCE_DIR` を同梱データ、
`BMZ_REQUIRE_MZ_SELECT=1`、`LANG` と `LC_ALL` を `ja_JP.UTF-8` に設定した。
失敗の内訳は、外部WMII LR2のBGA・ゲージ・ビットマップフォント・ボムの4件、
未許可の `.local/beatoraja` のJSONを読む1件、書庫内動画の白画素が253となる1件。
レビュー基準コミットの一時コピーでこの6件に絞って実行し、同じアサーションまたは許可エラーを再現した。
基準のビルド出力を分離し、ローカルの素材は比較に必要なものだけをコピーした。

macOSの既定音声出力で `audio-probe auto auto 48000 3` を実行し、無音ストリームのコールバックを確認した。
押鍵とBGMの重なりはソフトウェアPCMで検証した。Windows / Linuxの実機、実OBS、Cloudflare本番D1は未検証。
