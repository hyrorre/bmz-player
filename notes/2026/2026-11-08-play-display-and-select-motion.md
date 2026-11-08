# 音声予約の固定と選曲・HS変更時の移動

## 音声の先読み予約

- BMZ メニューのチェックボックスと選曲画面の `AUDIO LOOKAHEAD` を削除しました。
- アプリの音声開始経路から切り替え値を取り除き、音声時計の既定の先読み予約を常に使います。
- 旧 `audio.schedule_ahead` は読み込み時に無視し、保存時に除去します。既存のOFF指定を含む設定の読み込み・再保存をテストします。
- [音声設定](../../docs/settings.md#音声)と[音声予約と時計](../../docs/gameplay-runtime.md#音声予約と時計)を更新しました。

## 選曲項目のクリック

- 未選択の行をクリックしたとき、9KのKEY1 / KEY9と同じ選曲バー補間へ渡します。
- 通常の一覧、設定項目、G-BATTLEの候補一覧で低速スクロール時間を使います。一覧の端をまたぐ場合は短い距離で移動します。
- 選択中の行のクリックによる決定、右クリックによる退出は維持します。
- [カーソルの移動](../../docs/controls.md#カーソルの移動)を更新しました。

## 検証

- Windowsで `cargo check -p bmz-player --locked`、all-targets Clippyが成功しました。
- 最終差分のfmt、check、all-targets Clippy、HS関連73件、Windowsのクリックハンドラー実行テスト1件が成功しました。0 / 500 / 1000ms、補間中の再操作、snapshotの交換と描画停止、通常ノート・LN・地雷・小節線・判定領域を確認しています。
- 音声予約変更後の `cargo test -p bmz-player --locked` は2956 passed、56 ignoredでした。
- 選曲クリックは、Windowsの独立したWinitイベントループで実際のハンドラーを呼び、前後・端をまたぐ移動の補間開始・途中・完了を確認しました。
- Windows実ウィンドウで音声の切り替え項目の削除、表示セクションの末尾への配置、500msの初期表示、0ms / 1000msの操作と保存を確認しました。画面操作はユーザーのEscapeで停止し、その後は自動検証を続けました。
- 専用データ `.local/validation/play-display-motion-20261108/` でサンプル譜面の自動演奏を30 Play framesまで実行し、正常終了しました。既存データは利用していません。
- 初回の検証用起動では既存gilrsのWGI初期化が `HRESULT(0x80070057)` でpanicしました。smokeでは一時データのゲームパッドをOFFにしました。物理音声の計測、実コントローラーによる演奏、macOS / Linuxは未確認です。
- サンドボックス内の全テストでは、変更外のOBS・HTTPダウンロード・ディレクトリリンク操作で失敗が発生しました。停止して通常のWindows権限で再実行すると、2961 passed、0 failed、57 ignored（別の起動テスト2件もignored）で成功しました。ローカルログは `.local/play-motion-tests.log` と `.local/play-motion-tests-unsandboxed.log` です。

## HS変更時の線形移動

- 「BMZ メニュー → プレイ → 表示」の最後に「HS変更時のアニメーション時間」を追加しました。0〜1000ms、既定500msで、全キーモード共通の `lane.hispeed_transition_ms` に保存します。
- gameplay threadで表示倍率の移動区間を管理し、immutableなprojectionへ複製します。window threadは描画時の `Instant` で評価するため、描画やsnapshot取得の間隔で移動時間を引き延ばしません。
- 操作で再計算した実効HS倍率に追従し、移動中の再操作は現在の表示倍率からつなぎます。BPM / SCROLL / SPEED自体、設定値の表示、判定、音声処理は補間しません。
- [ハイスピードの操作](../../docs/hs.md#操作)と[プレイ設定](../../docs/settings.md#プレイ)を更新しました。
