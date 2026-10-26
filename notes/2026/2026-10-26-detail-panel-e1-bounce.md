# 実験的な詳細オプションのE1長押しとチャタリング

調査開始: 2026-10-26。実装・検証: 2026-10-27。
調査時点のHEAD: `405db380`。

## 症状と原因

実験的な詳細オプションで、コントローラーのE1を長押しして離してもパネルが残る。
現行のフィルターと状態機械を抜き出した検証で、以下を再現した。実機の入力列は未取得。

- 抑制20msで、押下0ms → 解放5ms → 押下10ms → 解放1000ms。
  最初の解放で固定表示になり、再押下は共通フィルターが抑制する。
  最後の解放も重複として抑制され、物理状態の再同期でも固定表示は残る。
- 1000ms間隔の押下・解放を一括処理すると、取得時刻が捨てられて処理時刻だけで
  200msを判定するため、チャタリングなしでも短押しになる。

## 変更

- app内の`ControlInputEvent`に取得時刻を保持する。キーボードは受信時刻、
  コントローラーは元の`GamepadButtonEvent.timestamp`を使う。
- 実験的なパネルのE1は、共通フィルターが抑制した物理イベントも観測する。
  E2とほかの単発操作は共通フィルターの受理結果に従う。
- E1解放は既存のデバイス別チャタリング設定（0〜20ms）だけ保留する。
  設定時間以内の再押下は同じ操作を続け、最初の押下時刻を維持する。
  固定表示を閉じた操作も、接触切れで開き直さない。
- 200msの判定には取得時刻の差を使い、解放確定までの待機を含めない。
  キューの全イベントと物理状態の再同期を処理してから、フレームの解放期限を進める。
- フォーカス喪失・モーダル等のキャンセルで保留を破棄する。
  修復された解放や不明・逆行した時刻から短押し固定を作らない。
- 物理hold・ゲームプレイ用の共通フィルターは変更しない。
  実験機能OFF時のLR2クリック開閉と3パネル方式も既存経路を維持する。

操作仕様は[操作方法](../../docs/controls.md)と
[詳細プレイオプション](../../docs/select-detail-options.md)へ反映。

## 検証

既存ASIO SDKのヘッダーを確認し、各Cargoコマンドのプロセス内だけで
`CPAL_ASIO_DIR=C:\Users\hyrorre\AppData\Local\Temp\asio_sdk`を指定する。
SDKの再取得、ユーザー設定の変更は行わない。

- `cargo fmt --check`: 成功。
- `cargo check -p bmz-player --locked`: 成功。
- `cargo clippy -p bmz-player --all-targets --locked -- -D warnings`: 成功。
- `cargo test -p bmz-player --locked --lib panel_session`: 19件成功。
  取得時刻・200ms境界、押し始め / 離し際の接触切れ、抑制済みの最終解放、
  キュー内の再押下、固定を閉じた後の再接触、E2、キーボードとE1の複数割当を検証。
- `cargo test -p bmz-player --locked --lib gamepad_route_keeps_suppressed_e1_edges_and_captured_timestamps -- --ignored --test-threads=1`:
  Windowsで1件成功。実際の`route_gamepad_button_event`へ時刻付きイベントを渡し、
  実験機能ONの接触切れと長押し解放、OFFの3パネルhold操作を確認。
  サンドボックス内では一時プロファイルのbootstrapでアクセス拒否となったため、
  実行制限を外して再実行した。実コントローラー・GPUを使う確認ではない。
- `cargo test -p bmz-player --locked -- --test-threads=2`: 成功。
  libは2688件成功・43件ignore、startup_stackは2件ignore。
  Windows入力経路の新規テストは上記の明示実行で確認済み。

実コントローラーのチャタリング波形、GPU表示、macOS / Linuxでの操作確認は未実施。
通常 / 詳細の両方で、短押し固定、長押し解放、E2切替、退出アニメーションを
実機で確認する余地が残る。

ローカルの検証ログは`.local/e1-panel-*.log`に保存し、Gitには含めない。
