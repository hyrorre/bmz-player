# 外部スコアDBの独立最小BPを保持する

最新main `490f8033`（PR #65取り込み済み）から、別ブランチで作業。
仕様は[スコア保存](../../docs/score-persistence.md#外部スコアdbの最小bp)、
取り込み操作・区分は[LN仕様](../../docs/ln.md#外部スコアdbの取り込みlr2--beatoraja--lr2oraja)を参照。

## 上流実装の確認

2026-11-04に以下を読み、DBフィールドの意味を確認した。

| 形式・入口 | 確認した実装と意味 |
| --- | --- |
| beatoraja | [ScoreData.update](https://github.com/exch-bms2/beatoraja/blob/9340253a866a2f0091d44c6fa9de7637ce229ef9/src/bms/player/beatoraja/ScoreData.java): EX改善時に判定内訳・ghostを置換し、minbpのMIN、comboのMAX、clearのMAXは独立更新。minbpの初期値はInteger.MAX_VALUE、validateは0以上を許可。 |
| beatorajaのDB | [PlayDataAccessor](https://github.com/exch-bms2/beatoraja/blob/9340253a866a2f0091d44c6fa9de7637ce229ef9/src/bms/player/beatoraja/PlayDataAccessor.java): score.dbへ集計score、scoredatalog.dbへ今回のnewscoreを書き込む。[ScoreDataLogDatabaseAccessor](https://github.com/exch-bms2/beatoraja/blob/9340253a866a2f0091d44c6fa9de7637ce229ef9/src/bms/player/beatoraja/ScoreDataLogDatabaseAccessor.java)はscoredatalogへINSERTする。scorelog.dbは別の更新差分ログ。 |
| LR2互換公開実装 | [OpenLR2 LR2_statplay.cpp](https://github.com/GOMazk/OpenLR2/blob/4ae445ad1825eb48735cbbe6e35fa95d032bc38e/LR2/LR2_statplay.cpp): 判定内訳の最高EX更新と、stat_maxcomboのMAX、minbpのMINを別々に実行する（単曲更新の660行付近）。minbp=-1は未記録。途中終了時は残ノーツを加えて更新する。 |
| LR2のDB | [OpenLR2 LR2_statlong.cpp](https://github.com/GOMazk/OpenLR2/blob/4ae445ad1825eb48735cbbe6e35fa95d032bc38e/LR2/LR2_statlong.cpp): scoreにperfect/great等とmaxcombo/minbpを別列で保存。[beatorajaのLR2 importer](https://github.com/exch-bms2/beatoraja/blob/9340253a866a2f0091d44c6fa9de7637ce229ef9/src/bms/player/beatoraja/external/ScoreDataImporter.java)もminbpを判定数とは別に読む。 |
| LR2oraja | [ScoreData](https://github.com/wcko87/lr2oraja/blob/lr2oraja/src/bms/player/beatoraja/ScoreData.java)（確認blob 4bbf6675943c082c54cd67ab84db1cd882ee3389）のupdateもEXとminbp/comboを独立更新。初期値Integer.MAX_VALUE、0以上が有効。[PlayDataAccessor](https://github.com/wcko87/lr2oraja/blob/lr2oraja/src/bms/player/beatoraja/PlayDataAccessor.java)もscoreとnewscoreの保存先を分離。 |
| LR2oraja ED | [ScoreData](https://github.com/seraxis/lr2oraja-endlessdream/blob/d4995153b0a759859ebc8f859dd868abae39db5a/core/src/bms/player/beatoraja/ScoreData.java)および[PlayDataAccessor](https://github.com/seraxis/lr2oraja-endlessdream/blob/d4995153b0a759859ebc8f859dd868abae39db5a/core/src/bms/player/beatoraja/PlayDataAccessor.java)でも、同じminbpの独立更新・初期値・score/scoredatalogの分離を確認。BMZでは通常規則とDXを選択した種別で分離する。 |

LR2は公開派生OpenLR2と互換importerの実装を根拠にする。全ての旧LR2配布バイナリで
DB生成・更新を実機確認したわけではない。確認したローカルscoreの未記録値は0ではないため、
正常なBP0を除外しない。NULLや負値は不明として扱い、beatoraja系のInteger.MAX_VALUEも
不明とする。Empty Poorが加算されるので、BP <= notesという制約は付けない。

最高EX時のBP20と別プレイで達成したminbp5が混在するのは正常。
scoredatalogのminbpはその行のプレイの値であり、scoreの累積最小値とは異なる。
既存のscoredatalog-only fallbackは維持し、各行で提供された値だけを別列に残す。
beatoraja系のoption/seedもminbpやcomboの改善で更新されるため、集計行から最高EX時の
実オプションを常に復元できるわけではない。既存の取り込み値を維持し、今回推測補正しない。

## 変更と判断

`score_state_from_beatoraja` / `score_state_from_lr2`は判定内訳を組み立てるが、従来は
row.min_bpを捨てていた。履歴BPをminbpへ置換すると代表比較・判定との整合性が壊れるため、
ScoreRecordにOption<u32>を持たせ、v34で追加するscore_history.source_min_bpへ保存する。
ローカルプレイはNULLに限定する。履歴INSERTと補助列の設定は既存の保存transaction内。

通常保存ではMIN(判定由来BP, source minbp)、cleanup/rebuildでは各履歴の同じMINを
さらに集計する。#65の代表履歴比較はhistory.bpだけを使うまま維持する。
再照合ではsource minbpを重複判定条件へ追加せず、既存履歴を検索し、NULLの補完か
より小さい値だけを同じ履歴へ保存する。回数・代表情報は変更しない。
device/ghostの従来補正と同じtransactionへまとめ、失敗時はBP補正も巻き戻す。

元DBが後で悪化・不明になっても、以前確認できた最小値は失わない設計とした。
その根拠となる履歴を削除すると寄与も消え、残存履歴から再集計される。
v34では既存値を変更せず、失われた過去minbpは元DBの再インポートで補完する。

MAX COMBOも上流の独立集計値であり、既存BMZの取り込みはrow.max_comboを履歴とBESTへ
保持している。source minbpのような破棄は無い。代表プレイそのもののコンボを上流集計行から
復元する情報は無いため、今回これを推測したり別履歴へ分解したりしない。
最小CB、コースの仕様、IRサーバー・外部スコアの自動送信も変更しない。
LR2コースは従来どおり具体的なu32 BPを要求し、NULL/負値の単曲サポートを流用しない。

## 検証

前回と同じDebian 13 / Rust 1.99.1 / native依存入りコンテナを再利用。
ログは作業環境の`/workspace/bmz-import-bp-*.log`に保持し、コミットしない。
元アプリの実ユーザーDB、実機UI、外部IRへの送信は行わない。

- `cargo fmt --all -- --check`: 成功。
- `cargo check -p bmz-player --locked`: 成功。
- `cargo clippy -p bmz-player --all-targets --locked -- -D warnings`: 成功。
- `cargo test -p bmz-player --locked --lib storage::score_import`: 39件成功。
- `cargo test -p bmz-player --locked --lib storage::score_db`: 79件成功。
- `cargo test -p bmz-player --locked --lib storage::migration`: 11件成功。
- 日本語ロケールで対象crateのlibテスト全体: 2836件成功、失敗0、36件ignore、31件filter。
  前回#65で確認したスキンアセット必須25件と同prefixのignore 5件、rootでは成立しない
  ディレクトリ権限テスト1件を除外した。今回もスキンsubmoduleは取得していない。

全体実行は`cargo test -p bmz-player --locked --lib --`へ次のskipを指定した。
`skin_loader::tests::luxe_battle_info`、`skin_loader::tests::luxe_detail_options`、
`skin_loader::tests::mz_battle_info`、`skin_loader::tests::mz_detail_options`、
`skin_loader::tests::detail_options::detail_experimental_off_keeps_legacy_and_on_shows_numbers_without_buttons`、
`storage::scan::tests::scan_song_roots_skips_unreadable_subdirectory_and_keeps_accessible_files`。

追加14テストで、全4種別の初回import・ローカルBESTとのマージ・BPだけの再import、
v33記録からの復旧、scoredatalog-only、0/NULL/-1/Integer.MAX_VALUE/notes超過値、
再importの単調改善とdevice同時補正、全ScoreKey軸、代表比較、cleanup一致・削除時の
根拠消失、v34の保全/再実行/rollback、補正transactionのrollbackを確認した。
最初のscore DB実行で、最新schemaからuser_versionだけを31へ戻す旧fixtureが
v34の列重複を起こしたため、実際の旧schemaへ戻すよう修正して再実行した。
PR #65のv33限定テストはv33までを明示し、通常保存・cleanupテストは最新schemaを使う。

自己レビューでは、通常保存とrebuildのBP式、代表比較がhistory.bpだけを使うこと、
再照合の全ScoreKey条件、BP補正で代表情報・回数・IR identityを変更しないこと、
migrationが列追加だけであることを確認した。コースの具体値を渡す型調整は保存仕様を変えず、
既存のLR2コース取込テストも通る。通常/IR downloadのsource_min_bp既定値はNULLのまま。
