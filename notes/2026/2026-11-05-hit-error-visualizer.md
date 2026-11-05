# ヒットエラービジュアライザーの座標・EMA修正

対象: `simple-play-simple`のヒットエラービジュアライザーが機能しない報告。
仕様は[skin.md](../../docs/skin.md#playのヒットエラービジュアライザー)を参照。

## 原因と変更

- `bmz-render`が正規化したdestination座標へ内部画像のピクセル幅を加えていた。
  内部画像の座標で線・三角を構築して範囲内へ切り、最後にdestinationへ拡縮する。
- `hiterrorMode=0`の早期returnが中央線・EMAまで消していた。
  ModernChicのEMA専用設定、LITONE9の同設定にも該当する。
- 未実装だったEMAの線・三角、判定色、POOR領域の透過を追加。
  減衰する履歴線は上流と同じく縦中央へ短くし、中央線とEMAを履歴線の上へ描く。
- BMZの入力差はFASTが負、beatorajaの`recentJudges`はFASTが正。
  描画時の符号をBMZのデータに合わせ、FAST左・SLOW右へ揃える。
- EMAをsceneの`DynamicTimerRuntime`に保持する。複数visualizerの`alpha`を独立させ、
  同じindexを描画フレームや複数passのたびに再取り込みしない。

## 他のスキン・描画への影響調査

共通経路を使う定義を確認した。

| スキン | 確認した定義・影響 |
|---|---|
| simple-play-simple | `customize/hiterrorvisualizer/default/parts.lua`。既定値の線とEMA |
| simple-play | `customize/extension/HitErrorVisualizer/parts.lua`。EMA三角 |
| Rmz-skin | `play.lua`。内部幅406、`lineWidth=6`（上流と同じ上限4）、既定EMA |
| ModernChic471 / Old | `Play/lua/sp/graph.lua` / `dp/graph.lua`。`hiterrorMode=0`とEMA 1〜3 |
| LITONE9 | `Play/play.lua`。履歴数30/100、線幅1/3/4、EMA 1〜3とEMA専用 |

通常画像・コンボ・判定・slider・BGAが使う正規化helperには変更を加えない。
タイミング分布、ノート分布、BPM、ゲージのグラフも正規化後に比率またはcanvasで割った
ピクセル寸法を使っており、今回と同じ内部ピクセル幅の直接加算は見つからなかった。
隣接する画像・value・横2列judge・スコアgraphの描画項目が変わらない回帰を追加した。

第三者スキンのLua・画像は変更しない。`default3`のコンボ画像が緑の帯である問題は別件で、
本変更に含めない。前の調査では公開書庫の3画像もローカルとSHA-256が一致した。

## 検証

Windowsで以下を実施した。

- `cargo fmt --check`、`cargo check --workspace --locked`、
  `cargo clippy --workspace --all-targets --locked -- -D warnings`に成功。
- `cargo test --workspace --locked --no-fail-fast`に成功。
  4,978 passed / 0 failed / 74 ignored。子プロセスが出力する内側のテスト結果は重複計上しない。
  rendererは1,001 passed / 11 ignored、playerは2,876 passed / 55 ignored。
  追加したrenderer回帰8件と実スキンのloader回帰1件を含む。
- ignoredの`simple_play_hit_error_gpu_changes_only_its_original_destination`を完全修飾名で
  明示実行し、1 passed。実スキンの元のdestinationをloader経由で取得し、
  WindowsオフスクリーンGPUの更新画素が`x=126..597, y=888..899`の範囲だけに収まることを確認。
- 最初のsandbox内workspace実行はOBSのlocalhostモック等で失敗し、中断した。
  上記の全体テストとGPUテストは通常のWindows権限で実行した結果。
- incremental cacheのhard linkが使えずcopyへfallbackする環境warningは発生した。
  Rust / Clippyの診断を抑制する変更は行っていない。

実スキンを使った検証素材は`data/skins/simple-play-simple`。
ソース参照は`.local/beatoraja/src/bms/player/beatoraja/skin/SkinHitErrorVisualizer.java`。
他スキンの上記定義は確認したが、各設定の実画面を全て比較したものではない。
実ウィンドウでの手動プレイ、beatorajaとのGPU画素比較、macOS / Linuxは未実施。
生ログは`.local/hit-error-workspace-{check,clippy-final,test-final}.log`と
`.local/hit-error-gpu.log`に保存し、Gitには含めない。
