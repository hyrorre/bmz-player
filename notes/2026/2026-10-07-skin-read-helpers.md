# 最終プレイ日時refとLua参照ヘルパー

## 対象

beatorajaの直近4か月の変更確認から追加実装を依頼された項目。
main上でサブエージェントを順に使用し、次を実装した。

- `main_state.numbers` とtimerのON/OFF・経過時間参照。
- 数値ref `243..249` の最終プレイ日時。
- `main_state.screen_width` / `screen_height`。

参照元は `.local/beatoraja` の `42bbd334` / `d68015ce`（2026-06-16）と
`39351940`（2026-08-04）。外部スキンは変更しない。

## 契約と確認事項

- 名前によるプロパティ解決は既存実装を使用する。
- `numbers` はテーブルではなく複数戻り値を返す。
- timerの経過時間はus / 整数ms / 小数秒。OFFは-1。
- 最終プレイ日時と、既存の `score_date_sec_time()` が返すベスト日時を区別する。
- 日時欠損は `Integer.MIN_VALUE`。日付部品はローカル時刻で計算する。
- Luaのロード時依存捕捉と永続VMのruntime参照を両方確認する。
- 仕様は [skin.md](../../docs/skin.md)、対応状況は
  [skin-compatibility.md](../../docs/skin-compatibility.md) に反映する。

### 画面サイズの参照元

上流の `MainLoader.java` はLWJGL2 backendを使用する。
同梱 `lib/gdx-backend-lwjgl.jar` の `LwjglGraphics` を `javap -c` で確認した。
通常の非AWT経路では `getWidth` / `getHeight` はDisplayの寸法に
`getPixelScaleFactor()` を乗じ、`getBackBufferWidth` / `getBackBufferHeight` も
同じ値を返す。したがってBMZでも実際の描画先のピクセル寸法を基準にし、
スキン設計解像度やDPIで除算したウィンドウ論理寸法とは区別する。

## 検証

### Luaの複数値・タイマー参照

`0499b7dc` に実装。既存number/timer/time accessorを利用し、ロード時と
永続runtime VMでhelperを登録する。推論中の試行値でロード時のnumber依存を
上書きする不具合も、同じ経路の回帰で検出して修正した。

- `cargo fmt --check`
- `cargo check -p bmz-skin --locked`
- `cargo clippy -p bmz-skin --all-targets --locked -- -D warnings`
- `cargo test -p bmz-skin --locked`: 256 passed / 0 failed（新規4件）。
- 空引数、名前/数値の混在、欠損値、捕捉関数、Auto/Compat、OFF→ON→OFF、
  開始0、未来開始、サブms、64bit時計、同一frame custom timer更新を確認。

### 最終プレイ日時

通常プレイはScoreKeyごとのnon-autoplay履歴MAX、履歴を作らないclear-onlyは
新しい `score_unrecorded_plays` 表から取得する。Select一覧ロード時にまとめて取得し、
描画時にはDBを参照しない。履歴削除・import日時補正は履歴MAXへ直接反映される。
コースはcourse hash / LN policy / RuleModeごとにreplay-onlyを除外したMAXを使う。
Resultは保存に成功した今回のattemptの日時を渡す。

score.db migration 31は追加表とindexを作り、履歴を持たないBEST行に残っている
日時を移行する。旧版で更新されなかったアシスト日時は復元できない。
既存BESTの `played_at` と `score_date_sec_time()` は変更しない。

- `cargo check -p bmz-player --tests --locked`: 成功。
- playerの日時関連対象テスト: 9 passed（新規8件・既存1件）。
- rendererの日時関連対象テスト: 2 passed。
- 自己ベスト未更新、Failed、clear-only、履歴cleanup、migration、205件バッチ、
  score key分離、コースのreplay-only除外、Select再読込を確認。
- Resultの通常/clear-only/Autoplay/Replay/Practice、Lua Auto/Compat、欠損描画、
  ローカル時刻、2038境界を確認。

### 描画先の画面サイズ

`screen_width` / `screen_height` はrendererの最終出力寸法を4種類のsceneへ渡す。
Luaで関数を捕捉した場合もruntime providerの現在値を参照する。offscreen出力では
動画出力寸法を使い、描画先が未接続の場合は0を返す。

ロード時に寸法を参照した場合だけcache依存へ追加し、寸法変更時に再decodeする。
runtime callback内でだけ参照した場合はVMを維持する。初回Selectのwindow生成前に
0でロードした文書と、0の除算等でロードに失敗した要求はsurface接続後に再試行する。
最大4sceneの現在の要求を保持し、古い寸法・世代のuploadを適用しない。
プロフィール切替時は切替先の要求と世代を維持し、旧プロフィールのrandom file選択を
新しい要求へ持ち込まない。

- `cargo check -p bmz-player -p bmz-render -p bmz-skin --all-targets --locked`: 成功。
- `cargo test -p bmz-skin --locked`: 259 passed / 0 failed。
- player画面寸法の通常回帰: 6 passed。GPU回帰は下記のとおり別途明示実行。
- headerだけのサイズ参照、Auto/Compatの捕捉関数、cache依存有無、初回0失敗からの
  再decode、profile世代、旧profileのrandom file非混入、4sceneの未接続0を確認。

### workspace統合検証

- `cargo fmt --check`: 成功。
- `cargo check --workspace --locked`: 成功。
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: 既存問題で失敗。
  `crates/bmz-player/src/input/capture.rs:111` の `let _ = route_changed_at` に対する
  `clippy::let_unit_value`。今回の親HEADにも同じ行が存在し、当該ファイルは変更していない。
  lintの抑制や無関係な修正は行わない。

- `cargo test --workspace --locked --no-fail-fast`: 3,924 passed / 0 failed / 31 ignored。
  nested test subprocessの重複集計は除外。GPU回帰1件は下記のとおり別途明示実行。
- `cargo build -p bmz-player --locked`: 成功。
- `cargo test -p bmz-player --locked screen_dimensions_follow_offscreen_output_and_target_replacement_in_all_scenes -- --ignored --nocapture`:
  成功。Auto/Compatの全4scene、640x360→800x600へのoffscreen付け替え、切断時0、
  内部解像度がSkinでも出力サイズを返すことを確認。

初回のworkspaceテストでは、`WinitApp` の既存64KiB上限回帰が65,664 bytesで失敗した。
スキン読込状態を `Box<SkinPipelineRuntime>` で保持し、型と構築処理の2行を変更して
既存上限へ戻した。制約値は変更していない。

実アプリのSelect確認では、画面サイズは一致したが、開始時刻0のcustom timerが
OFFのままになった。Select専用描画経路が共通経路の `advance_custom_timers` を
呼んでいなかったことが原因で、Auto/Compat双方で再現した。新しいtimer helperを
Selectでも使えるよう、共通経路と同じ順序で更新した。修正前に新規の描画回帰が
失敗することを確認し、修正後はcustom timer対象4件が成功した。同frameのpassive
timer更新によるdestination表示、経過時間3単位、同じ時刻の次frame、同frameの
複数描画passで一度だけ更新することをAuto/Compat双方で確認した。

### Windows実アプリ

Windows / DX12のdebugビルドを使用し、専用の空プロフィールとconfig・DB・cacheで確認。
Lua fixtureと生ログ・PNGは `.local/validation/2026-10-07-skin-read-helpers/` へ保存した。
ユーザーの通常プロフィールは使用していない。

- Auto: 960x540起動後、最初の描画より後にWin32でウィンドウをリサイズ。
  最終PNGは2241x1261で、`Render` と `Load` の表示が両方2241x1261に一致した。
  外部Win32操作の要求寸法はDPI仮想化の影響を受けるため、PNGの物理寸法を照合基準とした。
- Compat: 1600x900起動のPNGで `Render` / `Load` とも1600x900を確認。
- 両モードで開始0のtimerがON、未設定timerがOFF、us / ms / 小数秒の表示を確認。
  最終版のSelectログにLua callback失敗はない。
- 同梱サンプルの通常プレイ後、CompatのResultを180frame描画して終了。
  `number(243) > 0` と年月日の範囲をcallbackで検査し、Lua警告なし、正常終了を確認。
  AutoのResultでも同じ日時検査を確認した（Selectのtimer修正前のビルド）。

未実施: macOS / Linuxの実機表示、モニター間のDPI切替、外部スキン全体の見た目確認。
offscreen寸法はGPU回帰で検証し、動画ファイル全体のexportはこの作業では行っていない。
