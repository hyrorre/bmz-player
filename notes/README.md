# 作業記録

調査・実装・計測の経緯を、日付付きの記録としてGit管理します。
現在の仕様・操作・開発手順・対応状況は [docs](../docs/) を確認してください。
過去の記録にある「現在」「未対応」「次の候補」やテスト件数は、その記録時点の情報です。

## 置き場所

| 場所 | 内容 | Git管理 |
|---|---|---|
| 2026-10-20 | [詳細オプションの選択列を上へずらす](2026/2026-10-20-detail-option-selected-raise.md) |
| 2026-10-20 | [詳細オプションのSUDDEN+ / HIDDEN+ / LIFTを1項目にまとめる試行](2026/2026-10-20-detail-option-lane-cover.md) |
| 2026-10-20 | [詳細オプションの見た目の追加調整](2026/2026-10-20-detail-option-style-fixes.md) |
| 2026-10-19 | [詳細オプションの見た目を通常パネルに揃える](2026/2026-10-19-detail-option-normal-panel-style.md) |
| 2026-10-19 | [詳細オプションの大分類の再編と枠の色分け](2026/2026-10-19-detail-option-category-colors.md) |
| 2026-10-19 | [ECFN詳細オプションのフェード中に焼き込みOFFが透ける問題](2026/2026-10-19-ecfn-detail-value-fade.md) |
| `docs/` | 継続更新する仕様・手順・対応状況 | する |
| `notes/YYYY/` | 調査結果、実装時の判断、条件付きの計測・検証結果 | する |
| `.local/notes/` | 一時的な下書き | しない |
| `.local/performance/` 等 | 生ログ、計測出力、個別環境の素材 | しない |

これらはすべてリポジトリrootからのパスです。特記のない本文中のコードパスとコマンドもroot基準です。
外部スキン、個人の設定・DB、秘密情報は記録への添付としてもコミットしません。

## 命名と更新

- ファイル名は `YYYY-MM-DD-topic.md`、ディレクトリはその年にします。
  日付は調査・作業の開始日または対象となる計測日とし、複数日にまたがる場合は本文に実施日を残します。
- 調査・計画・実装・再計測は、一連の作業なら同じ記録へまとめます。完了しても別フォルダへ移しません。
- 対象commit、環境・条件、判断理由、主要結果、未確認事項・残課題を、必要な範囲で記載します。
  詳細ログをローカルだけに残す場合は、その保存先と共有されないことを明記します。
- 後日の結果や訂正は日付付きで追記し、当時の結果と区別します。新しい調査は別記録から関連記録へリンクできます。
- 確定した仕様や再利用する手順は `docs/` にも反映します。現在の仕様を作業記録だけに置きません。
- 新規作成時は下の索引に追加し、本文から関連する仕様書・手順書へリンクします。

過去のローカルログや一時ディレクトリが現在も存在することは保証しません。

## 2026年

| 日付 | 記録 |
|---|---|
| 2026-10-26 | [書庫資源の世代固定と容量制限のレビュー修正](2026/2026-10-26-archive-resource-generation-fixes.md) |
| 2026-10-25 | [書庫内楽曲を展開せずにメモリから再生する](2026/2026-10-25-archive-direct-playback.md) |
| 2026-10-24 | [文字atlas満杯時のスパイクの調査・行の再利用](2026/2026-10-24-text-atlas-recycling.md) |
| 2026-10-22 | [IR Webのnpmパッケージ更新（Nuxt 4.6）](2026/2026-10-22-ir-web-npm-upgrade.md) |
| 2026-10-22 | [IR WebのNode.js 26移行の影響調査](2026/2026-10-22-ir-web-node26.md) |
| 2026-10-22 | [IR Webのdev SSRでi18nキーがそのまま出る問題](2026/2026-10-22-ir-web-dev-ssr-i18n.md) |
| 2026-10-21 | [Lua runtimeイベントのレビュー指摘修正](2026/2026-10-21-lua-runtime-event-review-fixes.md) |
| 2026-10-21 | [Luaのkey_pressed・act・customEvents・音声・音量の実行時対応](2026/2026-10-21-lua-runtime-events.md) |
| 2026-10-21 | [スキン関連レビューの修正と最適化](2026/2026-10-21-skin-review-optimization.md) |
| 2026-10-21 | [LuaスキンAPIの互換性調査と拡張](2026/2026-10-21-lua-api-compat.md) |
| 2026-10-21 | [クライアント側の責務と重複の整理](2026/2026-10-21-client-refactoring.md) |
| 2026-10-21 | [ビットマップフォント文中のベクター文字を画像フォントの見た目に近づける](2026/2026-10-21-bitmap-fallback-style.md) |
| 2026-10-20 | [スキン文字の欠字表示・フォントfallback・カラー絵文字](2026/2026-10-20-unicode-emoji-text.md) |
| 2026-10-20 | [詳細オプションのスクロール補間を選曲バーへ統一](2026/2026-10-20-detail-option-scroll.md) |
| 2026-10-20 | [Luxez-Flat詳細オプションの角と継ぎ目](2026/2026-10-20-luxez-detail-corners.md) |
| 2026-10-19 | [WebP・APNG・アニメGIFの対応](2026/2026-10-19-animated-images.md) |
| 2026-10-18 | [書庫内の単一フォルダを選曲画面で1段省く](2026/2026-10-18-archive-wrapper-folder.md) |
| 2026-10-18 | [LR2レビュー以降のコミットのレビュー指摘修正](2026/2026-10-18-post-lr2-review-fixes.md) |
| 2026-10-18 | [F1メニューと設定ウィンドウの統合](2026/2026-10-18-unified-bmz-menu.md) |
| 2026-10-16 | [HTTP/IPFS譜面ダウンロードの管理・一括取得](2026/2026-10-16-chart-download-manager.md) |
| 2026-10-16 | [BMS `#TEXT` / `#SONG` / `#TEXT00` のスキン表示](2026/2026-10-16-bms-chart-text.md) |
| 2026-10-16 | [LR2スキン描画準備の最適化と比較計測](2026/2026-10-16-lr2-render-performance.md) |
| 2026-10-15 | [並列実行で不安定になるテストの調査と修正](2026/2026-10-15-parallel-test-flakiness.md) |
| 2026-10-15 | [LR2選曲カーソルの発光とBAR部品の描画順](2026/2026-10-15-lr2-select-bar-order.md) |
| 2026-10-15 | [LR2選曲スキンのIR状態表示の重複](2026/2026-10-15-lr2-select-ir-status.md) |
| 2026-10-15 | [LR2選曲スクロールバーのalpha](2026/2026-10-15-lr2-select-scroll-slider.md) |
| 2026-10-14 | [ADFX02 (EC:FN / Starseeker) 7KのBPM変化予告](2026/2026-10-14-adfx02-bpm-preview.md) |
| 2026-10-14 | [BPM変化予告の拡張refとantique実装](2026/2026-10-14-bpm-change-preview.md) |
| 2026-10-14 | [LR2 PlayのSRC/DST・分岐・フレームinclude・数値セル](2026/2026-10-14-lr2-play-compatibility.md) |
| 2026-10-14 | [LR2 ResultのCLEAR TYPE・左右切替・TARGET・DURATION](2026/2026-10-14-lr2-result-compatibility.md) |
| 2026-10-14 | [LR2スキン関連コミットのレビュー指摘の修正](2026/2026-10-14-lr2-review-fixes.md) |
| 2026-10-14 | [LR2選曲スキンのSKIN SETTINGSのマウス操作](2026/2026-10-14-lr2-skin-settings-mouse.md) |
| 2026-10-14 | [LR2選曲のASSIST（AUTO SCRATCH）とBATTLE](2026/2026-10-14-lr2-assist-auto-scratch-battle.md) |
| 2026-10-14 | [選曲バーの速度を引き継ぐHermite補間](2026/2026-10-14-select-hermite-scroll.md) |
| 2026-10-14 | [LR2選曲の未所持譜面・コースのバー画像](2026/2026-10-14-lr2-select-bar-types.md) |
| 2026-10-14 | [選曲バーの連続移動補間](2026/2026-10-14-select-continuous-scroll.md) |
| 2026-10-14 | [LR2選曲のDP表示・ホバー・スコアバー](2026/2026-10-14-lr2-select-dp-hover-score-bar.md) |
| 2026-10-14 | [Windows debug起動のスタックオーバーフロー](2026/2026-10-14-debug-startup-stack-overflow.md) |
| 2026-10-13 | [LR2選曲スキンのパネル2以降と埋め込みスキン設定](2026/2026-10-13-lr2-embedded-panels-skin-settings.md) |
| 2026-10-13 | [LR2選曲スキンへの値の供給（ゴースト・ライバル・レベルバー・履歴）](2026/2026-10-13-lr2-select-data-supply.md) |
| 2026-10-13 | [LR2選曲スキンの値のずれと条件の修正](2026/2026-10-13-lr2-select-value-fixes.md) |
| 2026-10-13 | [LR2専用のnumber / option参照](2026/2026-10-13-lr2-number-option-refs.md) |
| 2026-10-13 | [LR2選曲スキンのオプションbuttonを直接切り替えにする](2026/2026-10-13-lr2-select-option-buttons.md) |
| 2026-10-13 | [LR2の入れ子 #IF とPlay以外のBGA Size](2026/2026-10-13-lr2-nested-if-bga-size.md) |
| 2026-10-13 | [LR2スキンの解像度推定と全画面種別の上書き設定](2026/2026-10-13-lr2-resolution-inference.md) |
| 2026-10-12 | [upload-local / download-scores レビューと修正](2026/2026-10-12-ir-bulk-review-fixes.md) |
| 2026-10-11 | [設定画面からのIR一括操作と送信キュー画面](2026/2026-10-11-ir-settings-bulk-ops.md) |
| 2026-10-11 | [入場SEとシステムBGMの独立指定](2026/2026-10-11-system-sound-enter-loop.md) |
| 2026-10-11 | [プレイヤー向けマニュアルと設定説明の書き直し](2026/2026-10-11-player-manual.md) |
| 2026-10-08 | [RESULT BGMとサウンドセット（Discussion #29）](2026/2026-10-08-result-soundset.md) |
| 2026-10-07 | [最終プレイ日時refとLua参照ヘルパー](2026/2026-10-07-skin-read-helpers.md) |
| 2026-10-06 | [ZIP・RAR・7z内楽曲の直接再生](2026/2026-10-06-archive-song-playback.md) |
| 2026-10-06 | [演奏終了・ミス表示・見逃しキー音の任意設定](2026/2026-10-06-optional-play-behaviors.md) |
| 2026-10-06 | [beatoraja / LR2orajaED-rianの直近4か月の修正取り込み](2026/2026-10-06-upstream-compatibility-fixes.md) |
| 2026-10-06 | [Linuxのコントローラープレイ中のアイドル抑止](2026/2026-10-06-linux-idle-inhibit.md) |
| 2026-10-06 | [プレイ中の画面停止調査とGNOME / Plasma比較](2026/2026-10-06-play-freeze-diagnostics.md) |
| 2026-10-06 | [Linux遅延対応のレビュー指摘修正](2026/2026-10-06-linux-latency-review-fixes.md) |
| 2026-10-06 | [evdev以外の入力経路とWaylandイベント配送の比較](2026/2026-10-06-linux-input-alternatives.md) |
| 2026-10-06 | [Linux遅延診断・PipeWire・入力取得の実装と検証](2026/2026-10-06-linux-latency.md) |
| 2026-10-06 | [空フォルダへの選曲遷移と入力時再読み込みの修正](2026/2026-10-06-empty-select-folders.md) |
| 2026-10-05 | [リポジトリ横断リファクタリング](2026/2026-10-05-repository-refactoring.md) |
| 2026-10-05 | [Releaseメタデータの統合と橋渡し版の準備](2026/2026-10-05-release-metadata.md) |
| 2026-10-05 | [v0.4.3以降のレビュー指摘修正](2026/2026-10-05-post-v043-review-fixes.md) |
| 2026-10-05 | [LR2 Select / Resultレビューの修正](2026/2026-10-05-lr2-select-result-review.md) |
| 2026-10-05 | [ADFX02 / ECFNのBMZ基本拡張](2026/2026-10-05-ecfn-bmz-extensions.md) |
| 2026-10-05 | [ADFX02 / ECFNのDETAIL OPTIONS表示](2026/2026-10-05-ecfn-detail-options.md) |
| 2026-10-05 | [DETAIL OPTIONSのexperimental化と2パネル操作](2026/2026-10-05-experimental-detail-options.md) |
| 2026-10-05 | [DETAIL OPTIONSのレビュー7件への対応](2026/2026-10-05-select-detail-options-review.md) |
| 2026-10-05 | [途中FAILED時の譜面音声停止（issue #26）](2026/2026-10-05-failed-chart-audio.md) |
| 2026-10-04 | [PLAYターゲットスコアの最終値表示修正](2026/2026-10-04-play-target-score.md) |
| 2026-10-04 | [入力オフセットによる判定表示の点滅修正](2026/2026-10-04-input-offset-judge-display.md) |
| 2026-10-03 | [Luxez-FlatのDETAIL OPTIONS表示](2026/2026-10-03-luxe-flat-detail-options.md) |
| 2026-10-03 | [LR2 Select / Resultの互換対応](2026/2026-10-03-lr2-select-result.md) |
| 2026-10-03 | [Luxe Flatリザルトのランク差分とLua数値評価順序](2026/2026-10-03-result-grade-lua-order.md) |
| 2026-10-03 | [LR2のDXAアセット直接読込](2026/2026-10-03-lr2-dxa-assets.md) |
| 2026-10-03 | [LR2 Play skin互換修正](2026/2026-10-03-lr2-play-compatibility.md) |
| 2026-10-03 | [LR2 DST時刻の互換修正](2026/2026-10-03-lr2-destination-timing.md) |
| 2026-10-03 | [LR2スキンの既定解像度修正](2026/2026-10-03-lr2-default-resolution.md) |
| 2026-10-03 | [ResultパネルとLua状態の同期](2026/2026-10-03-result-panel-lua-state.md) |
| 2026-10-02 | [mz-selectのDETAIL OPTIONS表示](2026/2026-10-02-mz-select-detail-options.md) |
| 2026-10-02 | [リザルト更新時のLuaランダム背景維持](2026/2026-10-02-result-random-background.md) |
| 2026-10-01 | [Select E2 DETAIL OPTIONSの再設計](2026/2026-10-01-select-detail-options.md) |
| 2026-10-01 | [並列テストの一時ディレクトリ衝突対策](2026/2026-10-01-parallel-test-directories.md) |
| 2026-10-01 | [Starseeker ResultのMAX符号とLuaロード値](2026/2026-10-01-starseeker-result-next-rank.md) |
| 2026-10-01 | [リプレイ視聴速度による採点差の修正](2026/2026-10-01-replay-speed-scoring.md) |
| 2026-10-01 | [Core Audio IOProc音声と16 frames設定](2026/2026-10-01-coreaudio-ioproc.md) |
| 2026-10-01 | [macOS GameController入力](2026/2026-10-01-gamecontroller-input.md) |
| 2026-09-30 | [テキスト装飾の透明度修正](2026/2026-09-30-text-decoration-opacity.md) |
| 2026-09-30 | [macOS入力・音声診断と停滞試験](2026/2026-09-30-macos-latency.md) |
| 2026-09-30 | [antique Ambient背景の実装・検証](2026/2026-09-30-antique-ambient.md) |
| 2026-09-30 | [F3で譜面ファイルを選択表示](2026/2026-09-30-select-reveal-chart.md) |
| 2026-09-26 | [起動時間の調査と最適化](2026/2026-09-26-startup-performance.md) |
| 2026-09-25 | [LITONE9の表示同等性修正と描画最適化](2026/2026-09-25-litone-optimization.md) |
| 2026-09-25 | [WindowsでのFPS最適化レビューと追加調査](2026/2026-09-25-windows-performance.md) |
| 2026-09-24 | [選曲・リザルト・コースリザルトの性能調査](2026/2026-09-24-scene-performance.md) |
| 2026-09-21 | [Play画面の性能調査と修正](2026/2026-09-21-play-performance.md) |
| 2026-09-21 | [他のプレイスキンの性能調査](2026/2026-09-21-play-skin-performance.md) |
| 2026-09-15 | [Linux配布アーカイブの検証](2026/2026-09-15-linux-tar-validation.md) |
| 2026-09-08 | [Frame pacingとノーツ投影の実測・検証](2026/2026-09-08-frame-pacing.md) |
| 2026-08-17 | [Everythingによる曲探索の計測](2026/2026-08-17-everything-performance.md) |
| 2026-06-04 | [BMZ IRの設計記録](2026/2026-06-04-bmz-ir-design.md) |
