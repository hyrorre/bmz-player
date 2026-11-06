# スキンの許可root統一と循環探索対策

開始HEAD: `df0ac3a1907c7342b1368e152abafc373ad84704`（#73 / #74マージ後）。
作業ブランチ: `codex/skin-root-boundaries`。

## 現行確認と変更

#73のLua資源上限・native pattern quotaと#74のIR修正は維持した。
添付指摘のうち、DirectoryScanのroot外リンク、SkinPathContextの外部entry自動fallback、
JSON/LR2と二段階資源の境界不一致、CONTINUE補助探索の重複は現行でも残っていた。

| 経路 | 変更 |
|---|---|
| カタログ・再スキャン・ヘッダ/UI | 列挙/評価前に実体のroot検査。安定した論理パスとoriginを保持 |
| JSON / LR2 / Lua | 共通のSkinPathPolicy/Contextへ集約。外部entryの暗黙承認を削除 |
| include / LR2 CONTINUE | 許可rootを引き継ぎ、include循環と64段/4096ファイル上限、補助探索を共通化 |
| image / animation / video / audio | 共通resolverで境界確認後にdecode/cache。runtime audioも既存contextを使用 |
| font / font page / PMchara / DXA | 定義の先の参照とarchive実体にも同じ境界を適用 |
| wildcard / customization / Lua overlay | 選択先と未作成パスの既存親を検査。ディスク書き込みは追加しない |
| システム音 | スキンとは別の設定rootを探索と音源選択へ引き継ぐ |
| 許可変更 | entryごとの承認永続化、解除・世代更新・cache分離・Lua VM破棄 |

フォントページは事前検査だけでなく、cache依存の算出と実際の画像decodeでも同じresolverを使う。
検査で正規化したパスと、リンク＋`..`を含む元のパスを読み直す場合の不一致を回帰テストにした。
resource/dataが同じ実体を指す場合も、明示設定された両方のroot名を保持してworkerへ渡す。
実体の重複排除で論理root名を捨てず、document cacheの識別にもその名前を含める。

自動探索rootはresource/dataのskins。外部スキンはユーザー操作による最小の親rootと、
必要ならprofile.tomlに追記したentry専用の共有rootで許可する。
既存パスだけを自動承認しない。詳しい移行方法は[skin.md](../../docs/skin.md#スキンのファイル参照と外部スキンの許可)。
自己・親・相互リンクと別名は実体visitedで省き、深さ64・entry数100,000で探索を打ち切る。
root自体の明示指定リンクは利用可能。root外リンク・壊れたリンク・重複・上限をログで区別する。

## 検証

Linux x86_64、Rust 1.99.1。OSの開発ヘッダーは作業用sysrootへ配置して確認。
共有targetを使うため`BMZ_RESOURCE_DIR`はこのworktreeの`data`へ明示設定した。
既存テストの既定localeに合わせて`LANG=ja_JP.UTF-8` / `LC_ALL=ja_JP.UTF-8`を設定し、
`BMZ_REQUIRE_MZ_SELECT=1`でmz-selectの実素材を必須にした。テストの並列度は4。
ログは`/tmp/bmz-root-{fmt,check,clippy,tests}-complete.log`（ローカルのみ）。

| 確認 | 結果 |
|---|---|
| `cargo fmt --check` | PASS |
| `cargo check --workspace --locked` | PASS |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | PASS（警告なし） |
| `cargo test --workspace --locked --no-fail-fast -- --test-threads=4` | PASS: 5,035成功 / 0失敗 / 58 ignored。doctestもPASS |
| Windows junction / symlink、macOS | NOT VERIFIED: Linux環境のため実行不可。Windows junction fixtureは維持・拡張 |
| GUIでの許可/解除、音声/GPU実機 | NOT VERIFIED: ウィンドウ・実機backendなし |

主なcrateの結果: bmz-player 2,922成功 / 42 ignored、bmz-skin 409成功 / 2 ignored、
bmz-skin-assets 14成功、bmz-skin-document 28成功、bmz-render 1,001成功 / 11 ignored。
合計はテスト内部の子プロセスが出力する1件を重複計上しない値。
`m_select_lua_select_skin_loads_when_available`、root別名、フォントの検査済みパスの再利用、
追加した境界テストはいずれもPASS。ignoredのGPU等の試験は実施済みとしない。

submoduleは固定commitを取得して検証した: mz-select `a4ee0e1`、Rmz-skin `cb08901`、Luxez-Flat `9e0d42e`。
`data/skins`にあるのはこの3件と同梱defaultのみ。
WMII / ModernChic / Litone / ECFN / ADFX02等の任意ローカル素材はNOT VERIFIED。
素材不在で早期returnしたテストを互換確認に数えない。JSON/LR2の境界は一時fixtureでも確認する。

追加テストは、評価前のカタログ拒否、root自体のリンク、自己/親/相互リンクと別名、深さ/entry上限、
外部entryの承認・設定往復・別entryへの非共有・リンク付け替え・解除時のcache分離/世代更新、
workerの論理パス保持、全形式の共有資源、JSON/LR2 include境界とLR2の循環/正常な再利用、
フォントページ/PMchara/DXA/wildcard/Unicode/大小文字、未作成overlayパス、システム音を対象とした。
既存のLua sandboxとnative pattern quotaのテストもworkspace実行に含める。

途中の全体実行で、旧仕様どおりdata直下等へ置かれたfixtureは許可拒否になったため、
ライブラリ配下への配置かテスト側の明示承認へ変更した。共有targetの資源探索とlocaleによる失敗は
上記の実行環境設定で解消し、無関係な期待値は変更していない。
初回の高並列実行では既存の曲ダウンロード試験にtimeoutも出たため、最終実行は並列度4とした。
中間実行のdoctestでは共有targetで別のcheck/buildが依存rlibを更新して失敗したため、
最終検証のfmt/check/Clippy/testは順次実行し、test中に別のcargo処理を重ねていない。

静的なリンク境界を対象とし、検査後の差し替えTOCTOUやOS handleによる完全隔離は保証しない。
解除時はインストール済みVMを破棄し、実行中workerの旧世代結果とcacheを切り離す。
開始済みのworker/OS I/Oの即時強制終了、ネイティブdecoderのプロセス隔離は対象外。
ユーザーの指定どおりGitHubへのPR作成・コメント投稿は行わない。
