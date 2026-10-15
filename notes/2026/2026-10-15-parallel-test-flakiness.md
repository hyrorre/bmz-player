# 並列実行で不安定になるテストの調査と修正

`cargo test` の並列実行や、同じテストを別プロセスで同時に動かしたときに失敗するテストを調べ、
一時リソースと待機条件を修正した。継続して守る規則は
[development.md](../../docs/development.md) の「テストの一時リソースと並列実行」に反映している。

- 実施日: 2026-10-15
- 対象: ブランチ `claude/festive-mendel-rnpzpw`（`origin/main` を `b1633644` でmerge済み）
- 環境: Linuxのクラウドコンテナ。テストはrootで実行。Windowsの結果は「Windowsでの追加調査」に記載。macOSでは実行していない。

## 調べ方

1. `bmz-player` の全テストを、既定のスレッド数で繰り返し実行した（修正前20回）。
2. コードを読み、一時パスの名前の付け方（PIDだけ、時刻だけ、長さ等）、一時ディレクトリ直下の使用、
   本番の固定待受先、非同期処理の途中状態での待機を洗い出した。
3. 疑わしいテストは、同じテストバイナリを2プロセスで同時に繰り返し実行して再現を確かめた。
   通常の1プロセス実行ではほとんど表に出ないため、この方法が有効だった。

## 再現した不具合

| テスト | 原因 | 修正前 | 修正後 |
|---|---|---|---|
| `bmz-chart` のimportテスト | 譜面と参照する音声・画像を共有の一時ディレクトリ直下に置いていた | 2プロセス同時100回中49回失敗 | 0回 |
| `bmz-player` の `import.rs` のテスト | 同上 | 2プロセス同時100回中29回失敗 | 0回 |
| `viewer_ipc` の往復テスト | 本番と同じ固定の待受先（127.0.0.1:39078、Windowsは固定名のnamed pipe）でlistenerを立てていた | ポート占有時は必ず失敗、2プロセス同時50回中21回失敗 | 0回（ポート占有時も成功） |
| IR一時停止のテスト | 非同期jobの途中状態を待ってからassertしていた | 競合で失敗 | jobの最終状態を待つよう修正 |

`viewer_ipc` は、platformの `request_command` / `start_listener` に待受先を引数で渡せるようにし、
テストだけOSが割り当てる空きポート（Windowsはプロセス内で一意なpipe名）を使う。
公開APIは従来どおり固定の待受先を使い、本番の挙動は変えていない。

ほかに、lr2 continueのアセットテストのパスをplatformごとの絶対パスに直した。

## 再現していないが直した潜在リスク

いずれも `tempfile::TempDir` に置き換え、片付けはDropに任せた。assertionは変えていない。

- `bmz-skin` の `lua_skin_rejects_paths_outside_root`: スキンrootの外側の `outside.lua` を
  一時ディレクトリ直下に作り、削除していなかった。
- `bmz-skin` のLR2テスト: ディレクトリ名が `{name}-{nanos}` だけだった。
- `bmz-audio` の `write_temp_wav`: ファイル名がPIDとWAVのバイト長だけで決まり、
  同じ長さで内容の違うWAVが上書きし合いえた（現状は長さが偶然異なる）。
- `bmz-render` の静的画像テスト、`bmz-chart` の音声・BGA素材テスト、`bmz-player` の `chart_asset` 等:
  名前が時刻だけ、またはPID+時刻で、最後でだけ削除していた。
- `bmz-player` の `storage/replay`、`generated_preview`、`profile_cmd`、`paths` 等、`bmz-font`:
  名前がPIDだけで、PIDの再利用時に前回の残骸を拾いえた。
- `bmz-player` の `select_ir`: 一時ディレクトリ直下そのものをprofile rootに渡していた。
- `skin_catalog_rejects_json_without_explicit_skin_type`: 後片付けがなく、実行のたびに残骸が残った。

## 修正後の検証

- `bmz-player` の全テスト: 直列1回、既定の並列25回。どの回も失敗は
  `scan_song_roots_skips_unreadable_subdirectory_and_keeps_accessible_files` の1件だけだった。
  rootで実行しているため読み取り不可のディレクトリも読めてしまうのが原因で、今回の修正とは関係しない。
  一般ユーザーで実行する環境では成功する想定（この環境では未確認）。
- 変更した各crateのテスト、`cargo fmt --all -- --check`、
  `cargo clippy --workspace --all-targets --locked -- -D warnings` は成功。
- Windowsのnamed pipe経路はビルド・実行していない。

## 調査中に分かった環境上の注意

- 複数のworktreeで `CARGO_TARGET_DIR` を共有すると、内容が同じcrateのテストバイナリが再利用され、
  `env!("CARGO_MANIFEST_DIR")` に元のworktreeのパスが残る。そのworktreeを削除すると、
  同梱データを参照するテストが存在しないパスを読んで失敗する。
- 実スキンを読むテストはメモリを多く使う。テストプロセスを複数同時に走らせるとOOMで
  強制終了されることがある（終了コード137）。
- 繰り返し実行で `/tmp` にテストの残骸とdebugビルドの成果物が溜まり、ディスクが不足した。
  生ログはセッションのscratchpadにだけ置き、共有していない。

## Windowsでの追加調査

署名し直したブランチ（`fd820a93`）で、Windows 11（論理CPU 24、メモリ64GB）を使って同じ観点を確かめた。
テストバイナリを直接起動するときは、vcpkgのFFmpeg DLLのあるディレクトリ
（`%VCPKG_ROOT%\installed\x64-windows\bin`）をPATHに加える必要があった。

| 条件 | 失敗 |
|---|---|
| 全17テストバイナリを1プロセスずつ、10回 | 0 / 170 |
| 同じバイナリを2プロセス同時に、5回 | 3 / 170（すべて `bmz_player`） |
| 下記の2テストだけを負荷なしで30回 | 0 / 30 |
| 同じ2テストを、busy-loopのプロセス48本（CPUの2倍）と並走させて | 14 / 14（途中で打ち切り） |

失敗したのは `gameplay_runtime` の `gameplay_results_replay_and_audio_are_invariant_to_render_stalls` と
`autoplay_and_replay_are_invariant_to_render_stalls` で、どちらも描画停止中に音の送出数が増えたことのassertだった。
手動入力では次の音がtick 200等の押下時にしかなく、停止開始から最大でdriverとgameplayスレッドの往復が
約10回必要になる。これを停止時間（100ms）以内に終える前提が、CPUが混むと崩れていた。
gameplay側は停止中も進行しており、製品コードの不具合ではない。

修正では、停止時間のsleep後もslotを保持したまま、期限付きの `wait_until` で時刻と送出数が増えるのを待つ。
修正後は48本の負荷下で12 / 12成功し、`bmz_player` のlibテスト全体を2プロセス同時に5回流しても
10 / 10成功した。`cargo clippy -p bmz-player --all-targets --locked -- -D warnings` も成功。

一時パスの衝突やnamed pipe名の衝突など、Windows固有の失敗は見つからなかった。
`unreadable_subdirectory` のテストは `#[cfg(unix)]` のためWindowsでは実行されない。
一方、`%TEMP%` には `bmz*` の残骸が約3.6万件（約405MB、大半は空ディレクトリ）溜まっていた。
大半は `bmz-skin` の `unique_test_dir` 由来だった。

## `%TEMP%` の残骸の解消

テストバイナリを1回ずつ実行して増えたエントリを数えると、`bmz_skin` libが158件、`bmz_player` libが約54件、
`bmz_skin_document` libが1件だった。いずれも `std::env::temp_dir()` 直下の `PathBuf` を返すhelper
（`unique_test_dir`、`unique_skin_test_dir`、pm_charaの `test_dir`、`.keep()` したlr2のhelper）由来で、
237か所の呼び出しのうち末尾で `remove_dir_all` していたのは約50か所だけだった。

helperが `TestDir` を返すようにした。`TestDir` は専用の `TempDir`（名前をprefixにする）の中の
未作成パスを指し、`Deref<Target = Path>` と `AsRef<Path>` を持つ。未作成パスにしたので、
helperが従来返していた「まだ存在しないパス」の意味は変わらない。呼び出し側はほぼそのまま使え、
コンパイルエラーは2か所（`root.clone()` と配列への値渡し）だけだった。手動の `remove_dir_all(root)` 48か所は削除した。

移行で見つかった落とし穴:

- `unique_test_dir("x").join("skins")` は一時値のガードが文の終わりで破棄される。
  後続の `create_dir_all` が親ごと作り直すので、テストは通ったまま残骸だけが残った（8件）。
  ガードを変数に保持してからjoinする形に29か所を書き換えた。パスだけを返すfixtureはガードも返すようにした。
- `fs::canonicalize(root)` はガードをmoveして即座に削除し、3テストが失敗した。`&root` に直した。

修正後、`bmz-skin`（293件）、`bmz-skin-document`（22件）、`bmz-player`（2465件）のテストは成功し、
`%TEMP%` の新規エントリは0件だった。3crateのlibテストを2プロセス同時に3回流しても18 / 18成功した。
既存の残骸のうち、テスト形式の名前を持つ `bmz*` のディレクトリ40,470件を削除した。
`bmz*` のファイル32件は過去の検証用ツールやcommit message等で、テストの残骸ではないため残した。

## 残課題

- `scan_song_roots_skips_unreadable_subdirectory_and_keeps_accessible_files` は、
  root実行時にskipするか、権限で読めないことを前提にしない形にするか未決定。
- `bmz-player` のほかのテスト（`storage`、`ir` 等）にはPID+時刻で名前を作り、末尾で削除する形が残っている。
  通常は残骸を残さないが、失敗時には残る。
