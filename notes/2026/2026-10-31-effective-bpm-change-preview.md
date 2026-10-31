# BPM × SCROLLとSTOPによるBPM変化予告

実施日: 2026-10-31。作業ブランチ: `codex/effective-bpm-change-preview`。
本体の開始点: `7eec6619`（HS-FIX変更のマージ後）。mz-selectの変更commit: `6151f30`。
対象: BMZ予告ref 19060〜19069と、同梱mz-select / antiqueのBMZ専用予告表示。
仕様は[拡張ref](../../docs/skin.md#bmz-bpm-change-preview-refs)と[スキン設定](../../docs/settings.md)を参照。

## 確定した条件

- 予告値は`BPM × SCROLL`。SPEEDの予告は区間の扱いを別途検討するため含めない。
- SCROLLが0・負でも対象に含め、絶対値に変換しない。
- 四捨五入前の差の絶対値が1未満なら予告しない。ちょうど1は対象に含める。
- 加速・減速の色分けは従来と同じ大小比較を使う。数値が増えたら青、減ったら赤。
- STOP中は0。停止開始と解除の両方を予告する。

例えば`150 → 0`は赤、`0 → 150`は青、`0 → -150`は赤、`-150 → 0`は青になる。

## 本体側

`screens/play_snapshot/bpm_change.rs`でBPM・SCROLL・STOP境界を統合する。
同じ時刻の更新をすべて適用してから前後の積を比較するため、BPMとSCROLLが相殺される変化や、
連続するSTOPの途中で予告が発生しない。微小な変化も内部状態には反映し、直前の実値との差で判定する。

STOPの区間はGameSessionと同じTimingMapから得る。
SCROLLの切替時刻は描画カーソルがイベントtickに達する時刻とし、legacyなevent.time未設定譜面にも対応する。
表示オフセット、予告までの残り時間と変化後の経過時間の再生速度換算は既存経路を使う。

予告snapshotで表現できない非有限値は公開しない。
変化率ref 19063は負の変化前値も扱い、変化前が0のときは未定義（画像非表示、Luaでは0）とする。

## 同梱antique

既存の符号なし数字画像は絶対値を描くため、初回実装では負号を同じ色のテキストとして別途描いた。
2026-11-01に、下記の矩形表示へ変更した。
符号の有無で表示を排他的に切り替え、レーン内中央・レーン外の左右配置に負号1桁分の幅を含める。
既存の数字画像と色、予告時間、位置・サイズ・透明度の設定を利用する。

変更箇所はユーザー所有のmz-select forkにある既存BMZ専用拡張。
`docs/licenses.md`とmz-selectのreadmeで改変・再配布条件を確認した。画像やフォント資産の追加はない。

## 検証対象と制限

- 本体: SCROLL単独、BPMとの同時変更、0・負値、差が1の境界、STOP開始・解除、連続STOP、SPEED補間の除外、レーン時刻との同期。
- ref: 符号付きの数値・変化率、0の分母、数値の大小による色分け。
- 実際のantique: 0・負値・両方負のケースで負号と矢印が一度ずつ描かれること、数字の桁数、既存の配置・オプション。
- GPU: TYPE A〜Dとレーン外左右の42ケースをオフスクリーン描画し、色・表示期間・配置を検証。保存した負値とSTOPの画像も確認した。

確認用画像は`.local/performance/effective-bpm-preview/`に保存し、Gitには含めない。
GUIでの実譜面プレイ、物理入力、macOS / Linuxでの実行は未実施。

## 検証結果

Windowsで以下が成功した。

- `cargo fmt --check`
- `cargo check --workspace --locked`
- `cargo clippy --workspace --all-targets --locked -- -D warnings`
- `cargo test --workspace --locked --no-fail-fast -- --quiet`
  - bmz-player: 2,769件成功、47件ignored。
  - bmz-render: 981件成功、11件ignored。他のcrateとdoc-testも失敗なし。
- BPM予告関連の対象テスト: bmz-player 15件、bmz-render 4件成功。
- 実際のantiqueの表示テスト: 8件成功。既定でignoredのGPUテストは別途明示実行し、1件（42描画ケース）成功。

全体テストのログは`.local/performance/effective-bpm-preview/workspace-tests.log`に保存した。

## 2026-11-01: antiqueの負号を短い矩形へ変更

作業ブランチは`codex/antique-preview-minus-bar`。開始点は本体`8ee7c425`、mz-select `6151f30`。
mz-selectの修正commitは`28301aa`。

初回の負号は、13pxの文字枠に矢印用の32pxフォントと縁取りを使ったため、横に長く、数字に接していた。
既存の差分スコア用負号も確認した。`number_common_s.png`の`x=143, y=39`にある13×13pxのセルで、
実際の横棒は11×3px、左右各1px・上下各5pxの透明な余白を持つ。色は`#FFCCCC`。

太さ3pxを参考に、予告の負号は7×3pxの矩形へ変更する。
13pxの文字枠とレイアウトを保ち、横棒を中央に置いて左右各3px・上下各5pxの余白を確保する。
描画は既存の白矩形ID `-111`を使い、数字と同じ青／赤を設定する。フォント・縁取り・追加画像は使わない。
数字のw/h設定に比例して横棒も拡縮し、最小寸法は1px。位置・LIFT・判定位置・透明度・予告時間は既存設定に従う。

既存の実スキン表示テストを更新し、負号の数・寸法・色・数字との3pxの間隔・垂直中央揃えを確認する。
拡縮テストでは14×6pxへの倍拡大と、位置・透明度の設定を確認する。
GPU描画の出力先は`.local/performance/antique-preview-minus-bar/`。

Windowsで以下が成功した。

- `cargo fmt --check`
- `cargo check -p bmz-player --locked`
- `cargo clippy -p bmz-player --all-targets --locked -- -D warnings`
- `cargo test -p bmz-player --locked -- --quiet`: 2,769件成功、47件ignored。
- 実際のantiqueを使う対象テスト8件、および明示実行したGPUテスト1件（42描画ケース）。

修正後のTYPE Aとレーン外配置の画像でも、負号が数字から離れていることを確認した。
全体テストのログは`.local/performance/antique-preview-minus-bar-tests.log`。
実譜面の手動プレイとmacOS / Linuxでの確認は未実施。
