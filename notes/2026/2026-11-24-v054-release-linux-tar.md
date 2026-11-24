# v0.5.4 の Linux tar リリーステスト失敗

[Release Apps の Linux tar ジョブ](https://github.com/hyrorre/bmz-player/actions/runs/45305995647/job/138150064077)は、
`37c236bac` の `bmz-player` ライブラリテストで停止した。
失敗は `settled_runtime_result_records_final_gauges_and_stays_fixed_until_finished` の
`sampled.value > selected.value` の1件で、Windows、macOS 2アーキテクチャ、Flatpak のビルドは成功した。

ゲージ履歴は1回のワーカー更新につき定期サンプルを1時刻分記録する。テストが最初の更新で時計を1000msに進めると、
その更新は0ms、次の更新は500msの点を記録する。空POORの入力がその次の更新に届くと、1000msの点にも低下後の値が入る。
テストは1msと500msの更新完了を待ってから1000msの入力を送る。1000msの定期点と結果確定時の終端点を比較する検証は継続する。

ゲージ履歴の仕様は [採点の確定とResultのゲージ履歴](../../docs/gameplay-runtime.md#採点の確定とresultのゲージ履歴)を参照。

Windows でフォーマット、`bmz-player` の `check`、全ターゲットの Clippy、テストを実行した。
ライブラリテストは3472件成功、98件無視。対象テストは2プロセスで50回ずつ実行し、100回とも成功した。
修正後の Linux リリースビルドは未実行。

## 2026-11-24 公開確認

元のタグからテスト修正と CHANGELOG の追記を取り込んだ `493124ec1` を、承認後に v0.5.4 のタグに設定した。
[公開用 Release Apps](https://github.com/hyrorre/bmz-player/actions/runs/45367536665) で全5ビルドと添付工程が成功した。
Linux tar の `bmz-player` ライブラリテストは3506件成功、69件無視。失敗していたゲージ履歴テストも通過し、
実行用アーカイブと対応ソースからのオフライン再ビルドを検証した。

[v0.5.4 prerelease](https://github.com/hyrorre/bmz-player/releases/tag/v0.5.4) の配布物7本を公開済みのチェックサムと照合した。
`release.json` と `updates.json` の署名、Windows の最低 updater protocol 4 と橋渡し版 v0.5.3、
macOS arm64 / x64 の prerelease フィードへの反映も確認した。
