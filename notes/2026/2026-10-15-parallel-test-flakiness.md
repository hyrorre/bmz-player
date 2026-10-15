# 並列実行で不安定になるテストの調査と修正

`cargo test` の並列実行や、同じテストを別プロセスで同時に動かしたときに失敗するテストを調べ、
一時リソースと待機条件を修正した。継続して守る規則は
[development.md](../../docs/development.md) の「テストの一時リソースと並列実行」に反映している。

- 実施日: 2026-10-15
- 対象: ブランチ `claude/festive-mendel-rnpzpw`（`origin/main` を `b1633644` でmerge済み）
- 環境: Linuxのクラウドコンテナ。テストはrootで実行。Windows / macOSでは実行していない。

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

## 残課題

- `scan_song_roots_skips_unreadable_subdirectory_and_keeps_accessible_files` は、
  root実行時にskipするか、権限で読めないことを前提にしない形にするか未決定。
- `unique_test_dir` を使うbmz-skinのテストは、パスの寿命を呼び出し元が管理する方針のまま。
  失敗時の残骸をなくすには、各テストが `TempDir` を保持する形への移行が必要。
