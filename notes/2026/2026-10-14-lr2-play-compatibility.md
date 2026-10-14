# LR2 PlayのSRC/DST・分岐・フレームinclude・数値セル

2026-10-14。EndlessCirculationの難易度背景・重複ラベル・余分なスコア、
OVER ACTiVEのフレーム欠落を、実際のCSVと保存済みスキン設定、LR2参照ソースで調査した。
第三者製CSV・画像・設定・DBは編集せず、BMZの変換・描画・cacheを修正する。

## 原因と変更

- 全SRCが共有していたcurrentを命令の種類ごとに保持した。
  LINEのDST受理状態も更新し、小節線や判定画像が通常画像のDSTへ混入する経路を防いだ。
- DST前の条件付きSRCは、それ以前のSRCを置き換える。
  `lr2Not`の条件木で後続SRCが成立する場合の非表示を表し、複合ELSEも失わない。
- Play開始時に固定する状態が渡されている場合は、その値でIFを確定する。
  通常用・AUTO用のincludeを同時に読み込まず、参照した状態をcache依存へ記録する。
  Replayは通常用の32/33状態を維持する。LR2の最内側IF規則は維持した。
  Resultの固定条件・ゲージ条件も確定し、RED_BELTの条件付きIMAGEが
  二重登録されることによる画像番号のずれを回帰テストで防いだ。
- OAのASSAULTプリセットはFRAME MOTIONをRANDOMへ上書きする。
  TYPE-*.csvを抽選し、同一ロードの解像度推定と本変換で選択を共有する。
  抽選文書は再ロードで構築し直し、画像・textureは既存cacheを利用する。
  解像度推定にもSETOPTIONを反映し、上書き値を元の選択値のcache依存と混同しない。
- 数値はLR2専用属性で連続10/11/24セルを評価する。
  符号付き素材の半分をdivxではなく12セルとして扱い、横並びのアニメーションも更新する。
  LR2の空き桁・揃え・桁あふれ・負数非表示を適用し、JSON/Luaの既定値は維持する。

## 検証

小節線・判定・画像・数字の交互宣言、既定SRCの置換、複合ELSE、通常/AUTOのinclude、
CUSTOMFILE、wildcard抽選と基準解像度、modeによるcache無効化、符号付き4レイアウト、
数字のcycle・cache・揃え・桁あふれについて回帰テストを追加した。

### 実素材とGPU

8系統61エントリ（OA_DX+ 17、LR2 13、EndlessCirculation 10、WMIX_HD 9、
WMII_FHD_LR2 6、KCOOL 3、3R 2、PLATINUM 1）を再変換した。
変換エラー・変換警告は0件、通常UI destinationへの小節線混入は20エントリから0になった。
SelfEvolutionの保存設定ではref 93100のスコア定義が1つになり、上部の(x437,y642)だけが残った。
OAの保存ASSAULT設定ではframe_motionのincludeが解決され、フレーム部品を読み込んだ。

Windows / wgpu、1280×720、目標60 FPS、専用の無音プロファイルを使用した。
同梱サンプル譜面を検証用ディレクトリへコピーし、難易度ANOTHER・レベル11に変更した。
`--skip-decide --smoke-exit-after-frames 600 --smoke-screenshot <path>`で撮影し、画像を確認した。

| ケース | GPUで確認した結果 |
|---|---|
| SelfEvolution・通常 | レベル背景の灰色矩形が消え、BPM付近にスコアが重ならない |
| AC1P Wide・通常 | ANOTHERだけを表示し、既定BEGINNERと重ならない |
| OA AC1P・ASSAULT / RANDOM | レーン・鍵盤・ゲージ・スコア周囲のフレームが表示される |
| SelfEvolution・AUTO | AUTO用の配置に切り替わり、下部スコアとBPMが重ならない |

GPU起動時には外部パッケージ内に存在しないfont/05、load動画、disablelane画像、
mascot/OFFの読込警告が残る。これらの素材欠落は今回の変換不具合と分けて扱う。
全カスタム設定、左右・DP・9KのGPU画面、実際のReplay、全遷移、macOS / Linuxは未確認。
符号付き数値の4配置とアニメーションは描画項目・UVの自動テストで確認した。

### 自動検証

- `cargo fmt --check`、`cargo check --workspace --locked --offline` 成功。
- `CARGO_INCREMENTAL=0 cargo clippy --workspace --all-targets --locked --offline -- -D warnings` 成功。
  Windowsのincremental cacheでhard linkが拒否される環境のため、Clippyはincrementalを無効にした。
- `cargo test --workspace --locked --offline --no-fail-fast` 成功。
  主な件数はbmz-player 2436、bmz-render 739、bmz-skin 284、bmz-skin-document 22。
  その後に追加したResult固定条件のテスト1件も個別実行で成功した。
  サンドボックスでは一時ファイル・ローカル通信が拒否されたため、全体テストはホストで実行した。
- RED_BELTのAAA / AA実素材テストも成功。第三者製素材がある実行結果と、
  素材がなくても動く合成回帰テストを区別して確認した。

生ログと設定スナップショットは`.local/lr2-play-audit/`に保存し、コミットしない。

現在の契約は[スキン仕様](../../docs/skin.md#lr2の条件分岐)と
[対応状況](../../docs/skin-compatibility.md)へ反映した。
