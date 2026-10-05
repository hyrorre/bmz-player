# v0.4.3以降のレビュー指摘修正

レビュー対象は `v0.4.3..70eb1a25`。承認された2件を順に修正する。
現在の操作仕様は [controls.md](../../docs/controls.md) を参照。

## GameControllerのアナログ入力の二重処理

`GCAxis*` は従来の `starts_with("Axis")` に一致せず、スクラッチの合成ボタンと
axis ticksが選曲・設定変更の両方へ流れていた。

- backendが付与する `synthesized_analog_axis` をapp共通イベントへ引き継ぐ。
- 選曲、通常・詳細オプション、設定一覧、Result IR、プレイのレーンカバー・緑数字を
  名前でなく合成元の情報で判定する。物理holdとgameplayへの配送は維持する。
- アナログスクラッチOFFの端点ボタンにはticksがないため、通常ボタンとして処理する。
- 回帰テストは従来名・GameController名・未知のbackend名について、holdの押下/解放、
  ticksの端数蓄積と1回だけの移動、OFF時のボタン移動を確認する。

検証:

- Windowsでfmt、bmz-playerのcheck / all-targets Clippy（`-D warnings`）が成功。
- bmz-player全テストは2,187件成功、19件ignore。追加回帰テストも成功。
- 初回はsandbox内の一時ファイルへのアクセス拒否とローカルHTTP待ちで完了せず、
  対象テストプロセスだけを停止した。保存テスト単独でos error 5を確認し、
  通常権限で全テストを再実行して成功した。ソース側で失敗を抑制していない。
- ASIO SDKは既存の`CPAL_ASIO_DIR`を指定。ログはGit管理外の
  `.local/review-fix1-{check,clippy,test-unrestricted}.log`。
- 実配置のLR2 / WMIX_HD / Seraphicやmz-select / Luxez-Flatのロードテストも実行。
  GPU・別途素材を必要とするignoreテストと、macOS実機のGameController操作は未実施。
