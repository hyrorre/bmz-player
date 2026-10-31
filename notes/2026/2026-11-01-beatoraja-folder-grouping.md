# 選曲のフォルダ分けをbeatorajaに合わせる

選曲画面のフォルダ分けがbeatorajaと違うという報告があった。報告のスクリーンショットでは、
難易度10の譜面と並んで、曲フォルダらしい名前の行がFOLDERとして表示されていた（譜面数4など）。
報告者の曲はローカルに無いため、同じrootを持つローカルの `data/library.db` と
`G:/beatoraja/songdata.db`（2026-10-17更新、書庫スキャン無効）を読み取り専用で比較した。
現在の仕様は [曲フォルダの一覧](../../docs/select-folders.md#曲フォルダの一覧) と
[書庫内譜面](../../docs/archive-songs.md#選曲画面のフォルダ構成) を参照。

## 規則の違い

| | beatoraja（`.local/beatoraja/`） | 変更前のBMZ |
|---|---|---|
| スキャン | 譜面を含むフォルダの下は読まない（`SQLiteSongDatabaseAccessor` の `if(!containsBMS)`） | 書庫・サブフォルダもすべて読む |
| 曲としてまとめる条件 | フォルダ自身に譜面がある | 下にフォルダが無い（末端） |
| フォルダを開いたとき | 1つ下に譜面があれば譜面だけ、無ければ子フォルダだけ（`FolderBar.getChildren`） | 末端でない子フォルダと譜面を混ぜる |
| 書庫 | 書庫を1つのフォルダとして登録し、中にフォルダの行を作らない | 通常のフォルダと同じ |

G:/BMS配下7rootでは、書庫の段を省く規則を適用して数えると、BMZのFOLDER行の分類は次のとおりだった。

- 自分の譜面があり、下にも譜面がある曲フォルダ: 434。下の譜面は書庫由来が455フォルダ、
  通常のサブフォルダが15フォルダ（`hidden`、`_keysound`、`hinagata_file`、`未配置`、`sabun`等）。
  テンプレートや没譜面が多く、beatorajaでは登録されない。
- 曲フォルダと並ぶパッケージ: 3（`INSANE/[Caelus] 無性別ファッションショー`、`6UE/omega.zip`、
  `OTHERS/bmz-archive-validation`）。前者はsongdata.dbでフォルダの行と8譜面が登録されているが、
  INSANEは `parent = bdc23bb4` の譜面が67592件あり譜面だけの一覧になるため、選曲画面からは辿れない。
  ユーザーの判断では配置ミスであり、beatorajaに合わせて隠す。
- 曲フォルダ内のパッケージ: 1。beatorajaと同じ表示: 157。

当初の集計は書庫の段を省く規則を入れておらず、パッケージを94件と過大に数えていた。

## 変更

スキャンとDB schemaは変えず、選曲画面の一覧だけを `FolderBar` の規則にした
（`crates/bmz-player/src/screens/select_model/folder.rs`）。

- 自分の譜面を含むフォルダを開いた場合は、その譜面だけを並べる。同一曲フォルダもこの結果になり、
  beatorajaの `SameFolderBar` と一致する。
- 譜面を含む子フォルダは譜面を直接並べ、その下は扱わない。曲フォルダが1つでもあれば子フォルダを並べない。
- 書庫の中ではフォルダの行を作らない。書庫の段を省く規則と表示名は従来どおり。
- フォルダの譜面数・ランプ（`folder_summary_for_song_folder`）とフォルダオートプレイは、
  同じ規則で辿れる譜面だけを対象にする（`reachable_chart_folders`）。
  beatorajaのフォルダランプは直下に並ぶ譜面だけを数えるが、BMZは辿れる譜面全体の集計を維持した。

一覧に出ない譜面もライブラリに残り、検索・難易度表・仮想フォルダからは選べる。

## 検証

- `cargo fmt --check`、`cargo check -p bmz-player --locked`、
  `cargo clippy -p bmz-player --all-targets --locked -- -D warnings` が成功。
- `cargo test -p bmz-player --locked` は2772件通過、48件ignore。報告と同じ形（曲フォルダ内の差分書庫・
  サブフォルダ）、曲フォルダと並ぶパッケージ、自分の譜面を含むフォルダ、書庫内のフォルダ、
  辿れない譜面を数えないフォルダ集計のテストを追加・更新した。
- 同じ規則をPythonで再現し、ローカルの2つのDBで各rootから辿れる164フォルダを比較した。
  152フォルダ（BOF22等）はsongdata.dbの更新後に追加されたもので比較できない。残る12フォルダ
  （各rootとAery5Key）は、並ぶフォルダと曲フォルダが一致した。差は `Aery5Key/yutaortslabo_asianeuro`
  の譜面をbeatorajaが登録していない1件だけで、譜面単位の登録差である。
  比較スクリプトと結果はセッションのscratchpadに置き、共有しない。

アプリでの目視確認、beatorajaを実行しての確認、報告者のフォルダ構成の確認は未実施。
