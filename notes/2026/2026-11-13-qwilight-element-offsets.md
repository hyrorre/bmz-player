# Qwilightプレイスキンの要素別オフセット

## 対象

判定・数値・曲情報・ゲージ・BGA・ビジュアライザー・進行表示・レーン別エフェクトへ、
BMZ独自のx/y/w/h/a調整を追加する。判定線はyを除外しLIFTに統一する。
ユーザー指定により、フルコン/PERFECT/CLEAR演出、AUTO PLAY、背景・フレーム・装飾画像は対象外。

## 実装判断

- 既存のQwilight互換7項目はそのまま維持する。新しい34項目は`Qwilight elements`へ分類し、
  配置定義のある要素だけをヘッダーへ公開する。IDは1100+代表pipeline番号で保存キーは固定の英語名。
- 新しい値はスキン変換時に適用する。スキンLuaの配置と、描画時のLIFT・Judge offsetの間で補正する。
  描画スレッドへ辞書や追加の状態を持ち込まず、再読み込み・Play枠別の保存・スキン履歴を再利用する。
- 数字だけでなく小数点・単位・時間の区切り・進行バーのつまみも同じ領域で拡縮する。
  判定内訳は6行全体、DPは複製ごと、レーン別エフェクトはレーンごとの基準位置を使う。
- 文字サイズを縮めたときにフォント定義の元サイズが下限にならないよう、文字定義も更新する。
  中央揃えの曲名の幅調整はアンカーを保つ。判定線の厚さ変更は上端を保ち、保存済みyは無視する。
- 不透明度は通常255、スペクトラムは0.1を基準とする。スペクトラムの濃度を上げるため、
  schemaに省略可能な`qwilightVisualizer.opacity`を追加した。省略時の表示は変わらない。
  ms表示の終端alpha=0と素材・ランタイム側のアニメーションを維持する。
- 装飾画像に焼き込まれたFAST/SLOW等のラベルは移動しない。第三者製スキンのファイルは変更しない。

仕様は[スキン](../../docs/skin.md#qwilightのカスタマイズ)、操作は[設定](../../docs/settings.md)に反映する。

## 検証

- 合成スキンのロード・描画テスト3件が成功。34項目、除外対象、部品の拡縮、DP間隔、バーとつまみ、
  テキスト縮小、中央揃え、msの終端、不透明度、表示OFF時の定義保持を確認した。
- 初回のworkspaceテストでは、判定線を描かないfixtureにも調整欄が出る問題を検出した。
  `judgmentMainPosition`（ノーツの基準）と判定線画像の配置定義を区別するよう修正した。
- 最終差分でfmt、workspace check、workspace all-targets Clippy（`-D warnings`）、
  workspace test（`--no-fail-fast -- --test-threads=4`）、Windows buildが成功。
  playerは3,132成功/83除外、renderは1,038成功/25除外、skinは425成功/5除外。
  除外された実素材/GPUテストのうち以下は明示的に実行した。
- インストール済みCR_Core、CR_DX、CR_Fancy、CR_Mania、CR_minimal、CR_natural、CR_simple、
  Default、QMAX_Humanoid、QMAX_templateの10種類×8キー数で、既定値・全調整値ありの読み込みが成功。
- CR_minimal 6K / Default 14Kの1280×720 GPUオフスクリーン描画が成功。
  変更画素数はそれぞれ19,394 / 13,879。前後の画像で、数字と単位の拡縮、判定誤差msの移動、
  ゲージのサイズと透明度、曲名サイズ、DP両側の配置を確認した。
- 検証時に既存のincremental cacheのhard-link失敗とコピーへの切り替えwarningが出たが、
  コードのwarning抑制や無関係な修正は行っていない。
- 生ログと画像は`.local/qwilight-offset-*`へ保存する。実ウィンドウでの手動プレイ、実機入力、
  音声、macOS/Linuxは今回の確認範囲に含めない。
