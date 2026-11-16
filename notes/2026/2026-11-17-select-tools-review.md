# 選曲ツールとローカルライバルのレビュー対応

[PR #116](https://github.com/hyrorre/bmz-player/pull/116)のレビュー指摘を修正した。

## 0msのESC終了

通常・Viewer待機中とも、オプションパネル表示中は単押しの終了要求を作らない。
終了要求を消費する際にもSelect表示中・パネル0を確認し、不許可になった要求と長押し計測を破棄する。
ルートで受理した押下は、次のフレームまでにEscapeを離しても一度だけ終了要求として扱う。

回帰テストはパネル1〜3、Select以外の各画面、不許可の要求がパネルを閉じた後に残らないこと、短い押下の一度だけの消費を確認する。

## 古いスコアDBと通知

ローカルライバルのDBを読み取り専用で開き、`user_version`を現行のスコアmigrationの版と比較する。
古い場合はSQLでスコアを取得せず、参照先のプロファイルで一度起動して更新し、F5で再取得する手順を通知する。
ライバル読み込み時にはmigrationもデータ修復も実行しない。更新は利用者が参照先で起動した際の通常処理に任せる。
通知は6言語へ追加し、選択切替・設定変更・毎フレームの更新を同じ通知付きのapp helperへ統一した。
戻り値を捨てて通知を取りこぼす経路をなくし、読み込みAPIには`must_use`を付けた。

実際のmigration列からv1とv36のDBを作る回帰テストで、専用エラー、読み込み前後のファイル内容の一致、通常migration後の再取得を確認する。
v1は不足列、v36はv37/v38のデータ修復が未適用のケースに対応する。

## clear-onlyのライバル表示

`SelectRivalSnapshot`へ`numeric_score_available`を渡す。
ランプのみのベストでは、名前・ランプ・選択状態を維持し、EX・BP・コンボ・判定内訳とEXターゲットの数値refを非表示にする。
数値のあるEX 0のベストは0として表示し、既存のIRベストは数値ありとして扱う。

回帰テストでは、ローカルDBのclear-onlyと実スコア0の照合、描画時のref 271/275/276、ターゲット、判定内訳とランプの保持を確認する。

## 検証

- `cargo fmt --all --check`、`cargo check --workspace --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo build -p bmz-player --locked`: 通過。
- `LANG=ja_JP.UTF-8 LC_ALL=ja_JP.UTF-8 cargo test --workspace --locked --no-fail-fast`: player libは3306件通過・2件失敗・68件ignore。他の対象は失敗なし。追加・変更した回帰テストはすべて通過。
- 残る失敗は、前回と同じ`storage::scan::link_tests::overlapping_external_links_read_every_folder_once`と`storage::scan::tests::scan_song_roots_skips_unreadable_subdirectory_and_keeps_accessible_files`。スキャンの実装・テストは変更していない。
- Xvfb + Vulkan LavapipeのLuxez-Flatで、E1、E2、両方の各パネルが表示されていることをキャプチャで確認し、0msのEscapeで終了しないこと、解放後に遅延終了しないことを確認した。メニューを閉じる・フォルダを戻る操作とルートでの単押し終了も通過。
- 隔離した参照先DBで旧版エラーを発生させ、通知文の表示とDBのSHA-256が実行前後で一致することを確認した。通知の画面確認では、起動時の表取得通知に上書きされないようfixtureの難易度表取得を無効にした。
- Windows / macOS、実機のコントローラー・音声出力は今回再検証していない。

関連仕様: [操作](../../docs/controls.md#irとライバル)、[設定](../../docs/settings.md#選曲)。
