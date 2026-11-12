# 1P / 2Pで異なるキーコンフィグの入力解決

実施日: 2026-11-11

## 背景

14Kで1Pと2Pのキーコンフィグを変えると、選曲・設定・プレイの操作が誤動作していた。

- A: 選曲用の割り当てを、1P / 2Pの区別なく入力名の集合にまとめていた。2PのKEY1 / KEY2を入れ替えると、どの機器のButton1 / Button2も決定になった。2PのKEY3をButton1にすると、1PのButton1でGAUGEのつもりがRANDOMになった。
- B: アナログ軸のtickからDeviceIdを落としていた。プレイ中のE1 / E2 + SCRは、プレイの設定ではなく選曲用の`select_keys`で方向を決めていた。
- C: `GamepadSlotMap::device_id_for_player`が、割り当てのないスロットへ接続順の既定IDを補っていた。重複を避けて未解決にしたスロットが、別側と同じ実機を指すことがあった。

## パイプライン

入力は次の順だけで操作へ到達する。

1. backendの入力（winit / Raw Input / gilrs / GameController / evdev）
2. `ControlInputEvent`（機器、物理入力、Press / Release、時刻、repeat、アナログ由来か）
3. `AppInputRuntime::track_control`で入力元ごとの押下集合を更新
4. `input_resolve::resolve_input`で`ResolvedInput`を作る
   - `scene()`: 選曲・決定・リザルト・設定画面の意味（`SceneInputResolver`）
   - `play()`: 現在のプレイセッションの設定対象モードの意味（`PlayOptionInput`）
5. `scene_input::select_action` / `decide_action` / `result_action`などで操作を決める
6. 操作を実行する

`ResolvedInput`のコンストラクタは`resolve_input`だけで、操作層の関数は`&ResolvedInput`か`&ResolvedControl`を受け取る。
入力名の文字列を受け取って意味を判定するAPIは削除した。

`ResolvedControl`が持つもの:

- 入力元のスロット（1P / 2P / 割り当てなし）
- 解決したレーン（設定対象モード、レーン、側、プレイ用 / 選曲用のスクラッチ方向）
- E1〜E4、ショートカットの集合
- キーボードの既定操作（`StandardKey`: Enter・矢印・Escapeなど）

`ResolvedInput`は加えて、`InputSource`（DeviceIdと物理入力）、Press / Release、repeat、時刻、アナログ由来かを持つ。

`SceneInputResolver`と`PlayOptionInput`は、設定を読み込んだときに表を作る。入力ごとに設定全体を走査しない。

## 解決の規則

- 表の各行は`BindingDevice`（Keyboard / AnyController / ControllerSlot(1P|2P)）を持つ。行は、入力元のスロットと一致する場合だけ候補にする。同じ物理入力に側専用の行と共通の行があれば、側専用の行を優先する。
- 7K / 14Kの選曲入力では、機器ごとに次の順で表を調べる（決定カードで確認済み）。
  - キーボード: 7K → 14Kの2P側（7Kに同じキーがあれば7Kを優先する）
  - 1Pのコントローラー: 7K → 14K
  - 2Pのコントローラー: 14K → 7K
  - 割り当てのないコントローラー: 7K → 14K。共通の`gamepad`行だけが一致する
- 7Kへのフォールバックは、同じイベント・同じ機器で行う。もう一方の側の同名ボタンは候補にしない。
- 2PのKey8〜14は`ResolvedLane::side_key`で各側の1〜7へ写像し、`side`を保持する。9KのKEY8 / KEY9は2P側にしない。
- プレイ中は、開始時の設定対象モード（BATTLE、SP / DP変換を含む）とスロットで作った`PlayOptionInput`で解決する。
- アナログtickは`resolve_axis_tick`で、`AxisN+` / `AxisN-`のボタンとしてtickごとに機器と方向を解決してから合計する。割り当てのない軸は`None`。アナログ由来の押下（`synthesized_analog_axis`）は選曲移動に使わず、デジタル化した軸の押下は通常のボタンとして1回だけ移動する。

## E1〜E4の1P / 2P個別設定

- 共通の`gamepad`行に加えて、`gamepad1` / `gamepad2`の行を保存できる。
- その側の行が1件でもあれば、その側は個別設定で共通設定を置き換える。`control = ""`の行は明示的な未割り当て。
- 行がなければ共通設定を継承する（`ActionControllerBinding::{Inherit, Explicit, Unassigned}`）。
- 保存は対象スロットの行だけを置き換える。もう一方の側の行や共通の行は消さない。「デフォルトに戻す」は、その側の行を削除して継承へ戻す。
- 押下は入力元ごとに追跡し、ホールド（E1 / E2、E1〜E4の集合）は押下中の入力元をそれぞれ解決してから集約する（`select_hold_state_from_resolved`）。両側にまたがるE1 + E2も成立する。

## 押下状態の整合

- ホールド（選曲の長押し移動、IR順位表のスクロール、G-BATTLEの長押し、詳細オプションの値のリピート、レーンカバーのキー長押し）は、押したときの`InputSource`で保持する。Releaseはその入力元が離れたかで判定し、入力名では判定しない。
- 設定の保存、スロットの変更、切断、フォーカス喪失のあとは、押下中の入力元を現在の表で解決し直してホールドを作り直す（`sync_select_holds_from_pressed_controls`、`reconcile_select_holds_from_pressed_controls`）。

- 起動時の`AppInputRuntime`は、構築時に解決した1P / 2Pの割り当てで初期化する（`AppInputRuntime::with_gamepad_slots`）。以前は最初の機器更新までスロットが空のままで、すべてのコントローラーを割り当てなしとして解決していた（移行中に見つけた不具合）。

## Raw入力を使う箇所の監査

操作層で入力名を直接見ている箇所は、次の用途だけに限った。いずれもキーコンフィグの対象ではない。

| 箇所 | 用途 | 残す理由 |
| --- | --- | --- |
| `key_config_flow.rs`、`select_flow/keyboard.rs`のキーコンフィグ入力待ち | 押された物理キー・ボタン名をそのまま割り当てとして保存する | 割り当てる前の生の入力が必要 |
| `select_search.rs` | 検索欄の文字入力・IME・カーソル移動 | テキスト入力で、キーコンフィグの対象ではない |
| `lifecycle.rs`、`key_config_flow.rs`のF1 / Escape | BMZメニューの開閉と、メニュー・ポップアップを閉じる | 固定キーで、ゲーム入力より前にeguiへ渡す |
| `select_flow/keyboard.rs`冒頭のEscape Release、`select_flow/controls.rs`の`keyboard_key_held("Escape")` | 選曲の長押し終了の取り消しと継続判定 | モーダルが入力を消費してもReleaseで必ず解除するため、解決前の生のキーで見る |
| `viewer.rs` | Viewerの再生・シーク（Space / F5 / Escape / 矢印 / Home）と修飾キー | Viewer専用の固定キー。修飾キーは解決済みの`StandardKey`で見る |
| `play_support/controls.rs`の再生速度キー（1〜4 / テンキー） | 未割り当てのAutoplay・リプレイ再生速度 | プレイの割り当てに含まれるキーは除外してから使う |
| `play_flow/practice.rs`、`audio_helpers.rs`の`AxisN+` / `AxisN-`の生成 | 軸のtickとアナログ由来の押下をプレイの割り当てで解決する | 機器付きで`PlayOptionInput` / `LaneBinding`へ渡しており、解決の一部 |
| `integration_support.rs`の`physical_key_name` | 診断・ログ用の名前 | 操作には使わない |
| マウスのボタン・ホイール、スキンのクリック | 行のクリック、ホイール移動、スキンのイベントID | キーコンフィグの対象ではなく、入力名を介さず操作（`SelectRowClickAction`、イベントID）へ直接変換している |

マウスとスキンのクリックは、今回は入力元を持つ型へは移していない。キーコンフィグの対象外で、入力名による再判定もないため、現在の経路のまま残した。

## テスト

`app/input_resolve/tests.rs`に、OSのbackendなしで`ControlInputEvent` → 押下集合 → `resolve_input` → 操作までを通すテストを置いた（`Pipeline`）。
1P / 2Pのボタン番号の一致・不一致、KEY1 / KEY2・KEY1 / KEY3の入れ替え、物理的なコントローラーの入れ替え、7Kと14KのSCRが別の軸、同じ軸で逆向き、デジタル / アナログ由来の押下、E1〜E4の共通・個別・未割り当て・保存と再読み込み、他側の鍵盤と同じ番号のEボタン、同時押しと片側の解放、片側だけ接続・未割り当て、設定変更・切断・フォーカス喪失、キーボード7K優先、9K、を扱う。

WinitAppを作るテストはWindows専用で`#[ignore]`のため、新しいテストはWinitAppを作らない形にした。
