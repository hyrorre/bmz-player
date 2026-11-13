# Qwilightの打鍵タイミングと画面フェード

2026-11-13、`feat/qwilight-play-skins`で調査・修正。
現在の仕様は[skin.md](../../docs/skin.md#qwilightのプレイスキン)を参照。

## 原因と変更

- 打鍵タイミングの旧座標は`(ms + FAST幅) / (FAST幅 + SLOW幅)`だった。
  −220 / ＋280msの判定幅では0msが44%位置にずれる。表示オフセットはこの計算には含まれない。
- ユーザー指定により、判定幅による補正をやめ、`clamp(0.5 + ms / 500, 0, 1)`へ変更。
  固定の−250〜＋250ms、0ms中央、同じ絶対値の±msは等距離。4方向の向き・800msの残像・色は維持。
- Qwilightの遷移はプレイスキンではなく`@`テーマ側に`fadingProperty*`として定義される。
  インストール済み`@Default/@Default.yaml`には`30, 60, 1.0`の定義がある。
  プレイスキンのみ対応するBMZでは、このテーマ定義を解釈せず既定500msを使う。
- 通常退場の500ms黒フェードは既存実装だが、FAILED/途中終了は`close: 0`で即時遷移していた。
  入場用の`bmzPlayFadein`と終了用の`bmzPlayFadeout`を追加し、
  Qwilight変換では両拡張と`loadstart` / `fadeout` / `close`を500msに設定。
  終了用は既存timer 2/3の演出があればそちらを優先し、両終了経路の描画と待ち時間を揃える。
  `loadstart`で入場フェード完了まで譜面開始を待つ。シームレスなクイックリトライでは入場フェードを省略。
  描画はスキンの全体オフセット適用後に画面全体へ重ねる。既存JSON等は既定0で変更なし。

## フルコンボ演出の調査

CR_minimalの`band!-frame: 300`、`band!-framerate: 60`、`Paint/J 9 …`は5秒の演出を定義している。
BMZにはフルコンボ検出とtimer 48/49があるが、Qwilightアダプターはこの画像列・配置を読み込んでいないため未対応。
今回は対応状況の質問なので、演出追加は含めない。第三者製スキンのファイル変更は行わない。

## 検証

- `cargo fmt --check`、workspace全体の`check --locked`、`clippy --all-targets --locked -- -D warnings`が成功。
- `cargo test --workspace --locked --no-fail-fast -- --test-threads=4`が成功。
  bmz-playerは3105 passed / 78 ignored、bmz-renderは1020 passed / 23 ignored、
  bmz-skinは422 passed / 3 ignored、bmz-skin-documentは36 passed。
  一時ファイルとローカル通信を使える通常のWindows実行環境で実行した。
- 4方向それぞれで0ms・±125ms・±250ms・範囲外を検証。非対称な判定幅や表示オフセットを変えても同じ座標になる。
  入場0/250/500ms、オフセットから独立した全面描画、シームレス入場、FAILED、既存timer 3演出の優先も検証。
- `BMZ_QWILIGHT_SKIN_ROOT=data/skins`としてQwilight関連テストをignored含め実行し、19件成功（`qwilight_installed`は除外）。
  CR_minimalの実素材を使うGPUオフスクリーンテスト3件を含む。
  入場0/250/500ms、通常終了500ms、FAILED 250/500ms、シームレス入場を描画し、全暗転と中間の明度を確認。
- ローカルのみのログは`.local/qwilight-timing-validation/`、画像は`.local/qwilight-timing-previews/`。
  実ウィンドウでの操作・音声デバイスでのタイミング確認は未実施。実打鍵のFAST/SLOW偏り全般を解消したという検証ではない。
