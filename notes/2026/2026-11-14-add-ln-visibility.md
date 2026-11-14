# ADD LNで生成したLNの表示欠落（#103）

[Issue #103](https://github.com/hyrorre/bmz-player/issues/103) では、v0.5.2のWindows版で
ADD LNとEXTRA NOTEを併用するとコンボが頻繁に切れると報告された。
合成譜面をADD LN 0.15、seed 0で変換すると、LNの配列が始点12秒、18秒、2秒の順になった。
描画側は始点順を前提に二分探索していたため、2秒のLNが判定線に来ても表示されず、
入力がなければ見逃しPOORになった。EXTRA NOTEをOFFにしても再現した。

描画キャッシュに始点tick順の索引を作り、各位置までの終点時刻の最大値を同じ順序で保持する。
描画候補から保持状態を引くときは譜面内のペア番号を使う。
ADD LNによる追加やBATTLEの複製で譜面内のLNが時系列順に並ばない場合も、可視範囲を検索できる。

追加した回帰テストでは、EXTRA NOTE DEPTH 0・1・16で生成されたLNの表示、押下中の表示、
終点まで保持したときのPGREATを確認する。
Windowsで `cargo fmt --check`、bmz-playerのcheck、全ターゲットのClippy、
全テストを確認した。ライブラリテストは3,223件成功、88件ignore、失敗0件。
起動統合テストの2件はignoreのまま。
EXTRA NOTEの同時刻上限と衝突回避は [配置仕様](../../docs/extra-note.md) を参照。
報告者の譜面での手動プレイは未実施。
