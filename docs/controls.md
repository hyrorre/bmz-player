# 操作方法

このドキュメントは現在の BMZ Player 実装に基づく操作一覧です。
キーコンフィグで変更できる操作は、デフォルト割り当てを前提に書いています。

## 共通

| Key | 操作 |
| --- | --- |
| F1 | 詳細設定ウィンドウを開閉 |
| F12 | スクリーンショットを保存（左上に短時間メッセージ表示。撮影フレームには写らない） |

詳細設定の「入力デバイス」では、ゲームパッドbackendを`自動選択` / `gilrs` /
`GameInput (Windowsのみ)`から選べます。既定値と自動選択はgilrsを優先し、Windowsで
gilrsを初期化できない場合はGameInputへfallbackします。backend変更は次回起動時、1P / 2Pの
コントローラ割り当て変更は次回プレイ開始時から反映されます。

## 選曲画面

### 共通キー

| Key | 操作 |
| --- | --- |
| Up / Down | カーソル移動 |
| PageUp / PageDown | ページ移動 |
| Home / End | 先頭 / 末尾へ移動 |
| Enter / Space / Right | 決定、フォルダを開く、曲を開始 |
| Left | フォルダを閉じる |
| / | 検索モードを開始。E1/E2/E3/E4 hold 中は通常キー入力として扱う |
| F5 | フォルダ内の BMS 再スキャン / 難易度表の再取得 |
| F8 | favorite song を登録 / 解除 |
| F9 | favorite chart を登録 / 解除 |
| Numpad8 | 選択中譜面と同じフォルダを開く |
| マウスホイール | カーソル移動 |
| 選択中の行をクリック | 決定、フォルダを開く、曲を開始 |
| 未選択の行をクリック | その行へカーソル移動 |
| 右クリック | フォルダを閉じる |

`SELECT INPUT` は設定フォルダの `INPUT` から `7K/14K` または `9K` を選べます。
デフォルトは `7K/14K` です。
設定フォルダ内では検索モードには入りません。
E1 / E2 / E1+E2 を hold している間は、選曲オプションパネルを表示します。
`RANDOM SELECT` 行は設定フォルダの `選曲 > RANDOM SELECT` から表示を切り替えられます。
favorite 操作は invisible を使わず、登録済みなら解除、未登録なら登録します。

### 選曲画面 7K/14K

| Key | 通常 | E1 hold | E2 hold | E1+E2 hold |
| --- | --- | --- | --- | --- |
| KEY1 | 決定 / 開く / 曲開始 | 1P RANDOM 次 | - | BGA 切替 |
| KEY2 | 戻る / 閉じる | 1P RANDOM 前 | - | GAUGE AUTO SHIFT 切替 |
| KEY3 | 決定 / 開く / 曲開始 | GAUGE 次 | - | JUDGE AUTO ADJUST 切替 |
| KEY4 | 戻る / 閉じる | GAUGE 前 | - | - |
| KEY5 | 決定 / 開く / 曲開始 | HS-FIX 次 | ASSIST 次 | VISUAL OFFSET -1 ms |
| KEY6 | 戻る / 閉じる | DP OPTION 次 | - | - |
| KEY7 | 決定 / 開く / 曲開始 | AUTOPLAY 切替 | - | VISUAL OFFSET +1 ms |
| 2P KEY1 | 決定 / 開く / 曲開始 | 2P RANDOM 次 | - | BGA 切替 |
| 2P KEY2 | 戻る / 閉じる | 2P RANDOM 前 | - | GAUGE AUTO SHIFT 切替 |
| 2P KEY3 | 決定 / 開く / 曲開始 | GAUGE 次 | - | JUDGE AUTO ADJUST 切替 |
| 2P KEY4 | 戻る / 閉じる | GAUGE 前 | - | - |
| 2P KEY5 | 決定 / 開く / 曲開始 | HS-FIX 次 | - | VISUAL OFFSET -1 ms |
| 2P KEY6 | 戻る / 閉じる | DP OPTION 次 | - | - |
| 2P KEY7 | 決定 / 開く / 曲開始 | AUTOPLAY 切替 | - | VISUAL OFFSET +1 ms |
| Scratch Up | カーソル上 | TARGET 前 | - | - |
| Scratch Down | カーソル下 | TARGET 次 | - | - |
| Up / Down | カーソル移動 | TARGET 前 / 次 | - | - |

### 選曲画面 9K

| Key | 通常 | E1 hold | E2 hold | E1+E2 hold |
| --- | --- | --- | --- | --- |
| KEY1 | - | 1P RANDOM 次 | - | BGA 切替 |
| KEY2 | - | 1P RANDOM 前 | - | GAUGE AUTO SHIFT 切替 |
| KEY3 | 戻る / 閉じる | GAUGE 次 | ASSIST 次 | JUDGE AUTO ADJUST 切替 |
| KEY4 | カーソル下 | GAUGE 前 | - | - |
| KEY5 | 決定 / 開く / 曲開始 | HS-FIX 次 | - | VISUAL OFFSET -1 ms |
| KEY6 | カーソル上 | DP OPTION 次 | - | - |
| KEY7 | 決定 / 開く / 曲開始 | AUTOPLAY 切替 | - | VISUAL OFFSET +1 ms |
| KEY8 | - | TARGET 前 | - | - |
| KEY9 | - | TARGET 次 | - | - |
| Up / Down | カーソル移動 | TARGET 前 / 次 | - | - |

9K では、プレイ鍵盤とデフォルト UI 操作が同じキーに割り当てられている場合、選曲操作は 9K 側の意味を優先します。

## 決定画面

| Key | 操作 |
| --- | --- |
| Enter / Space / 選曲画面の決定キー | プレイへ進む |
| Escape / E1+E2 / E2+E3 | 選曲へ戻る |

## プレイ画面

### デフォルトプレイキー

| Mode | Scratch | Keys |
| --- | --- | --- |
| 7K | LShift / LControl | Z, S, X, D, C, F, V |
| 14K | LShift / LControl, RShift / RControl | Z, S, X, D, C, F, V, M, K, Comma, L, Period, Semicolon, Slash |
| 9K | - | Z, S, X, D, C, F, V, G, B |

### ゲームパッド (10K / 14K)

2 台のコントローラで 10K / 14K をプレイできます。

| 論理スロット | 既定の役割 | デフォルト binding |
| --- | --- | --- |
| `gamepad1` | 1P (Scratch + Key1–7) | 接続順の 1 台目 |
| `gamepad2` | 2P (Scratch2 + Key8–14) | 接続順の 2 台目 |

- 未割当時は **接続順フォールバック** (1 台目 = 1P、2 台目 = 2P) です。
- 1P / 2P の物理パッド割り当ては F1 → 本体設定 → **入力デバイス** から変更できます (接続一覧・自動割り当て・入れ替え)。
- キー設定 (`設定 > キー設定 > 10K/14K`) の CONTROLLER スロットは、1P レーンが `gamepad1`、2P レーンが `gamepad2` として保存されます。
- 7K など単一パッドモードの CONTROLLER は `gamepad` ワイルドカード (どのパッドでも可) です。
- 割り当て変更は **次回プレイ開始から** 反映されます。

10K は 14K の binding を継承し、両側 5 鍵 + 両皿だけが有効です。

### プレイ中操作

| Key | 操作 |
| --- | --- |
| Left / Right | ハイスピードを HS MODE ごとの設定刻みで下げる / 上げる (NHS 既定 0.25、FHS 既定 0.50) |
| Up / Down | レーンカバー表示中はカバー位置、非表示中は LIFT を調整 |
| E1 hold + KEY1/3/5/7 | HS MODE ごとの設定刻みでハイスピードを下げる。9K では KEY1/3/5 |
| E1 hold + KEY2/4/6 | HS MODE ごとの設定刻みでハイスピードを上げる |
| E1 hold + E2 | HS MODE を切替 |
| E1 hold + Scratch Up/Down | レーンカバーを上げる / 下げる |
| E2 hold + KEY1/3/5/7 | 緑数字を下げる。9K では KEY1/3/5 |
| E2 hold + KEY2/4/6 | 緑数字を上げる |
| E2 hold + Scratch Up/Down | 緑数字を上げる / 下げる |
| E1 double press | レーンカバー表示を切替 |
| Escape | プレイを中断して選曲へ戻る |
| E1+E2 hold | 一定時間長押しでプレイを中断 |
| E2+E3 | 即時にプレイを中断 |
| FAILED 演出中に E1 | リザルトへ進まず別配置でクイックリトライ |
| FAILED 演出中に E2 | リザルトへ進まず同配置でクイックリトライ |

コースの `NoSpeed` 制約中は、ハイスピード変更とハイスピードへ影響するレーンカバー操作が無効になります。
クイックリトライは単曲の通常プレイでのみ有効です。

## リザルト画面

対応スキンでは、`GRAPH DATA` / `INTERNET RANKING` タブをマウスでクリックしてパネルを直接切り替えられます。
Favoriteボタンは現在の譜面をfavorite chartへ追加 / 解除します。BMZはInvisible状態へ切り替えません。

### 単曲リザルト

| Key | 操作 |
| --- | --- |
| R | 同配置でリトライ |
| Enter / Escape | 選曲へ戻る |
| KEY1-KEY4 | 選曲へ戻る |
| KEY5 | 選曲へ戻る / 終了アニメーション後に押されていた場合、別配置でリトライ |
| KEY6 | ゲージグラフ種別を切替 |
| KEY7 | 選曲へ戻る / 終了アニメーション後に押されていた場合、同配置でリトライ |
| E2 / SELECT | 対応スキンでIRパネルとグラフパネルを切替。非対応時のSELECTは従来どおり選曲へ戻る |
| 1 / 2 / 3 / 4 | リプレイをスロット 1 / 2 / 3 / 4 に保存 |

KEY5 と KEY7 を両方押している場合は、同配置リトライを優先します。

### コース曲間リザルト

| Key | 操作 |
| --- | --- |
| R / Enter / Escape | 次の曲へ進む |
| KEY1-KEY5 | 次の曲へ進む |
| KEY6 | ゲージグラフ種別を切替 |
| KEY7 | 次の曲へ進む |
| E2 / SELECT | 対応スキンでIRパネルとグラフパネルを切替。非対応時のSELECTは従来どおり次の曲へ進む |
| 1 / 2 / 3 / 4 | リプレイをスロット 1 / 2 / 3 / 4 に保存 |

コース曲間リザルトではリトライは行いません。

### コース最終リザルト

| Key | 操作 |
| --- | --- |
| R | コース全体を同配置でリトライ |
| Enter / Escape | 選曲へ戻る |
| KEY1-KEY4 | 選曲へ戻る |
| KEY5 | 選曲へ戻る / 終了アニメーション後に押されていた場合、別配置でリトライ |
| KEY6 | ゲージグラフ種別を切替 |
| KEY7 | 選曲へ戻る / 終了アニメーション後に押されていた場合、同配置でリトライ |
| E2 / SELECT | 対応スキンでIRパネルとグラフパネルを切替。非対応時のSELECTは従来どおり選曲へ戻る |
| 1 / 2 / 3 / 4 | リプレイをスロット 1 / 2 / 3 / 4 に保存 |
