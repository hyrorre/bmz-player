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

## 第3段階: 入力設定の表示整理

- GCKeyboard/GameControllerの時計方式・文字入力・遅延測定の説明を設定欄から削除。
  接続状態を短く表示し、キー再登録の案内を残す。1P/2Pと再起動時の解除は割り当て欄で案内する。
- キーボードの表示名を`GCKeyboard`に変更。configの`MacOsGameController`は維持する。
- IOHIDも診断用の補足・エラーコードを表示せず、許可ボタンは未許可時、
  再試行案内は許可不足・エラー時のみ表示する。開発者向けの詳細はdocs/ログに保持する。
- OS/backendの利用可否を設定読み込みとUIで共用する。Raw InputはWindowsのみ、
  GCKeyboard/GameControllerはmacOS 11以降のみ表示する。版の確認だけでは入力を初期化しない。
  未接続や権限未許可を理由に選択肢を隠すことはしない。
- 非対応の保存済み選択はキーボードAuto/ゲームパッドgilrsへ戻す。他の入力設定は維持する。

### 検証

- macOS 26.6.2 aarch64でfmt、check、all-targets Clippy、debug build成功。
- 全体直列テスト2096件成功、6件ignore。OS別候補、非対応設定移行、全6言語の整合性を確認。
- Objective-C bridgeのIntel macOS 10.13指定のSDK構文検査もwarning/errorなし。
- 一時設定/DB・専用bundle IDのdebugアプリでF1 → 入力デバイスを実際に開き、
  短い接続表示、長い説明の削除、キーボード4候補・ゲームパッド3候補、Raw Input非表示を確認。
- macOS 10.x・Windows・Linuxの候補は模擬条件の自動テストで確認。各OS実機では未確認。
- ローカルログ: `.local/performance/input-settings-tests.log`、
  `.local/performance/input-settings.zcl5iD/`。一時アプリ・設定・DBはGit管理外。

## 第4段階: IOHIDのfeature化

- `bmz-player`に`macos-iohid` featureを追加し、default featureには含めない。
  Rust backend・native IOHID bridge・許可要求UIはmacOSかつfeature有効時だけコンパイルする。
- GameControllerとIOHIDが共有する前面確認・Mach時計取得を`native/input_common.c`へ分離する。
  GCKeyboard/GameControllerは通常ビルドでも引き続き利用できる。
- feature無効時はIOHIDを設定候補から隠し、保存済みの`MacOsHid`は読み込み時にAutoへ戻す。
  他の入力設定を維持し、有効ビルドではIOHID選択を保持する。
- 有効化・検証用パッケージの手順をdocsへ反映する。

### 検証

- macOS 26.6.2 aarch64でfmt成功。通常ビルドと`--features macos-iohid`の両方で
  check、all-targets Clippy、debug build成功。
- 全体直列テストは通常ビルド2096件、feature有効ビルド2098件成功。両方とも6件ignore。
  featureによる候補表示と保存済み設定の移行・保持を確認するテストを追加した。
- 実行ファイルのsymbolを確認。通常ビルドではIOHIDのopen/close/許可要求が含まれず、
  有効ビルドでは含まれる。共通の前面確認・時計取得とGC bridgeは両方に残る。
- 共通C helperとIOHID bridgeはIntel macOS 10.13指定のSDK構文検査でwarning/errorなし。
- 一時設定/DBで通常ビルドの同梱sample譜面をautoplayし、正常終了を確認。
  `MacOsHid`の保存済み選択はAutoへ移行し、GCKeyboard/GameControllerの同時初期化も確認した。
- IOHIDの許可付与・実キー入力、macOS 10.x/Intel・他OSの実機確認は未実施。
- ローカルログ: `.local/performance/iohid-feature-default-tests.log`、
  `.local/performance/iohid-feature-enabled-tests.log`、
  `.local/performance/iohid-feature.NTd9OT/`。設定・DB・ログはGit管理外。

## レビュー修正: キーボード復帰時の保持

GCKeyboardとwinitの受理・配送をKeyboardStateの同じロックで排他化した。
winitで受理した保持を専用のButtonDeliveryで追跡し、GC復帰前に旧sinkへ解放する。
未接続時のheartbeatでwinit保持を解除しないこと、再接続・再押下・route終了を回帰テストで確認。
player check / all-targets Clippy成功。実キーボードの抜き差し操作は未実施。
player全体はsandbox内で2083件成功・6件ignored・ローカルsocket制限17件失敗。
制限による失敗は関連モジュールをsandbox外で再実行し、すべて成功した。

## レビュー修正: UI保持の再同期

不正時刻・履歴欠落・routeリセットで解放済みのボタンをUI保持一覧から除外した。
物理状態のbaselineと再押下抑止は維持し、解放後の新しいPressでのみ再び一覧へ戻す。
gameplayへのReleaseに加え、UI一覧・複数ボタン・baseline・再押下・routeリセットを回帰確認。
GameController関連16テスト成功。繰り返しresetで同じReleaseを重複配送しないことも確認。

## 参考資料

- [Apple: modern physical input](https://github.com/apple/game-porting-toolkit/blob/main/game-porting-skills/skills/using-game-controller/reference/modern-input.md)
- [Apple: profile-based input](https://github.com/apple/game-porting-toolkit/blob/main/game-porting-skills/skills/using-game-controller/reference/profile-based-input.md)
