# 選曲ツール・撮影名・終了操作・ローカルライバル

## 変更

- Random Trainerに共通ショートカット`OpenRandomTrainer`を追加。既定は未割り当てで、既存プロファイルにもキーを追加しない。
- 手動スクリーンショットは撮影フレームの`table_text_secondary`を曲名の前に追加する。表の選択規則、禁止文字、240バイト制限、重複時の連番は既存経路を使う。コース総合には追加しない。
- `select.exit_hold_ms`は既定1200ms、UIで0〜5000ms。0はルートで受理した押下を終了要求として保持し、次の描画前に解放されても終了する。編集取消・メニューを閉じる・フォルダを戻る操作とプレイ中断用の時間は独立。
- 選曲設定から`LocalProfile`ライバルを登録し、IRライバルと同じ選択操作で切り替える。IR未設定でも使える。
- 別プロファイルの`score.db`を読み取り専用で開き、一貫したread transactionでベストを取得する。参照先のmigrationや書き込みはしない。キャッシュは譜面SHA-256・LN保存区分・DOUBLE保存区分・ルールの完全な`ScoreKey`で引く。
- ローカルライバルのEX・ランプ・BP・コンボ・判定内訳を選曲へ渡し、数値ベストをプレイ時のターゲットに使う。clear-onlyの記録をEX 0の記録と混同しない。G-BATTLEと配置再現は対象外。
- 選択変更とF5で参照先を読み直す。取得世代を確認し、切り替え前のデータやエラーを反映しない。欠損DBは作成せず、失敗を通知する。
- Selectの標準ノート分布グラフの背景を、1pxの線から密度10刻みの帯へ修正。色と縦軸、`backTexOff`は既存契約を維持する。

背景の根拠は[beatorajaのSkinNoteDistributionGraph](https://github.com/exch-bms2/beatoraja/blob/0430bf010e4123159ffc6260ffd76abab54305ab/src/bms/player/beatoraja/skin/SkinNoteDistributionGraph.java#L470)。
同梱Luxez-Flatの`select_skinparts/default_chart/parts.lua`は`judgegraph`のtype 0と`backTexOff=0`を使うため、本体の修正で反映される。

## 統合と検証

調査開始時は`4cd70074`。作業中に更新されたmainの`4017d9c6`を取り込み、BMZ IR / rianIR / BMS-IRの選択・保存区分と共通キーコンフィグの変更を維持した。

追加した回帰テストは、未割り当てと設定保存、物理キーとコントローラーの解決、撮影フレームの表情報、終了時間と取消、ローカルスコアの完全なキー・clear-only・参照先保全・世代・欠損DB、背景帯の高さと密度に応じた拡縮を確認する。

Linuxの検証環境にはRustと開発ヘッダーがなかったため、作業領域内にRust stableと必要な開発パッケージを配置した。環境準備や生ログはリポジトリへ含めない。
同梱の3スキンsubmoduleは記録されたcommitで初期化した。任意の外部スキンがない場合に早期returnするテストを、そのスキンの互換性の確認済み件数には含めない。

Xvfb + Vulkan Lavapipeと隔離したデータで、起動・描画・PNG保存を確認した。F2を明示的に割り当てたプロファイルでRandom Trainerを直接開き、画面キャプチャを確認した。
0msではメニューのEscapeとフォルダ内のEscapeで終了せず、ルートで短く押して離すと正常終了した。
統合後の実行ファイルでも同じ入力を確認した。Luxez-Flatで隔離した別プロファイルのEX 10・HARDランプと設定画面の登録・選択を確認し、実行前後で参照先`score.db`のSHA-256が一致した。

統合前の全体テストは、C.UTF-8で日本語既定値を仮定する4件とスキャン関連2件が失敗した。日本語ロケールでの再実行では4件が通過し、スキャン関連2件のみ失敗した。
スキャン実装・該当テストは今回の変更範囲に含めない。

`4017d9c6`へ統合後の結果:

- `cargo fmt --all --check`: 通過。
- `cargo check --workspace --locked`: 通過。
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: 通過。
- `LANG=ja_JP.UTF-8 LC_ALL=ja_JP.UTF-8 cargo test --workspace --locked --no-fail-fast`: playerのlibは3303件通過・2件失敗・68件ignore。他の対象は失敗なし。追加した回帰テストはすべて通過。
- 残る失敗は`storage::scan::link_tests::overlapping_external_links_read_every_folder_once`と`storage::scan::tests::scan_song_roots_skips_unreadable_subdirectory_and_keeps_accessible_files`。選曲ツール等とは独立した探索件数の期待値不一致であり、mainと同じ実装・テストを維持した。
- Windows / macOS、実機のコントローラー入力・音声出力は未確認。コントローラーの割り当ては共通入力パイプラインの自動テストで確認した。

関連仕様: [操作](../../docs/controls.md)、[設定](../../docs/settings.md)、[スキン](../../docs/skin.md)。
