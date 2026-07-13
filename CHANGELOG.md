# CHANGELOG

## Unreleased

### 改善

- Windowsのゲームパッド入力にmain-thread polling方式のGameInput backendを追加しました。
  - ゲームパッドbackendの既定値と自動選択はgilrsを優先し、Windowsでgilrsを初期化できない場合はGameInputへfallbackします。
  - GameInputのreading時刻を判定へ渡し、1P / 2P割り当てをstable device IDで保存するようにしました。
  - GameInputの履歴取得をデバイス単位にし、曲終了後や一時切断後も入力と割り当てが復帰するようにしました。

## v0.1.7

### 改善

- 入力遅延と複数コントローラ対応を改善しました。
  - Windows でプレイ中のキーボード入力を Raw Input 経路へ切り替え、入力を描画前に反映するようにしました。
  - gamepad のイベント時刻を保持し、10K / 14K で 1P / 2P に別々のコントローラを割り当てられるようにしました。
  - 接続順に欠番がある場合や旧 wildcard 設定が混在する場合も、物理デバイス固有の割り当てを優先するようにしました。

- 外部アプリ連携を追加しました。
  - Discord Rich Presence で Select / Decide / Play / Result / Course Result の状態、曲名、アーティストを表示できるようにしました。
  - OBS WebSocket v5 によるシーン切り替え、録画開始・停止、再接続、状態別 action 設定に対応しました。

- 選曲プレビューと音量バランスを改善しました。
  - `#PREVIEW` や preview 音声が無い譜面では、ノーツ密度の高い区間からプレビューをオンデマンド生成するようにしました。
  - 選曲プレビューの音声が乱れる不具合を修正しました。
  - プレイ音量の正規化基準を調整しました。

- IR と外部スコアの取り込みを拡張しました。
  - IR 登録前のローカルスコアを throttled sync で一括送信する `bmz ir upload-local` を追加しました。
  - 送信済みスコアの device key attestation、import 元・option・device type の保持、再取り込み時の重複 cleanup を追加しました。
  - beatoraja / LR2 スコアの LN policy とノート数を検証し、対応できないレイアウトを安全に skip するようにしました。

- スキン互換性を改善しました。
  - PeacefulPlay のゲージ値・先端発光、キービーム、NPS / key logger、READY 前表示を再現できるようにしました。
  - mz-select の Result タイトル、WMII CSV LR2Skin の LN animation、ECFN 14K の Lua layout と turntable 回転を修正しました。
  - Result skin で今回の IR 送信成功・失敗を表示できるようにしました。

- 選曲画面と設定操作を改善しました。
  - 設定項目をマウスクリックとホイールで編集できるようにしました。
  - favorite 登録 / 解除とスクリーンショット保存を左上のトーストで通知するようにしました。
  - 新規設定に Dystopia、PMS、DP 系を含む難易度表を追加しました。

### 修正

- FHS 使用中の通常のハイスピード変更で target green number が書き換わる問題を修正しました。
- 2P コントローラ操作が 1P の選曲 option として解釈される問題と、9K の選曲移動方向を修正しました。
- Discord / OBS が後から起動した場合や再接続した場合に、表示・シーン・録画状態が復帰しない問題を修正しました。
- JavaScript の安全整数範囲を超える random seed が IR 署名検証時に丸められる問題を修正しました。
- BMS / BMSON の beatoraja 互換性を向上させました。

### テスト・開発環境

- 入力 backend の queue、timestamp age、drain / translate / drop 件数を診断できるようにしました。
- Raw Input、gamepad 割り当て、生成プレビュー、OBS / Discord、スコア import / IR cleanup、PeacefulPlay を含む外部スキンの回帰テストを追加しました。
- `docs/controls.md`、`docs/ir.md`、`docs/ln.md` を更新しました。

## v0.1.6

### 改善

- 選曲画面に favorite song / favorite chart のコレクション機能を追加しました。
  - `F8` で song、`F9` で chart を favorite 登録 / 解除できます。
  - favorite 用の仮想フォルダを追加し、通常フォルダと同じようにスコア・リプレイスロット・難易度表情報を表示するようにしました。

- コースプレイとコースリザルトの保存・表示を改善しました。
  - コース結果を profile の `score.db` に保存し、選曲画面でベスト、リプレイスロット、トロフィー達成状況を表示できるようにしました。
  - コース結果を rule mode ごとに分離し、FAILED 時もコース全体のノート数で達成率を計算するようにしました。
  - コース用の Result / Select スキン表示、stage 結果、ゲージ推移、retire / fail 音の扱いを調整しました。

- BMZ IR と IR Web のコース・ランキング表示を拡張しました。
  - コーススコア送信、コースランキング、自己スコア一覧、プレイヤー一覧を追加しました。
  - charts / courses / players の一覧に pagination を追加しました。
  - 1P / 2P 別の arrange option を IR 表示・payload に反映するようにしました。

- プレイオプションとハイスピード表示を改善しました。
  - `F-RANDOM` / `MF-RANDOM` を追加しました。 (いわゆる `HALF RANDOM` / `MIRROR HALF RANDOM` です)
  - プレイ中に E2+Scratch または E2+鍵盤 で緑数字を調整できるようにしました。
  - BMZ 独自の HS mode / target green number skin ref を追加しました。

- スキン互換性とデフォルト選曲スキンを改善しました。
  - デフォルト選曲スキンの表示情報を大幅に増やし、コース・favorite・リプレイスロット・ランプ情報を見やすくしました。
  - play / result / select skin の score graph、value number、image ref、course row、folder lamp、operating time ref の対応を増やしました。
  - Lua skin の option 依存 draw、`value` 式、end-of-note timing、result miss count 差分の扱いを改善しました。
  - Rmz-skin の 5K / 6K 系 note color を同梱向けに更新しました。
  - replay autosave rule、favorite、folder lamp などの select skin ref を追加・整理しました。

- 音声再生をより安定させました。
  - AudioEngine への操作を command queue 経由にし、system / play / preview 音の更新を audio callback と分離しました。
  - 選曲 preview 音の切り替え、Result 終了音の fade、quick retry 時の asset 再利用を改善しました。
  - 同一 chart sound の restart policy を整理し、beatoraja に近い鳴り方へ寄せました。

- 動画 BGA / skin movie の再生を改善しました。
  - skin movie が再生時刻に追従し、loop や未来フレーム待ちで不自然に止まったり早送りされたりしにくくしました。
  - decoder drop 時に decode thread を join し、Result 背景動画などの freeze を防ぎました。

- 配布物のライセンス表示を整備しました。
  - `THIRD-PARTY-NOTICES.txt` と `cargo-about` 由来の Rust 依存ライセンス report を release package に含める流れを追加しました。
  - アプリ内の egui 画面と BMZ IR Web の `/licenses` でライセンス report を表示できるようにしました。

### 修正

- BMS / BGA / 音声 asset の beatoraja 互換性を修正しました。
  - 同時刻の複数 BGA layer を保持するようにしました。
  - 未定義 BGA layer event をクリアし、同 stem の音声・BGA asset fallback を beatoraja に合わせました。
  - long note end の無音キー音や hidden note 周りの音声扱いを修正しました。
  - BGA 画像の読み込み完了前に READY を抜けないようにしました。
  - system se の再生開始、終了条件を微調整しました。

- プレイ中の安定性を修正しました。
  - gauge / judge の境界ケースで play 中に panic せず継続できるようにしました。
  - READY 待ち中の hold 入力、READY 中の skin intro 停止、同 tick STOP 後のノート非表示を修正しました。

- リザルトと MYBEST 表示を修正しました。
  - 初回 Result で MYBEST 表示が異なる不具合を修正しました。
  - Result ゲージ遷移グラフ、clear lamp image ref、min BP 差分の初回表示を修正しました。
  - current play / previous best / target の score rate、graph、value number の解決を修正しました。

- 選曲画面の表示と操作を修正しました。
  - best clear lamp 更新が保持されるようにしました。
  - select bar / folder lamp / course level / course score / favorite ref の表示を修正しました。
  - 初期選曲画面で select BGM が再生されない不具合を修正しました。
  - `E1+E2` でも decide をキャンセルできるようにしました。

- IR 同期とセキュリティを修正しました。
  - 手動 IR sync の retry が詰まる問題を修正しました。
  - course score best の FK failure を回避しました。
  - replay upload に認証と size limit を追加し、score submit / refresh / replay endpoint に rate limit を追加しました。
  - production では session password を必須にし、IR error response body をログへ丸ごと出さないようにしました。

- スクリーンショット保存先と portable layout の表示を修正しました。
  - スクリーンショットを data dir 配下へ保存するようにしました。
  - portable 版では スキンの[同梱]表示を隠すようにしました。

### テスト・開発環境

- `bmz-skin-document` crate を追加し、skin document schema / load / runtime 型を `bmz-render` から分離しました。

- `score.db` / `network.db` / `collection_db` まわりの migration と責務を整理しました。
  - IR sync state を `network.db` に移し、client score database schema を整理しました。
  - course score 専用 DB 層を追加しました。

- release packaging と CI を更新しました。
  - Windows release を default features で build するようにしました。
  - release metadata assets の対象を絞りました。
  - macOS / Windows package script に license report の同梱処理を追加しました。

- `docs/hs.md`、`docs/ir.md`、`docs/licenses.md`、`docs/skin.md`、`docs/controls.md` を更新しました。

- BGA、Lua skin、select skin ref、audio command queue、course score、IR Web まわりの回帰テストを追加・更新しました。

## v0.1.5

### 改善

- プレイ中のスキン option 変更と live editing の反映を大幅に高速化しました。
  - decoded skin document / source / font / GPU texture を cache。
  - options-only の変更は可能な範囲で即時反映。
  - full reload 中も描画を止めず、切り替え時の固まりを軽減しました。
  - 同一画像・動画 first frame・font payload の再 decode / 再 upload を抑制しました。

- WMII FHD / LR2Skin / Lua skin のプレイスキン互換性を改善しました。
  - レーンカバー、LIFT、緑数字/白数字、score graph、target 表示、判定詳細、gauge 表示を beatoraja の挙動に近づけました。
  - LR2 `#SETOPTION` 由来 op、play key mode op、autoplay 中の score graph 表示を正しく扱うようにしました。
  - LR2 text の overflow shrink、bitmap font、TGA font page、w=0 text align などの描画差異を修正しました。

- BGA / chart asset の beatoraja 互換性を改善しました。
  - `#BMP` の同 stem 拡張子 fallback を追加。
  - GIF / TGA 静止画 BGA、低 bitrate 動画 stream、WebM などの扱いを改善。
  - 小さい静止画 BGA を beatoraja 互換の 256x256 padding で読み込むようにしました。
  - 動画 BGA の初回フレームを open 時に prime し、黒表示を減らしました。
  - 非表示の動画 BGA decoder を停止するようにしました。

- 文字描画品質を改善しました。
  - bitmap font の非整数倍率拡縮を bilinear 補間に変更。
  - vector font atlas を supersampling して、小サイズや非整数倍率でのジャギーを軽減しました。

- 音声の乱れを改善しました。
  - system sound の音量更新をまとめ、同値更新時の lock を回避。
  - play keysound の音量変更は pending queue 化し、AudioEngine が busy の場合は次フレーム以降に retry するようにしました。

### 修正

- リプレイ再生時の FAST/SLOW 表示条件を修正しました。
  - replay 中に profile の autoplay state が skin op へ漏れないようにしました。
  - replay でも Auto 表示の scope を正しく扱うようにしました。

- 選曲画面から単曲リプレイを開始した場合も decide 演出を経由するようにしました。

- Result 画面の fadeout skip と終了キーの扱いを調整しました。
  - 長い fadeout を入力でスキップ可能にしました。
  - Key2 / 2P Key2 を result exit 対象から外しました。
  - リプレイ後に選曲へ戻ったとき、押下状態が stale hold として残る問題を修正しました。

- Floating hispeed / HS-FIX / READY 前の表示を修正しました。
  - READY 前から基準 BPM と固定緑数字に基づく HS 表示を揃えました。
  - 曲開始前は HS-FIX 基準 BPM、開始後は現在 BPM を使うようにしました。

- EmptyPoor を LR2 / beatoraja 互換の poor / miss 系 skin ref に含めるようにしました。

- STAGEFILE / BANNER / BACKBMP などの chart meta image でも同 stem 画像拡張子 fallback を行うようにしました。

- スキン filepath 選択を basename ではなく skin root 相対 path で解決し、同名ファイルや LR2 `#CUSTOMFILE` の選択が正しく反映されるようにしました。

### テスト・開発環境

- WMII FHD / LR2Skin / BGA / skin reload cache / audio diagnostics まわりの回帰テストを追加・更新しました。
- スキン reload timing ログを通常時に邪魔にならないよう debug 出力へ変更しました。
- `AGENTS.md` の PowerShell 7 / utf8NoBOM commit message / worktree setup に関する作業メモを更新しました。

- `bmz-audio` / `bmz-player` に CPAL / ASIO 出力の診断ログを追加しました。
  - callback 時間、lock miss、clipping、stream error を計測。
  - lock miss を system / play / draining などの source 別に追跡可能にしました。

- BGA 互換確認用の最小 fixture `data/songs/bga-compat` を追加しました。
  - PNG / GIF / TGA / WebM / 拡張子 fallback / animated GIF の挙動をテストで固定しました。

- プレイ中スキン reload の計測を追加しました。
  - skin decode / source decode / GPU upload / main apply の時間をログ化。
  - cache hit / miss や skip 件数を確認できるようにしました。
