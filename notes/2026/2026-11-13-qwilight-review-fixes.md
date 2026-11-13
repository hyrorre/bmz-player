# Qwilightプレイスキン対応（PR #97）のレビュー指摘への対応

## 対象

2026-11-13、`feat/qwilight-play-skins`のレビューで挙がった10件を修正した。
前段は[終了演出とプレイ表示の補完](2026-11-13-qwilight-play-completion.md)。
現在の仕様は[skin.md](../../docs/skin.md#qwilightのプレイスキン)に反映した。

## 修正内容

1. **終了演出の待機がQwilight以外にも適用されていた。**
   フルコンボ以外のクリアでtimer 908の演出長だけ待つ処理が、スキンの形式を問わず入っていた。
   BMZ拡張`bmzPlayClearCompletion`（既定false）を追加し、Qwilight変換だけがtrueを出力する。
   判定は`Renderer::play_skin_clear_completion_ms`に集約し、実プレイと動画出力の両方で使う。
2. **終了演出の遅延画像を毎フレーム展開・転送していた。**
   描画計画にないフレームを即座に破棄していたため、コマ送りの演出では毎フレーム、
   zlib展開と最大16 MiBのRGBA転送がwindow threadで同期的に起きていた。
   直近に使ったフレームを128 MiB（表示中のフレームを含む）までGPUに残し、古いものから解放する。
   表示中の演出の次の最大4フレームは、`bmz-deferred-decode`スレッドで先に展開する。
   待機中の展開済み画像は64 MiBまで。差し替えられた画像に対する古い展開結果は`Arc`の同一性で捨てる。
   演出が1枚も表示されなくなったら、GPU画像をすべて解放する。
3. **遅延画像1枚の失敗でフレーム全体を描かなかった。**
   `prepare_deferred_images()?`は描画とeguiより前に失敗を返し、計画が変わるまで毎フレーム失敗していた。
   展開・転送に失敗した画像は失敗として記録して再試行せず、同時表示の上限を超えた画像とともに
   描画計画から除く。白い代替テクスチャで描かないためである。警告は1回だけ出す。
4. **Play終了後もスペクトラムのtapが動き続けた。**
   tapのON/OFFは`active_play`がある間しか更新されず、Resultで余韻を鳴らす出力では
   audio callbackがPCMをリングへ書き続けた。`AppAudioOutput::mark_draining`で無効にする。
5. **レーンカバー一覧のキャッシュが古くなった。**
   Qwilightの定義はオプションだけをキーにキャッシュしていたが、レーンカバーの選択肢はディスク上の画像に依存する。
   追加・削除したときと、ウィンドウがフォーカスを取り戻したとき（フォルダへ手で置いた場合）に、
   Qwilightの定義だけを捨てる。キャッシュの処理は`cached_skin_defs`へ切り出してテストした。
6. **スペクトラムの解析が競合したとき0を返していた。**
   doc commentどおり直前の解析結果を返すよう、結果をatomicにも保持する。
   リセット時はこれも0に戻し、リセット前の音を表示しない。
7. **スキン一覧の走査で無関係なYAMLまで読んでいた。**
   QwilightはYAMLと同じフォルダのZIPを読むため、ZIPがあるフォルダのYAMLだけを候補にする。
8. **合成用シェーダーを文字列置換で作っていた。**
   `IMAGE_SHADER`に`fs_composite`を追加し、CanvasCompositeのpipelineはそのentry pointを使う。
9. **rustfmtが整形しなかったブロックを手で整形した。**
   長い`tracing::warn!`があるとclosure全体が整形されないため、マクロ呼び出しを複数行に分けた。
   変換コードの`json!`内の長い行は、既存の書き方に合わせてそのままにした。
10. **ZIPのentry名に`\`があると書庫全体を拒否していた。**
    `\`区切りは`/`に正規化し、親移動・絶対パス・ドライブ指定のentryは一覧から除く。
    要求名は検証済みのため、traversalのentryには一致しない。
    このPCのQwilightスキンZIP 32個（`Assets/UI`、`SavesDir/UI`、`SavesDir (beta)/UI`）には
    `\`・`:`を含むentryはなかった。今回の変更は他の作成ツールへの備えである。

## 検証

検証環境はWindows。以下は成功した。

- `cargo fmt --all --check`、`cargo check --workspace --all-targets --locked`。
- `cargo clippy --workspace --all-targets --locked -- -D warnings`。
- `cargo test --workspace --locked --no-fail-fast -- --test-threads=4`。
- `cargo test -p bmz-render --lib qwilight_deferred_gpu --locked -- --ignored`：
  GPUの再利用、破損したフレームを除いた描画、CanvasComposite 13種類のalpha、IDの再利用と解放を検査。
- `cargo test -p bmz-player --lib draining_play_audio_stops_the_spectrum_tap --locked -- --ignored`：
  既定の音声出力を開き、余韻の出力でtapが止まることを確認。
- `cargo test -p bmz-skin --lib installed_qwilight_play_skins --locked -- --ignored`：成功。
- `cargo test -p bmz-player --lib qwilight --locked -- --include-ignored --test-threads=1`：30成功。
  実素材のオフスクリーンGPU描画で、新しい遅延画像の経路を通るFULL COMBO / PERFECT / CLEARの演出を含む。
  CR_minimalのFULL COMBOの出力画像を目視した。

実素材テストは`BMZ_QWILIGHT_SKIN_ROOT`を`G:/SteamLibrary/steamapps/common/Qwilight/SavesDir (beta)/UI`へ設定した。
出力画像とログはGit管理外の`.local/qwilight-review-previews/`と`.local/qwilight-review-player.log`に保存した。

追加した自動テスト：終了演出の待機の対象（bmz-render）、次フレームの順序・LRU解放・展開スレッドの
古い結果の破棄・入れ子の描画からの除外（bmz-render）、競合時の解析結果（bmz-audio）、
`\`区切りと不正entryを含むZIP（bmz-skin-assets）、ZIPのないYAMLの除外とレーンカバーのキャッシュ（bmz-player）。

実ウィンドウでのプレイ、終了演出のフレーム時間の計測、macOS / Linuxは未確認。
先読みの効果は条件を揃えた計測をしていないため、性能改善の数値はない。

## 同日の追加レビュー対応

`447dd2b4`までをmainのmerge base `b31b83b5`と比較したレビューで、追加の3件を確認した。

- 進行バー・判定比率の画像は`cycle=0`でも状態に依存する。Playで使う静的画像キャッシュから
  `qwilight_animation`のある画像を除き、開始時に描画が空でも、その後の値を反映する。
- CR_minimal / QMAX_Humanoidの`Hide Turntable`は幅0のdrawingへ切り替える。
  レーン幅の検証で0を許可し、非表示レーンも配列に残してノーツと入力timerの対応を保つ。
  負の幅・4096を超える幅は引き続き拒否する。
- フレーム寸法が違う画像列は、各画像を最大寸法のセル全体へ拡大してからatlasへ配置する。
  同じ寸法の画像はそのまま使う。QMAXの180×1446 / 170×1800の組合せでも、
  押下画像を左上へ詰めた結果として右側に余白が生じないようにする。

3件とも自動回帰テストを追加し、修正前のコードで失敗することを確認した。
実素材を使わないテストでは、同じSkinContextを通した進捗0→50%→75%と判定比率の変化、
SP / DPの非表示スクラッチと入力timerの対応、横長・縦長フレームを含むZIPの展開後RGBAを検査する。
実素材用には、2スキン×5K / 7K / 10K / 14K×Show / Hideの定義読み込みと、
CR_minimalの7KでShow / Hideを比較するGPUオフスクリーンテストを追加した。

QMAX_Humanoid全体のGPUテストも試したが、既存の判定画像列（1020×300、120フレーム、
atlasの11×11セルで約141 MiB）が128 MiB上限を超え、6種類の判定素材の読み込みで失敗した。
この制限は今回の3件とは別であり、上限の緩和や失敗を許容する変更はしていない。
QMAX_HumanoidはShow / Hideの定義読み込み、画像列の縮小問題は独立したZIP fixtureのRGBAで検証する。

仕様は[スキン仕様](../../docs/skin.md#qwilightのプレイスキン)と
[互換対応状況](../../docs/skin-compatibility.md)へ反映した。

追加修正の検証結果（Windows）：

- `cargo fmt --check`、`cargo check --workspace --locked`、
  `cargo clippy --workspace --all-targets --locked -- -D warnings`、
  `cargo build -p bmz-player --locked`：成功。
- `cargo test --workspace --locked --no-fail-fast -- --test-threads=4`を実行。
  制限された実行環境ではbmz-playerの139件とbmz-renderの3件が通信・ファイル操作等で失敗した。
  通常環境で失敗したテストだけを再実行し、139件・3件とも成功した。
  bmz-playerの更新ダウンロード検証1件は制限環境でローカルサーバーの受信待ちが残ったため、
  自分のテストプロセスの実行ファイル・引数・親PIDを確認して終了し、その1件も通常環境で成功した。
  ログのテスト名を集計して、bmz-playerの全3206件が3122成功・84 ignoredとして確認できることを検査した。
  他crateとdoc testは完走し、bmz-renderは再実行分を合わせ1037成功・25 ignored。
- 実素材のShow / Hide定義読み込み：CR_minimal / QMAX_Humanoidの全16条件で成功。
- Qwilight関連の実素材GPUを含む31件、およびCR_minimalのShow / Hide GPU比較1件が成功。
  出力画像も目視確認した。実ウィンドウでのプレイとmacOS / Linuxは未確認。

追加検証のログはGit管理外の`.local/qwilight-followup-*.log`、
`.local/qwilight-review-regressions-{before,after}.log`、`.local/qwilight-hidden-*.log`、
画像は`.local/qwilight-review-followup-previews/`に保存した。

## 同日の追加対応：保持フレームのatlasセル共有

上記で残したQMAX_Humanoidの判定画像列の上限超過は、上限が小さいためではなく、重複した画像が原因だった。
判定（`judgment-paint-frame: 120`）の実画像は各判定11枚（J 3は9枚）である。
フェードイン0〜5とフェードアウト115〜119だけがあり、6〜114の欠番はフレーム5を保持する。
変換は120フレーム分のパスを11×11のatlasへ並べていた。1セルは1020×300 RGBAなので、約141 MiBの大半がフレーム5の複製だった。

- 変換（`Builder::image`）で重複するパスを除き、異なる画像だけでatlasを作る。
  フレームからセルへの対応を画像の`qwilightFrameCells`に持たせる。
  `qwilightAnimation`は後から各モードで上書きされるため、対応は画像側の独立した項目に置いた。
- 描画（`state_values/image.rs`）は、論理フレーム数を対応表の長さとして扱う。
  算出したフレーム番号を対応表でセルへ変換する。表が空の画像（既存JSON / Lua / LR2を含む）は従来どおり。
- atlasの組み立て（`load_frame_sequence`）は、同じパスを1回だけデコードする。
- QMAX_Humanoidの判定は各12セル以下（約14 MiB）になり、128 MiBの上限は変更していない。
  見た目は欠番の保持と同じで、PNGのデコードも判定1種類あたり121回から11回に減る。

上限を上げる案は採らなかった。判定だけでGPUに約850 MiBを使い、atlas幅11,220pxが一部GPUの8192px上限を超えるためである。
終了演出の遅延転送を使う案も採らなかった。判定は常時表示され、6種類で同時表示の予算を取り合うためである。

既存のテスト2件は、101個の符号付きレーン別FAST/SLOWフレームが1枚の画像を保持するfixtureだった。
セル共有で1セルになり、意図していた「素数フレーム数の配置」と「フレームごとの選択」を検査できなくなった。
fixtureへ-50〜50の異なる画像を追加し、元の意図のまま検査する。
Hide Turntableの修正で追加した寸法の異なる画像列のテストも、保持フレームが共有セルになるため期待値を2セルへ更新した。

検証（Windows）：

- `cargo fmt --all --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings`：成功。
- `cargo test --workspace --locked --no-fail-fast -- --test-threads=4`：成功（失敗0件）。
- 追加テスト：保持フレームのセル選択（bmz-render）、QMAX形式の判定fixtureのセル数・対応表・atlasのRGBA（bmz-player）。
- 実素材：`installed_qmax_judgments_store_each_held_image_once`を追加し、7判定すべてが120フレーム・12セル以下で成功。
  `qwilight_installed_play_skins_gpu`にQMAX_Humanoidの7Kを加え、全ソースのデコードと描画が成功した。出力画像で判定文字を目視確認した。
- `cargo test -p bmz-player --lib qwilight --locked -- --include-ignored --test-threads=1`：fixture更新後に33件成功。
- 実ウィンドウでのプレイ、GPUメモリ使用量の実測、macOS / Linuxは未確認。

ログと画像はGit管理外の`.local/qwilight-dedup-*.log`と`.local/qwilight-dedup-previews/`に保存した。
