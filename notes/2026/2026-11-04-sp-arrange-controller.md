# SPの配置を左右どちらのコントローラーからも変更する（#64）

2026-11-04に、[issue #64](https://github.com/hyrorre/bmz-player/issues/64)の対応として実装しました。
調査・実装の基点は`d862b709`です。操作仕様は[controls.md](../../docs/controls.md)を参照してください。

## 原因と対応

選曲入力が7K / 14Kの場合、コントローラーの鍵番号は14Kのキーコンフィグで解決します。
右側のKEY1 / KEY2はKEY8 / KEY9となり、選択譜面が7Kでも2Pの配置を変更していました。
7Kの演奏入力は既定で左右どちらのパッドからも受け付けるため、右側で演奏する人が
自分の配置を変更する際に左側のコントローラーを使う必要がありました。

入力を解決した後、配置の変更先を決める段階で5K / 7Kの右側入力を1P側へ対応付けます。
メインコントローラーの設定や左右の入れ替えは追加しません。
演奏入力、保存形式、キーコンフィグ、物理デバイスの割り当ては変更しません。

- 通常パネルは左右どちらのKEY1 / KEY2でもSPの配置を次 / 前へ切り替えます。
- LR2パネルは左右どちらのKEY2でもSPの配置をLR2の順で切り替えます。
  マウスの1P / 2P配置ボタンは表示どおりの側を変更します。
- 10K / 14K、SP-TO-DP、D-BATTLEは左右それぞれの配置を変更します。
- 5K / 7KのG-BATTLE / AUTOPLAY BATTLEは相手の表示側と操作側を区別し、右側からも自分の配置を変更します。
  G-BATTLEの相手一覧が有効な場合は、プレイ開始時と同様にG-BATTLEを優先します。
- ALLフィルターでも選択譜面と適用される変換・BATTLE設定で判定します。
  モードが未確定の混合コースは左右別の操作を保ちます。
- 9K選曲入力の操作は変更しません。

## 実装と回帰確認

`select_flow/mode_config.rs`に配置変更用の論理レーン解決を置きました。
既存の`select_effective_key_mode`を使い、DP変換とBATTLEの優先順位を共用します。
通常パネルのキーボード・コントローラー入力は配置変更処理を共用し、LR2パネルも同じ変更先を参照します。

`app/tests/select/arrange_controls.rs`はWindowsのWinitイベントループを必要とするため、
既存のアプリ制御テストと同様に通常はignoredとして、単独で明示実行します。
外部スキンを必要としない最小の通常 / LR2スキンを使い、アプリの入力処理と変更後の配置を確認します。

- 7K→14K→7K、5K / 10K、ALLフィルター
- 両側のキーボード・コントローラー、次 / 前への変更、非対象側の配置の保持
- SP-TO-DP、D-BATTLE、AUTO SCRATCH付きD-BATTLE、G-BATTLE、AUTOPLAY BATTLE
- 相手一覧が有効な場合のG-BATTLE優先、混合コース、9K選曲入力
- コントローラーの左右の入れ替え、独自のボタン番号

操作仕様書と6言語のマニュアルを更新しました。

## 検証結果

Windowsで以下が成功しました。

- `cargo fmt --check`
- `cargo check -p bmz-player --locked`
- `cargo clippy -p bmz-player --all-targets --locked -- -D warnings`
- `cargo test -p bmz-player --locked`：2,853件成功、libの54件と起動確認の2件はignored
- `cargo test -p bmz-player --locked --lib select_arrange_controls_follow_sp_dp_transitions_from_either_side -- --ignored --test-threads=1`：追加したアプリ回帰テスト1件成功
- `cargo build -p bmz-player --locked`
- `bun test bmz-ir-web/tests/manual.test.ts`：8件成功

制限された実行環境では、IR / OBS等のローカル模擬サーバーへの接続がOS error 10013で失敗しました。
失敗したログイン契約テストの単独実行でも同じ通信拒否を確認し、ローカル通信が可能な権限で全体を再実行して成功しました。
最終の全体テストログはローカルの`.local/issue-64-tests-unrestricted.log`に保存し、Gitには含めません。
ビルド時のincremental cacheのhard link警告はコピーへのフォールバックで、ソースの警告はありません。

物理コントローラー2台での操作、実ウィンドウでのスキン表示、macOS / Linuxでの動作は未確認です。

## 2026-11-04追記：すべてのSPキーモードへ拡張

PR #67の初回コミット`eb17d966`では、実効モードが5K / 7Kの場合だけ右側入力を1P配置へ振り替えていました。
4K / 6K / 8K / 9Kと7K→6K / 7K→9K変換でも自分の配置を変更できるように、既存ブランチへの追加コミットで対象を広げます。
ここから下は拡張後の仕様・検証結果です。上の節は初回対応時点の記録です。

### 配置変更先の判定

`select_flow/mode_config.rs`の`select_arrange_option_lane`を純粋な判定ヘルパーとして分離しました。
入力に使う選曲配列と、選択譜面・変換・BATTLE設定から得た実効モードを別々に受け取ります。
通常 / LR2パネルから呼ぶ`selected_arrange_option_lane`は、既存のモード解決とG-BATTLE優先処理を共用します。

- 7K/14K選曲入力では、4K / 5K / 6K / 7K / 8K / 9KのSPと7K→6K / 7K→9Kを1P配置へ対応付けます。
- 10K / 14K、5K→10K / 7K→14KのSP-TO-DP、D-BATTLE / BATTLE ASは左右独立です。
- SPのG-BATTLE / AUTOPLAY BATTLEは、相手の表示領域があっても自分の配置を変更します。
  IR対戦相手一覧が有効な場合はG-BATTLEを優先します。
- 9K選曲入力のKEY8 / KEY9は振り替えず、既存のターゲット操作を保ちます。
  7K/14K選曲入力でネイティブ9K譜面を選ぶ場合とは区別します。
- 混合コースなどモード未確定の場合は、初回対応と同じ左右別の操作です。

演奏入力、左右スロット、キーコンフィグと保存形式、配置アルゴリズム、プレイ開始時の変換処理は変更しません。
スキンイベント42 / 43とLR2マウスの1P / 2P配置ボタンも、指定された側の配置を変更します。
操作仕様書と6言語のマニュアルを更新しました。

### 追加した回帰確認

`app/tests/select/arrange_controls.rs`をOS非依存の純粋テスト2件と、Windowsのアプリ統合テストに分けました。
純粋テストは通常のテスト実行に含め、Windows統合テストはイベントループの制約により単独で明示実行します。

- 全SPモード、DP、未確定モードについて全レーンの対応を確認
- 9K選曲入力のKEY8 / KEY9保護とターゲット・通常配置キーの対応を確認
- 通常 / LR2パネルのキーボード・コントローラー両経路で、全SPと7K→6K / 7K→9Kの1P変更・2P保持を確認
- 通常パネルの次 / 前、LR2の既存の配置切り替え順を確認
- 10K / 14K、両方のSP-TO-DP、D-BATTLE / BATTLE AS、G-BATTLE / AUTOPLAY BATTLE、IR一覧の優先順位を確認
- ALLフィルター、7K→14K→7K、混合コース、左右スロットの入れ替えと独自ボタン番号を確認
- 9K選曲入力のアプリ操作と、スキンイベント42 / 43・LR2マウスボタンの左右独立を確認

### 拡張後の検証結果

Windowsで以下が成功しました。

- `cargo fmt --all -- --check`
- `cargo check -p bmz-player --locked`
- `cargo clippy -p bmz-player --all-targets --locked -- -D warnings`
- `cargo test -p bmz-player --locked`：2,855件成功、libの54件と起動確認の2件はignored
- `cargo test -p bmz-player --locked --lib select_arrange_controls_follow_sp_dp_transitions_from_either_side -- --ignored --test-threads=1`：拡張したWindowsアプリ統合テスト1件成功
- `cargo build -p bmz-player --locked`
- `bun test bmz-ir-web/tests/manual.test.ts`：8件成功

純粋テスト2件は全体テストにも含めて実行しました。
全体テストは初回対応時に必要だったローカル通信の権限を使って実行し、
ログをローカルの`.local/issue-64-all-sp-tests.log`に保存しました。
incremental cacheのhard link警告はコピーへのフォールバックです。ソースの警告はありません。

物理コントローラー2台、実ウィンドウでのスキン表示、macOS / Linuxでの実行は未確認です。
Windowsアプリ統合テストの通常 / LR2スキンは外部素材を必要としない最小の定義を使っています。
