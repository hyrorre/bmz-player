# macOS GameController入力

IOHIDの開発ビルドでの許可問題を受け、許可要求なしの前面入力として
GCKeyboardとGCControllerを段階的に追加する。
現在の契約・測定境界は [macOS遅延検証](../../docs/macos-latency.md)、
画面操作は [controls](../../docs/controls.md) を参照。

## 第1段階: GCKeyboard

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

## 参照

- [Apple: modern physical input](https://github.com/apple/game-porting-toolkit/blob/main/game-porting-skills/skills/using-game-controller/reference/modern-input.md)
- [Apple: profile-based input](https://github.com/apple/game-porting-toolkit/blob/main/game-porting-skills/skills/using-game-controller/reference/profile-based-input.md)
