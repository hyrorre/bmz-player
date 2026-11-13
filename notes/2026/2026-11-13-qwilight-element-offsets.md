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

## 同日の追加対応：装飾の追従と装飾ごとの調整

レビューで、CR_minimalの「Judgment counts」を動かすと数字だけが動き、`Y:` `C:`等のラベルが残ることを確認した。
ラベルは`Drawing/P 3`（paintProperty 3）に描かれ、同じ画像にFAST・SLOW・RATEのラベルも含まれる。
判定数の調整だけにラベルを付けると他のラベルまで動くため、次の2つを追加した。

- 9番目の値（pipelineヒント）で要素に紐付いた装飾は、その要素の調整に追従する。
  CR_minimalでは、Timing（43）・NPS（44）・判定線（31）に紐付いた装飾が該当する。
  要素の未調整の領域を変換時に記録し、横方向に重なる領域（DPの側・またぐレーン）、なければ最も近い領域を基準に、
  要素と同じ倍率で拡縮・移動する。装飾が要素の領域を参照できるよう、paintPropertyの変換を要素の後へ移した。
  描画順は`lambda.pipeline`で決まるため変わらない。
- 装飾ごとの`Qwilight Decoration <n> offset`（ID 1200+n、「Qwilight decorations」）を追加した。
  YAMLにある`paintProperty<n>`だけを表示し、装飾の中心を基準に、要素への追従の後へ加える。
  CR_minimalでは4項目と`Qwilight Decoration 3 offset`を同じx/yにすると、ラベルと数値を揃えて動かせる。

検証（Windows）：

- 合成fixtureで、NPSに紐付いた装飾がNPSの拡縮・移動・不透明度に追従すること、
  紐付けのない装飾は要素の調整で動かず自身の調整だけで動くこと、項目のID・名前・分類を確認した。
- 実素材のGPU描画`qwilight_installed_decoration_offsets_move_shared_labels_gpu`を追加した。
  CR_minimal 6Kで5項目を同じx/yにずらし、ラベル領域の95%以上の画素がそのまま平行移動することを確認した。
  出力画像もGit管理外の`.local/decoration-previews/`で目視した。
- `cargo fmt --all --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、
  `cargo test --workspace --locked --no-fail-fast -- --test-threads=4`が成功。
- Qwilight関連の実素材・GPUテスト40件と、判定線とノーツの重なり順の実素材GPUテストが成功。
  paintPropertyの変換順を移した後も、CR_minimalの判定線バー（`qw-paint-10`）はノーツの背後に描かれる。
- 実ウィンドウでの設定操作とmacOS / Linuxは未確認。
