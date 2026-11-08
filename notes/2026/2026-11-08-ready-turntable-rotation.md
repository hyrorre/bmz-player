# READY中のターンテーブル回転停止

## 原因

`1e73de02`（gameplay専用スレッド化）で、画面の経過時間を設定した後に
ターンテーブルの回転角を再計算する処理が削除されていた。
producerが作るsnapshotでは負の譜面時刻を0に丸めるため、READY中は回転角が進まない。
画面側で`play_elapsed_time`だけを更新しても、確定済みのoffsetには反映されない。

## 修正

- 描画用projectionにスクラッチ入力の累積回転量を保持し、producerの更新で引き継ぐ。
- Play画面向けのsnapshot取得時に、画面経過時間と累積回転量からoffset 1 / 2を再計算する。
- 終了演出のsnapshotも同じ画面時計で更新し、演奏終了時に譜面時計の回転角へ戻さない。
- producerから新しいsnapshotを受け取れないフレームでも通常回転を進め、READYから
  譜面時刻0への移行で角度をリセットしない。既存の位置・サイズ・alphaと他のoffsetを保持する。
- 開始前と開始後の回転角計算を共通化する。判定・入力・音声の時計は変更しない。

仕様は[スキン仕様](../../docs/skin.md)を参照。

## 検証

- gameplay clientを実際に起動する回帰テストを追加。7K / 14K、再生速度50 / 100 / 200%、
  READYから譜面時刻0への移行、snapshot交換停止、入力による累積回転量の更新、
  同じ画面時刻での再評価とoffsetの保持が成功。
- Windowsで`cargo fmt --check`、`cargo check -p bmz-player --locked`、
  `cargo clippy -p bmz-player --all-targets --locked -- -D warnings`が成功。
  incremental cacheのhard link失敗warningはcopyへのfallbackで、ソースのwarningではない。
- 独立したdata / cache / logsを使用し、mz-select antique 7Kと同梱サンプル譜面を実行。
  60 FPS指定の210 / 222フレーム目のスクリーンショットはともにGET READY表示中で、
  スクラッチ中央の線の向きが変わることを目視確認。自動演奏からリザルトへの正常終了も成功。
  検証素材は`.local/validation/ready-turntable-20261108/`に置き、Git管理しない。
- 全テストの最初の並列実行はgilrs-coreのWindows WGI callback内で`SendError`のpanicが起き、
  exit code 3で中断。制限環境の直列実行はIRのlocalhost HTTPテストで停止したため中止した。
  同じIRテストは制限外では成功した。全テストも制限外で
  `cargo test -p bmz-player --locked --no-fail-fast -- --test-threads=1`を実行し、
  libは2964 passed / 57 ignored / 0 failed、bin・doc testも成功。
  native windowを使うintegration test 2件は既定どおりignored。
  初回ログは`.local/ready-turntable-tests.log`、直列の中止ログは
  `.local/ready-turntable-tests-serial.log`、制限外ログは`.local/ready-turntable-tests-windows.log`。
- 物理音声の確認、実コントローラー、macOS / Linuxは未実施。
