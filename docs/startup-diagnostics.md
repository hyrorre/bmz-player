# 起動時の診断ログ

通常のINFO設定で、初回surface描画までの段階を確認できます。
明示されたログレベル設定は従来どおり優先されます。
`startup environment` は実行ファイルの絶対パス、`resource_dir`、`data_dir`、`logs_dir` を
起動時に1回記録します。実行ファイルのパス取得失敗は `executable_error` に記録し、起動を継続します。
設定全体やIR認証情報は出力しません。設定TOMLの解析エラーはソース本文を省略して記録し、
元のエラーは従来どおり呼び出し側へ返します。

## 読み方

```text
startup phase begin phase="renderer_default" startup_elapsed_ms=...
startup phase complete phase="renderer_default" elapsed_ms=... startup_elapsed_ms=...
```

`startup_elapsed_ms` はアプリ起動処理開始からの累積時間、`elapsed_ms` はその段階の実時間です。
いずれも `Instant` で計測します。失敗は `startup phase failed` に段階名、両方の時間、
エラーを記録します。処理が `?` やpanicで中断された親段階は `startup phase interrupted` となります。
子の失敗を処理してフォールバックした親は完了する場合があります。子の失敗を成功には置き換えません。
停止中は最後の `begin` と、対応する `complete` / `failed` がない段階を確認してください。

各時間は子の処理を含みます。親子を足すと二重計上になります。
カタログ走査やSelect / Decide / Resultのdecodeは並列に進むため、それらも単純に足せません。
`skin_catalog_scan_dispatch` は走査workerの起動要求まで、`load_initial_skin_textures` は
同梱PNGの同期ロードとfrontend decode要求・pending登録までです。
非同期decode / uploadの完了は既存のskin pipelineログと照合します。

## 段階名

`WinitApp::new()` 開始から既存の `before_catalog_ms` の計測点まで:

| phase | 処理 |
|---|---|
| `winit_app_new` | constructor全体（以下の子段階を含む） |
| `constructor_options_and_ir_handle` | latency起動オプション、IR操作ハンドルとwake、CLI renderer反映 |
| `startup_difficulty_table_fetch_urls_for_boot` | 起動時の難易度表取得対象URLの選定 |
| `startup_difficulty_table_download_metadata` | 上記で使う取得済み難易度表メタデータのDB読み込み |
| `initial_select_settings` | rival sync要求、初期folder stack・filter・sort・cache生成 |
| `virtual_folder_catalog_load` | `VirtualFolderCatalog::load()` |
| `load_items_for_stack` | 起動時の選曲リスト構築全体 |
| `startup_play_options_and_input_bindings` | 初期mode反映、boot chart解決、起動オプション・replay・gauge・arrange・入力割り当て |
| `renderer_default` | `Renderer::default()` とBoxへの格納 |
| `renderer_initial_configuration` | フォント検索先・coverage・内部解像度の初期設定 |

選曲リスト内部（起動時のみ出力し、通常の選曲操作では出力しない）:

| phase | 処理 |
|---|---|
| `select_list_build` | DB / 設定からの一覧生成全体 |
| `select_list_sources` | 有効な曲root・難易度表sourceの選定 |
| `select_list_cached_ir_sources` | IR sourceがある場合のキャッシュDB読み込み |
| `select_root_base_items` | 曲root・コースの一覧生成 |
| `select_root_difficulty_table_folders` | 難易度表フォルダ情報の取得 |
| `select_root_virtual_folders` | 仮想フォルダのルート一覧整形 |
| `select_root_favorite_folders` | お気に入りルート情報の取得 |
| `select_root_settings_and_history` | 設定・検索履歴のルート一覧整形 |
| `select_list_db_and_formatting` | first-seen等のDB読み込み、filter / sort / random項目の整形 |
| `select_list_collection_flags` | 上記に含まれるお気に入り等のcollection情報の反映 |

前後の接続:

| phase | 処理 |
|---|---|
| `startup_environment` | 実効パスの取得・記録 |
| `bootstrap_and_boot_options` | 既存のbootstrapとCLI boot設定 |
| `event_loop_setup` | Raw Input bridge、イベントループ生成、Viewer listener、Ctrl-C handler等 |
| `skin_catalog_scan_dispatch`, `player_stats_snapshot` | カタログ走査要求と初期統計読み込み |
| `load_initial_skin_textures`, `default_skin_assets_load` | 初期スキン処理と、その中の同梱PNGロード |
| `gamepad_initialization` | gamepad backend・slot・analog設定 |
| `obs_and_system_sound_catalog` | OBS controller起動とシステム音候補走査 |
| `select_assets_and_result_signature` | preview / asset runtimeと初期Result signature |
| `difficulty_tables_for_ui`, `select_folder_summary_runtime` | UI用DB読み込みとsummary worker起動 |
| `app_state_assembly`, `constructor_final_setup` | app状態生成、debug boot・システム音要求・Discord設定 |
| `event_loop_handoff` | profile IR sync要求からイベントループ開始直前まで |
| `window_setup` | 初回window初期化全体 |
| `window_attributes_and_monitors`, `winit_create_window` | monitor・属性解決とnative window作成 |
| `window_native_and_renderer_configuration` | native初期化とrenderer backend / present設定 |
| `renderer_attach_surface` | GPU / surface生成。fullscreen fallback時は同名で再試行 |
| `window_input_and_egui` | windowへの入力attach、clipboard、egui / font初期化 |
| `initial_select_skin_ready` | 初回Selectのdecode / upload待ち |
| `first_surface_frame` | 最初の描画要求から初回present成功まで（復旧・再試行を含む） |

既存の `application bootstrap complete`、`startup constructor timings`、
`starting winit event loop`、`first surface frame presented` も引き続き出力します。
`before_catalog_ms` はconstructor開始からカタログ走査要求直前まで、`skins_ms` は
初期skin pipeline生成・読み込み・pending登録、`after_skins_ms` はその後からconstructor集計まで、
`constructor_total_ms` はconstructor開始から集計までです。
カタログ走査要求・統計読み込みは `before_catalog_ms` と `skins_ms` の間なので、
既存3区間の合計がconstructor全体と等しいとは限りません。
`startup_to_first_present_ms` は従来と同じ起動開始から初回present成功までです。

## 再計測

同じreleaseビルド・設定・DB・アセットで複数回起動し、`startup environment` から
`first surface frame presented` までのINFOログを確認します。
まず `before_catalog_ms` に時間が残るかを確認し、上のconstructor子段階で長い区間を絞ります。
完了しない場合は最後の `begin` を確認します。それより後なら、window / surface / Select待ち /
初回描画の各段階へ進みます。認証情報を含む設定ファイル全体を診断目的で共有する必要はありません。
