# 選曲バーのスコア・状態フィールド

## 調査と実装

基点はmain `23dd79a6ace3975756cdcc96bdbcf02fe987b3a1`。
beatoraja `196facb07712166631046d84af5809936c7100f1` の
`skin/json/JsonSkin.java` のSongList、`select/SkinBar.java` を参照した。
既存のliston/listoff、text、level、lamp、trophy等は変更せず、BMZ専用の13フィールドを追加した。
現在の仕様とJSON/Luaの例は[skin.md](../../docs/skin.md#bmz-songlist-score-fields)に記載した。

- SelectRowSnapshotの既存値を使う。ランクの境界計算・ラベルはScoreGradeFactsと共有する。
- BP/CB/コンボはscore_bestの独立した最良値を使い、代表プレイの内訳とは分ける。
- ランプのみの行の数値列は中立値0なので、通常のバッチ取得で数値記録の有無を追加取得し、
  Snapshotへ渡す。従来のlampや選択曲refの値は変えない。DB migrationは不要。
- JSONの宣言順を維持するためserde_jsonのpreserve_orderを有効化した。
  Luaのテーブルキーは順序を持たないため、scoreOrderで追加部品の重なり順を明示できる。
- inline text/value素材は既存型を再利用する。通常textと描画本体を共有し、
  数字の配置・UV・色・画像生成は通常valueと共通関数へ切り出した。
- フォント名前空間と必須素材収集を追加部品にも適用した。描画からDB/IRへアクセスしない。

## 検証範囲

13フィールドの値評価から描画アイテム生成、ランクrefとの全スコア比較（ノーツ数1〜100）、
同距離・境界一致・重複境界・MAX、未記録と0、ランプのみの記録、ライバル未選択・未記録・同点、
お気に入り4状態、配列と描画順、スクロール行・位置、Snapshot更新、dst条件・timer・clipを検証する。
JSON/Lua、ベクター/ビットマップフォントの設定と通常loaderのfallback・素材解決も対象。
既存のLN/ScoreKey/rule modeと独立最良値の保存テストはworkspaceテストで確認する。

Rust未導入のDebian環境にツールチェーンをユーザー領域へ導入し、FFmpeg等の開発ヘッダーを
Git管理外の作業領域へ展開して検証した。実スキンはmainで固定されたRmz-skin、mz-select、
Luxez-Flatのsubmoduleを使用する。

実機ウィンドウでの見た目・長時間スクロールと、大量行での性能計測は未実施。
文字描画は既存atlas経路を使い、追加部品未定義時は空のループのみとなる。
digitsで素材に必要な符号・小数点・%がない場合や枠数不足の場合は数値全体を非表示にする。
TTF/OTFとビットマップの描画設定はアイテムで検証し、実フォントのdecode/fallbackはloaderで検証する。

## 最終検証結果

- `cargo fmt --check`: 成功。
- `cargo check --workspace --locked`: 成功。
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: 成功。
  未変更のbmz-audioにanyhowのmanifest警告がある。
- `cargo test -p bmz-render -p bmz-skin -p bmz-skin-document --locked --no-fail-fast`:
  最終差分で描画1060件、スキン432件、スキーマ37件が成功。
- `LANG=ja_JP.UTF-8 LC_ALL=ja_JP.UTF-8 cargo test --workspace --locked --no-fail-fast`:
  全crateを実行。bmz-playerは3251成功、2失敗、67 ignored。
  rendererのdst回帰テストはfixtureへ実際の行state適用を加え、上の最終suiteで全件再確認した。
- `cargo test -p bmz-player --locked numeric_play_after_clear_only`:
  最終クエリでランプのみ→初回数値記録（0点を含む）の区別に成功。
- 新規の通常loaderフォント・fallback・素材テスト、および既存LN/rule/独立最良値テストも成功。

この環境で残る失敗は今回変更していないファイル走査テスト2件。
`storage::scan::link_tests::overlapping_external_links_read_every_folder_once`は外部リンク先の
祖先内の譜面が不足し、`storage::scan::tests::scan_song_roots_skips_unreadable_subdirectory_and_keeps_accessible_files`
は読み取り不可ディレクトリのスキップ件数が異なる。PRにも明記する。
