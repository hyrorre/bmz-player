# macOS GameController入力

IOHIDの開発ビルドでの許可問題を受け、許可要求なしの前面入力として
GCKeyboardとGCControllerを段階的に追加する。
現在の契約・測定境界は [macOS遅延検証](../../docs/macos-latency.md)、
画面操作は [controls](../../docs/controls.md) を参照。

## 第1段階: GCKeyboard

対象commit: `f90ee1e9`。

- macOS 11以降。キー別pressedChangedHandlerを専用の直列dispatch queueに登録する。
- callback受信時のBMZ単調時計を判定に使う。OS側の取得遅延は測定不可。
- 物理キー・JIS Native:MacOS表現は既存helperを再利用する。
- 入力route世代とnative handler世代を照合する。旧世代を拒否してから登録を更新し、
  route変更時点で既に押されていたキーは解放まで再押下を受け付けない。
- 切断・フォーカス喪失・停止時に旧sinkへ解放。close fence後にRust contextを解放する。
- API未対応OS、未接続、割り当てたキーがAPIで得られない場合はwinitへ戻る。
- Autoは変更していない。ゲームパッドOFFでも独立取得する。

### 検証

- macOS hostでcheck、all-targets Clippy成功。
- 短い押下・解放、旧世代拒否、フォーカス喪失、未対応キー、切断の状態遷移テスト成功。
- 実際のGCKeyboard APIのopen/close fenceテスト成功。実キー受信の証明ではない。
- sandbox内の全体テストはローカル待受socketのPermissionDeniedで17件失敗。
  通常環境では2077件成功、6件ignore。生ログ: `.local/performance/gamecontroller-keyboard-tests.log`。
- 物理キーのA/B p95/p99、複数キーボード、JIS実機、スリープ復帰は未実施。

## 第2段階: GCControllerゲームパッド

- macOS 14以降は`GCController.input`の入力履歴を専用queueで即時drainする。
  `inputStateQueueDepth=128`は保持上限であり、配送を待つ件数ではない。
  個別のpressed/value host timestampをBMZ単調時計へ変換する。
- macOS 11〜13はプロフィールのキー/軸別callback引数を使い、受信時刻を記録する。
  API未対応OSではgilrs、未接続時はGameControllerのまま接続を待つ。
- gameplay配送と4096件までのUIコピーを分離する。履歴欠落とUIコピー欠落を別に記録する。
  scratch停止は専用queueの1ms timer、履歴の軸処理は入力サンプル時刻で進める。
- 標準ボタン・十字キー・stick/trigger軸に固定のGC入力名を付け、gilrsのraw番号を移行しない。
  公開APIに永続的な個体IDがないため、slot選択は実行中のみ有効にする。
  複数台は明示選択し、再接続時の同一個体推測は行わない。
- GC slotを保存から除去すると片方だけ未指定になるため、TOMLのslot配列は空文字で未指定を表す。
  他backendの保存済み割り当てと、メモリ上のGC選択は維持する。
- 不正時刻の解放は最後の有効な押下を解放する。物理状態を時刻検証より先に上書きしない。
  route世代変更とnative fenceの間の接続通知は、route非依存の機器構成として受理する。

### 検証

- macOS 26.6.2（25G83）aarch64 hostでfmt、check、all-targets Clippy、debug build成功。
- GCパッドの状態遷移10件が成功。短い押下・解放のOS時刻保持、旧API、履歴欠落、
  前面/切断、旧世代、不正時刻の解放、接続競合、digital/analog軸の基準状態、native closeを確認。
- Objective-C bridgeは`x86_64-apple-macos10.13`指定のSDK構文検査でwarning/errorなし。
  Intel版全体buildおよびmacOS 11〜13実機での動作確認は未実施。
- 全体の並列テストでは既存のLuaスキン準備と部分course replayの2件が失敗
  （2089件成功、6件ignore）。両方とも単独再実行では成功。今回の入力テストは全件成功。
  並列失敗の根本原因は未確定であり、関係のないテスト実装は変更していない。
- 全体を`--test-threads=1`で再実行し、2091件成功、6件ignore。追加・既存の入力テストも全件成功。
- 隔離した設定/DBでGCKeyboardとGameControllerを同時選択し、同梱default skin/sample譜面を
  autoplay再生。`--latency-stall-test --smoke-exit-on-result`でResult到達・正常終了を確認。
  `native_keyboard_active=true`、両APIの初期化、終了時のGC診断JSONを確認した。
  実パッド入力は0件であり、空の遅延分布は性能改善の証拠ではない。
- 生ログはローカルのみ: `.local/performance/gamecontroller-tests.log`、
  `.local/performance/gamecontroller-tests-serial.log`、
  `.local/performance/gc-smoke.KwHTs1/diagnostics.log`。
- 実キー/実パッドのA/B p95/p99、複数機器・同名機器・JIS・スリープ復帰の実機確認は未実施。

## 参照

- [Apple: modern physical input](https://github.com/apple/game-porting-toolkit/blob/main/game-porting-skills/skills/using-game-controller/reference/modern-input.md)
- [Apple: profile-based input](https://github.com/apple/game-porting-toolkit/blob/main/game-porting-skills/skills/using-game-controller/reference/profile-based-input.md)
