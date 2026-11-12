# Qwilight CR_minimalの表示欠落とLIFT連動

- 作業日: 2026-11-13
- ブランチ: `feat/qwilight-play-skins`、開始時HEAD: `57aa6cdb`
- 対象: ユーザー画像と同じCR_minimalの6K。第三者素材・実行時profileは変更しない。

## 原因と変更

- `earlyValue` / `lateValue` / `judgmentVSVisualizer` / `judgmentMeter`の変換がなかった。
  BMZのFAST/SLOW合計、PGREAT対その他判定の比率、判定誤差msを元の素材で描画する。
  RATEはEXスコアレート欄とは別の比率であり、分母ゼロ時は0.00または∞。
  Qwilight素材の`JM -`はFAST、`JM +`はSLOW。符号が逆のbeatoraja参照525を使うため、
  ラベルのIDだけでなく実際の画像パスとGPU画像も確認する。
- `paintProperty`を8項目必須としてmode 2を除外していた。省略時の既定値とmode 2の終端保持へ対応する。
  左右の枠も誤ったフォルダー・配置名を参照していたため、`Main` / `mainWall*`へ修正する。
  CR_minimalが独自定義した`P1Length()`を組み込み関数で上書きしていた問題も直し、
  判定線装飾が指定する「レーン幅 + 枠の4px」をそのまま評価する。
- ゲージの`Drawing/HP 2 0.png`等のフレーム付き表記を検索していなかった。先頭フレームも解決する。
- 判定文字は2フレーム / 24 FPS ≒ 83msで打ち切られ、判定別の反復回数を無視していた。
  CR_minimalのGREAT→YELLでは48回反復、合計4秒となる。既存のrendererの判定保持を使い、
  gameplayの800ms表示履歴・採点・replayの処理は変更しない。
- 旧スキン調整1005を削除し、保存済み値を無視する。判定線・押下画像・ボム・判定/コンボ/msへ
  既存のLIFT offset 3を適用する。ノーツは既存のLIFT経路のままなので二重適用しない。

公式参照ソースはインストール版1.17.14に対応する
[`e9ef433d`](https://vcs.taehui.net/root/qwilight/-/tree/e9ef433d56a72aeba8a00a42cb5294d2c61a2ab8)の
UI.cs / DrawingSystem.cs / DefaultCompute.cs。現在の契約・制限は[スキン仕様](../../docs/skin.md#qwilightのプレイスキン)に記載する。

## 検証

- Windowsで`CARGO_INCREMENTAL=0`を設定して検証する。生ログ・生成画像は
  `.local/qwilight-visual-*`へ置き、Git管理しない。
- CPU回帰テスト: FAST/SLOW合計、RATEの0.00/有限値/∞、msの正負/0/表示抑制/終了時刻、
  背景の省略値/mode 2終端、枠、ゲージ0/20/80/100%、判定画像4秒、LIFT追従、
  旧1005の無効化、Luaの配置関数の上書きを対象にする。
- インストール済み10スキン×8キー数（通常/全調整あり）の読み込み: 成功。
  対象はCR_Core、CR_DX、CR_Fancy、CR_Mania、CR_minimal、CR_natural、CR_simple、
  Default、QMAX_Humanoid、QMAX_template。スキン側11テストが成功した。
- プレイヤー側のQwilightテストはGPUを含む14件が成功した。
  CR_minimal 6K、Default 7K、CR_DX 7K、CR_simple 14KをGPUで描画した。
  CR_DX 7KとDefault 14Kの位置調整・SUDDEN+・追加カバー画像の回帰も成功した。
- CR_minimal 6Kの最終GPU再検証: 成功。FAST 25/SLOW 25の素材、背景・枠・ゲージ・
  各数値、LIFT 15%での移動、1.1秒後のYELL保持と4.1秒後の消去を画像で確認した。
- `cargo fmt --all --check`、`cargo check --workspace --locked`、
  `cargo clippy --workspace --all-targets --locked -- -D warnings`、
  `cargo build -p bmz-player --locked`: 成功。
- `cargo test --workspace --locked --no-fail-fast`: 初回はplayerが終了コード3で途中終了した。
  またrendererの既存bitmap fontテスト3件が、一時フォルダー名の`OpenAI`/`openai`の差で失敗した。
  その他の対象は成功。playerの途中終了の原因は特定していない。
- 失敗した2対象を通常の実行環境で
  `cargo test -p bmz-player -p bmz-render --lib --locked --no-fail-fast -- --test-threads=4 --nocapture`
  により再確認し、player 3,100成功/76除外、renderer 1,018成功/23除外となった。
  除外したGPUテストのうち、上記Qwilightテストは明示的に実行済み。

GPU確認は1280×720のオフスクリーン描画であり、実ウィンドウでの手動プレイ・実機入力・音声・
macOS/Linuxの確認は行っていない。原作との全オプション組合せや画素一致も検証していない。
