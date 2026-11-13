# Qwilight CLEAR演出を結果確定時に開始する

## 原因と変更

`feat/qwilight-play-skins`のCLEAR（Paint/J 10）は曲終了のtimer 908につながり、
最終ノーツから通常5秒の終了待機後に表示されていた。
Qwilightの参照実装は全ノーツの処理完了で開始するが、ユーザーの指定により、
BMZでは空POOR等の受付終了を含む結果の自然確定を起点とする。

- gameplayの既存の自然確定処理で、演出用の開始時刻を一度だけ保持する。
  手動退出やFAILEDの結果確定とは区別し、採点・保存・IRの条件は変えない。
- BMZ timer 19075へ開始時刻からの経過を渡し、J 10を接続する。
  consumer側のsnapshot再投影でも開始時刻から再計算し、曲終了時に再開しない。
- 通常プレイと動画出力で、退場時に残っている演出時間の計算を共有する。
  FULL COMBO / PERFECTの起点、timer 908、曲終了待機自体は維持する。
- 保存・通信の非同期処理には依存しない。外部スキンのYAML/Lua/ZIPは編集しない。

現行仕様は[スキン仕様](../../docs/skin.md#qwilightのプレイスキン)を参照。

## READY時間

追加依頼により、Qwilight変換にRmz-skinと同じ`playstart: 1000`を設定した。
従来は未指定のため0になり、ロード後に曲開始までの待機がなく、decide BGMのフェード時間も0になっていた。
既存のREADY制御により1秒待機し、その間にdecide BGMをフェードアウトする。
Loadingの500msフェード、READY中の操作待ち、アニメーション付きSTAGEFILEの待機は既存の処理を使う。

## 検証

Windowsで以下を確認した。ログはGit管理外の`.local/qwilight-clear-settlement-*.log`。

- `cargo fmt --all --check`、workspaceの`check --locked`と`clippy --all-targets --locked -- -D warnings`が成功。
- `cargo build -p bmz-player --locked`が成功。
- workspace全テストを実施。追加したPracticeの停止時計テストで、描画更新が終了演出の時計を上書きする問題を検出し、適用順を修正した。
  その後、READY追加も含めた`bmz-player` / `bmz-skin`の全テストを再実行して成功。
  最終結果はplayerが3129成功・82 ignore、skinが425成功・5 ignore。
  他crateは先の全体実行で成功（gameplay 275、render 1038など）。
- 空POORの受付、負の入力オフセット、リプレイの未処理tick、手動終了、確定後の時刻維持を検査。
  snapshot再投影では表示オフセットの独立性と50% / 100% / 200%の速度換算も確認。
- 動画出力では最初のノーツを見逃した通常クリアを作り、曲終了前の演出開始と、2秒・8秒の演出の残り待機を確認。
- Qwilightの合成fixtureで確定前・曲終了のみ・確定直後・演出終了後・FAILED・ゲージ不足を検査。
  READYの1秒宣言と、decide BGMの1秒分のフェードフレーム数も確認。
- `qwilight_cr_minimal_completion_gpu --ignored`が成功。インストール済みCR_minimalの6K素材でFULL COMBO / PERFECT / CLEARをオフスクリーンGPU描画。
  `.local/qwilight-clear-settlement-previews/CR_minimal-clear.png`のCLEAR表示を目視確認。

最初の制限付きテスト実行では、既存ダウンロードテストのローカル接続がWindowsエラー10013で失敗した。
通常環境での全体実行と再実行では成功した。ビルドキャッシュのhard link警告は環境由来で、コードの警告抑制は行っていない。
実ウィンドウでのプレイ、decide BGMの聴感、コントローラー、macOS/Linuxは未確認。
