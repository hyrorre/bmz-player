# mz-selectのG-BATTLE相手情報

合意した左下の独立パネルをmz-selectへ実装。ECFN / Luxez-Flatは対象外。
文字は同梱のmgenplus-1c-bold.ttfへ揃え、HARD等のゲージ名は削除した。
鍵盤は既存result/keys1p.pngの白鍵・黒鍵を切り出し、番号をTTFで重ねる。
枠・背景・区切りはLuaのpanelで構築し、新しいPNGやフォントは追加しない。

一覧の表示中だけランキング / rival data / ir dataの本文を条件20200で隠し、
閉じると復帰する。上部のライバル選択は維持。架空のMIRROR OFF切り替えは設けず、
実際のMIRRORオプションと既存CHART REPLICATION設定を表示する。

選択中の相手情報は自分のオプションと別のsnapshot / ref群20200..20229で渡す。
IRの取得済みメタデータとローカルリプレイを参照し、欠損は未取得のまま扱う。
リプレイは120msの停止待ち後、同時1件まで別処理で読む。
カーソル・譜面変更時の古い結果は破棄し、一覧を閉じると状態をリセットする。
固定配置は既存の乱数・permutation helperで計算し、動的なノート配置は固定鍵盤にしない。

仕様: [skin.md](../../docs/skin.md#g-battle一覧の相手情報)

検証:

- `cargo fmt --check`、`cargo check --workspace --locked`、workspaceのall-targets Clippyを確認。
  app内のキャッシュキー調整後もcheckとbmz-playerのall-targets Clippyを再確認。
- `cargo test --workspace --locked --no-fail-fast`はbmz-playerの既存HTTPテストで
  Windowsのローカル接続制限（os error 10013）に当たった。待機し続ける今回の
  テストプロセスだけを終了し、残りのworkspace targetはすべて成功。
  同じbmz-playerテストバイナリをPC内通信が可能な条件で再実行し、
  2780成功・失敗0・48 ignored。bmz-renderは366成功・失敗0・1 ignored。
- 実素材のパネル表示、旧3欄の非表示・復帰、未取得への切り替え、
  自分のオプションとの分離をテスト。固定配置は各キー数・旧/現行seed・FLIPで
  実プレイの変換結果と比較し、一致を確認。古いカーソル・譜面・score keyの結果も破棄。
  最終のALL-SCR表記調整後も対象4テスト、app check / all-targets Clippy / buildを確認。
- Windows GPUでSP、DP（FLIP）、未取得、一覧を閉じた画面を描画して目視確認。
  `cargo build -p bmz-player --locked`も成功。
- 実IRサービスとの対戦プレイ、物理入力、macOS / Linuxは未確認。

描画に使った相手情報は検証用snapshot。ユーザーの設定・DBは変更していない。
プレビューとログは`.local/notes/2026-11-01-gbattle-layout/implemented/`に保存し、コミットしない。
