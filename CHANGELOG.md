# CHANGELOG

## v0.4.2

### 改善

- 選曲画面から、再起動せずにプロファイルを切り替えられるようにしました。
  - 新規作成・複製後の切り替えにも対応し、設定・入力・スキン・スコア・リプレイ・IR送信をプロファイルごとに切り替えます。切り替えに失敗した場合は元のプロファイルを維持します。

- Linux版の実行用 tar アーカイブを正式リリースへ追加しました。
  - Ubuntu 22.04を基準に、同梱リソース・共有ライブラリ・ライセンス表記をまとめた実行用アーカイブと、オフライン再ビルドに必要な対応ソースを分けて配布します。

- 選曲画面のキーモードフィルターに4K・6K・8Kを追加しました。同梱の mz-select / Luxez-Flat でも現在のモード名を表示し、FORCE LN / CN / HCN 設定時にはFORCE表示を追加しました。
- スキン設定に追加スキンの配置案内、フォルダー表示、一覧更新、再試行と読み込み診断を追加しました。各OSの配布物に日本語・英語のREADMEを同梱します。
- Luaスキンへ、アプリ起動中のプレイ回数・処理済みノーツ数と選択スコアの日時を公開しました。セッション統計にはローカル保存済みのプレイを集計し、Autoplay・Replay・保存対象外のプレイは含めません。
- 自己ベストEX SCOREを記録した際の1P・2P配置とDP OPTIONを、Lua / JSONスキンから参照できるようにしました。Resultでは今回の保存前のベストを参照します。

- スキン描画のCPU負荷とメモリ確保を削減しました。
  - ノーツの座標・画像情報をフレーム内で再利用し、画像・数値の検索情報や静的な描画要素をキャッシュすることで、高密度譜面の描画準備を軽くしました。
  - 動画のテクスチャ転送バッファを再利用し、非表示中のGPU転送を省くようにしました。動画は初めて表示するときに起動し、開始済みの動画は非表示中も再生時刻を維持します。
  - Luaの状態参照API・コールバックの関数ハンドルとbitmap fontのページ画像・文字情報を再利用し、コールバック評価やスキン再読み込み時の生成・コピーを減らしました。
  - 選曲バーの一時データ生成、アニメーションのキーフレームのコピーを減らし、固定されたリザルトグラフと変化のない数値画像を再利用するようにしました。Luaコールバックの評価順・回数と動的表示は維持します。

### 修正

- Windowsインストーラーで更新・再インストールした際、追加したユーザースキンが削除される問題を修正しました。
- スキンのオプション変更による再読み込み中に設定一覧が消える問題を修正しました。読み込み待ち・失敗時も設定値を保持します。
- プロファイル切り替え時に内蔵の選曲画面が一瞬表示される問題を修正しました。切り替え先のスキンを準備してから一括適用し、準備・保存に失敗した場合は元の画面を維持します。スキン演出中の空フレームも内蔵描画へ切り替えずに扱います。
- 自動ダウンロードした曲の保存先を有効な曲フォルダとして登録し、フォルダ一覧や検索から譜面が消える問題を修正しました。
- BMS-IRの曲ページを、選択中譜面のMD5を使って正しく開くようにしました。
- Luaスキンで、動的な表示条件やテキストが固定値として扱われ、TIME・TOTAL・色分けが更新されない問題を修正しました。
- 選曲スキンで、クリア・フルコンボ表示、IRランキングのランプ人数・割合が欠ける問題を修正しました。人数・割合はランキングを全件取得できた場合に集計します。
- Luaスキンで未取得のIRスコアが0と表示される問題を修正し、Auto / Compatの両モードで欠損値を非表示にしました。実際のスコア0は表示します。
- 選曲スキンのNOTES / LN集計からスクラッチを分離し、LN終端の密度を時刻順に計算するようにしました。
- 一時的に非表示になったスキン動画が再表示時に先頭へ戻る問題と、停止・再起動の繰り返しによる描画負荷を修正しました。
- CIM形式のbitmap fontページを読み込めない問題を修正しました。
- 同梱選曲スキンのモード名・ボタン枠・矢印を復元し、文字サイズとFORCE表示の位置・間隔を調整しました。
- Wayland環境でスクリーンショット画像と譜面ハッシュをクリップボードへコピーできない問題を修正しました。FlatpakやGNOMEでもネイティブWayland経由でコピーします。
- FlatpakでPulseAudioの認証情報を読み込めず、音声出力の自動選択がALSAへ切り替わる問題を修正しました。
- 終了前にスキン・BGA画像の読み込みとGPU転送処理の完了を待つようにし、Linuxの一部GPU環境でアプリ終了時にクラッシュする問題を修正しました。

### テスト・開発環境

- プロファイル切り替え、曲の自動ダウンロード後の登録、選曲フィルター・同梱スキン表示、Luaの動的評価・セッション統計、CIMフォント、動画更新と終了処理の回帰テストを追加・更新しました。
- 複数スキンで描画結果の一致と描画準備時間を比較し、動画転送のGPU読み戻しによる画素一致も検証しました。
- 選曲・単曲リザルト・コースリザルトの性能計測を追加し、macOS / WindowsでCPU処理時間と実画面のFPSを分けて比較しました。ベスト時の配置、IR欠損値、プロファイルとスキンの一括切り替え、動画の再表示、描画キャッシュの回帰テストも追加しました。
- Linux配布物の展開後の独立した動作確認、対応ソースからのオフライン再ビルド、チェックサムとリリース情報の検証を整備しました。Windowsインストーラーのユーザーファイル保持検証もリリースCIへ追加しました。

## v0.4.1

### 改善

- Windows portable版とmacOS版に、アプリ内からの自動更新機能を追加しました。
  - v0.4.0以前からは、この機能を搭載したバージョンへ一度手動で更新する必要があります。

- 一般設定にUI倍率を追加しました。
  - 設定画面などの補助UIを、OSの表示倍率に加えて100・125・150・175・200%から拡大できます。倍率はプロファイルごとに保存します。

### 修正

- 左右Shiftの同時押し・解放時にオプションパネルの状態が残ったり、詳細オプションが通常パネルへ戻ったりする問題を修正しました。
- キーボード入力方式にWinitを選択した場合も、Windows Raw Inputから譜面入力が混入する問題を修正しました。
- 選曲画面で7Kのカスタムキーが14Kの2P設定にも一致した場合、別のオプションが反応する問題を修正しました。
- 難易度表のレベル表示設定がスキンの数値欄へ反映されない問題を修正しました。整数でない表レベルは数値欄を非表示にし、G-BATTLEの順位表示は保持します。
- 譜面ファイルが存在しないなどの理由でDecide中の先読みが失敗すると、Play画面で開始待ちが続く問題を修正しました。選曲画面へ戻り、対象パスと原因を通知します。
- 演奏終了後の待機中にもEscで退出演出を開始できるように修正しました。
- BMS-IRの難易度表取得時、rianIRから取得したかのような表示になっていた問題を修正しました。

### テスト・開発環境

- UI倍率の保存、選曲入力、難易度表のレベル表示、譜面先読み失敗、自動更新・署名検証の回帰テストを追加しました。
- 更新情報の生成・署名・配信をリリース処理へ追加し、`docs/auto-update.md` と操作・配布関連のドキュメントを整備しました。

## v0.4.0

### 改善

- IR接続先としてBMS-IRを追加しました。
  - スコア送信、ランキング取得に対応しました。
  - G-BATTLE による RANDOM 配置コピーは未対応です。

- ゲーム進行と音声再生を描画ループから分離しました。
  - 判定・入力取得・キー音の予約を描画とは独立して処理し、描画の停止や負荷が演奏へ影響しにくくしました。
  - ノーツ位置を描画時刻に合わせて補間し、FPS制限付き Immediate モードのフレーム間隔も安定させました。

- 外部BMSエディタ向けの Viewer モードを追加しました。
  - `-P` / `--viewer-play` で、Decide / Result を省略し、スコアを保存せずオートプレイできます。`-N` で開始小節、`-S` で実行中Viewerの停止を指定できます。
  - 再生終了後も待機し、後続の再生要求を同じウィンドウへ転送します。
  - 一時停止・再開、シーク、譜面のドラッグ＆ドロップによる切り替えにも対応しました。
  - `-B` による Battle 起動と、`--profile` による今回だけのプロファイル指定に対応しました。

- プレイ動画のオフライン書き出しを追加しました。
  - `export video <PATH> -o <OUTPUT.mp4>` で、Playスキンの入場から退場までを音声付きの H.264 / AAC 動画へ出力できます。オートプレイ・保存済みリプレイ、解像度・FPS・seed指定に対応します。

- CLIから起動時の設定を一時的に変更できるようにしました。
  - 配置、ゲージ、GAUGE AUTO SHIFT、ハイスピード、BGA、オートスクラッチ、画面モード・サイズ・モニターを指定できます。設定ファイルには保存しません。
  - Viewer モードや動画書き出しモードなどで利用する想定です。

- 設定UIとキーコンフィグを整理しました。
  - 設定画面をサイドメニュー形式にし、入力デバイスとキーコンフィグ、共通・選曲・プレイ・リザルト・キーモード別の割り当てを分類しました。OSの言語への追従と設定の自動保存にも対応しました。
  - ハイスピード・レーンカバー・描画判定調整のショートカットを変更できるようにし、数字キーとテンキーの操作を統一しました。選曲操作方式も設定画面から選択できます。
  - Decide / Play 中も一般・音声・映像・連携・録画設定を編集・保存できるようにしました。音声出力の再構築は選曲復帰まで保留します。
  - Notes Offset / Bar Line Offset の位置・サイズ・透明度を編集できるようにし、スキン選択肢の幅と長いライブラリ名の表示を整えました。

- HS設定を拡充しました。
  - 従来の直接倍率方式（Classic）とFloatingに加え、20段階のNormalハイスピードと5種類のHS設定を追加しました。

- 未プレイ・FAILED・各クリアランプ未満を条件とするランダム選曲を追加し、各項目の表示を個別に切り替えられるようにしました。
- 未処理の通常ノートを判定確定まで判定ラインへ留める表示設定を追加しました。(LR2風)
- キー音の自動再生するオプションを追加しました。

### 修正

- 起動前のシステム音声読み込み・音量解析をバックグラウンドへ移し、コースリンクの全件修復を初回描画後へ遅らせて起動待ちを軽減しました。フルスクリーンでのsurface初期化失敗からの復旧も改善しました。
- Luaスキンのランダムパーツを画面に入るたびに再抽選し、共有モジュールのPractice状態を現在のセッションへ同期するようにしました。
- HIDDENカバー操作の方向、無効なカバー操作からハイスピード操作への切り替え、Floating使用時のカバー補正の二重適用を修正しました。
- Viewer のシーク時にBGM・BGA・自動キー音・Battle相手の再生位置がずれる問題を修正しました。一時停止位置からの再開、変換前キーモード、READY演出のタイマーも保持します。
- G-BATTLE の相手の判定・タイマー・スコア表示を修正し、HCNの継続ゲージ変化を反映するようにしました。Autoplay Battle の相手はMineを踏まず、リプレイ判定へ実入力が混入しないようにしました。
- コースのコンボ切断条件と全ステージの判定に基づくランプ計算、譜面のノート数集計を修正しました。
- IR送信の識別子衝突、並行更新時のベストスコア競合、コースランキングの取得順を修正しました。未送信ジョブは元のアカウントへ紐付け、現在の送信を優先して処理します。
- BMZ IR Web のブラウザーセッションに認証情報の失効を反映し、コース送信で同じリクエスト識別子に異なる内容を送った場合の検証を追加しました。
- スコア履歴削除時に集計値が失われる問題を修正しました。曲スキャン成功後に存在しない譜面パスを整理し、検索結果を有効な曲フォルダに合わせました。
- 動的なLuaランク値、数値形式の `isRefNum`、テキスト縮小、2体目のPMchara、E1 / E2パネルタイマーの互換性を改善しました。
- LR2 Battle のゲージ・スコア表示と、プロファイルのBGA拡縮設定の継承を修正しました。次ランク差分、ライバル名、選曲スキンの譜面レベル表示も修正しました。
- フルコンボ演出を実際の判定に基づいて表示し、最終ノート後のフェードアウトを現在のプレイ状態から開始するようにしました。
- スキンのGPUキャッシュに上限を設け、未使用テクスチャを解放するようにしました。未対応サイズのテクスチャ、循環・過深度のJSON includeを検出し、Lua実行量の制限を描画フレーム単位でリセットします。
- bitmap fontの画像変更がキャッシュへ反映されない問題、選曲BGMの再開時の音切れ、破損PCMで譜面全体が無音になる問題を修正しました。
- ASIO切り替え時にWASAPI出力モードが残る問題と、Battleで無音化したノーツが音量解析へ含まれる問題を修正しました。
- キー設定の既定値復元とScratchの競合警告、設定画面のID衝突、描画判定調整の反映と長押し時の自動調整切り替えを修正しました。
- ウィンドウ移動・サイズ変更中も描画を継続するようにしました。Windows配布版をターミナルから起動した場合にヘルプ・CLIエラーを表示し、出力リダイレクトも保持します。
- IPFS RPC がHTTPメソッドを拒否した場合、POSTで再試行するようにしました。

### テスト・開発環境

- 描画停止時の判定・音声の独立性、入力・音声の遅延、フレーム間隔とスナップショット更新周期を検証・計測できるようにしました。
- Viewer、動画出力、CLIの設定非保存、設定UI・キーコンフィグ、G-BATTLE、コース・IR、Lua実行時評価、スキンキャッシュの回帰テストを追加・更新しました。
- `docs/cli-overrides.md`、`docs/video-export.md` と操作・互換性関連のドキュメントを整備しました。

## v0.3.0

### 改善

- プレイオプションと Practice を拡張しました。
  - beatoraja 互換の `EXPAND JUDGE`、`CONSTANT`、`JUDGE AREA`、`LEGACY NOTE`、`MARK NOTE`、`BPM GUIDE`、`NO MINE` と、譜面モディファイア `SCROLL` / `LONGNOTE` / `MINE` / `EXTRA NOTE` を追加しました。アシスト使用時はランプとプレイ回数だけを保存し、スコア・リプレイ・IR は更新しません。
  - Practice で小節範囲、再生速度、ゲージ、判定ランク、グラフを設定し、指定区間を繰り返し練習できるようにしました。専用コントローラーだけで設定・開始でき、ラウンド間では譜面・音源・BGAを再利用します。Practice は独立したセッションモードとして autoplay とスコア保存を無効にします。
  - `SP TO DP`、`7K TO 9K`、`7K TO 6K` のキーモード変換を追加しました。7K→9K は6種類の配置と3種類のScratch処理に対応し、7K規則では7Kの判定・ゲージ・スコアidentityを維持してリプレイを現在の変換設定へ投影します。
  - 120Hz の論理フレーム補正を使う LM 近似 S-RANDOM 配置方式を追加しました。緑数字、レーンカバー、判定表示などは 4K・5K・6K・7K・9K・10K・14K のキーモード別に保存します。

- コース作成・管理と選曲操作を拡張しました。
  - コース・段位の作成、編集、削除、beatoraja 互換 JSON の import / export をアプリ内へ追加しました。選曲画面からも最大10譜面を追加し、並べ替えてローカルコースを作成できます。
  - 現在のキーモード、レベル、BPM、曲数から LR2 RANDOM MIX コースを生成できるようにしました。
  - コース構成譜面を定義順に確認し、未所持譜面を難易度表の取得情報から一括ダウンロードできるようにしました。
  - LR2 難易度フィルター、難易度表レベルと譜面レベルの表示切り替え、選曲画面でのリプレイスロット選択に対応しました。

- 選曲画面と設定UIを拡張しました。
  - 選曲画面のフォルダから、セッションモード、キーモード変換、アシスト、表示、リプレイ、UI、音声、映像設定を変更できるようにしました。選曲 skin の設定 event、`GUIDE SE`、`CONSTANT`、`KEY CONFIG` も実際の設定へ接続します。
  - 設定行へ項目名・現在値・説明・編集中表示を公開し、skin property とファイル候補には前後の項目へ移動するボタンを追加しました。
  - egui のプロフィール設定からキーボード、コントローラーボタン、軸、共通操作、キーモード別操作を割り当てられるようにしました。選曲画面から開くショートカットも変更できます。
  - beatoraja 互換の数字キー操作を追加し、Autoplay / Replay の長押し速度変更をゲーム時計・音声へ反映しました。Result / Course Result からは選曲画面と同じ `OPEN IR` 割り当てを利用できます。

- beatoraja のリプレイを取り込めるようにしました。
  - 設定画面と `replay import` CLI から、player / replay ディレクトリまたは単一 `.brd` を現在のプロファイルへ取り込めます。
  - 5K / 6K / 7K / 9K / 10K / 14K、DP option、ゲージ、配置 seed、BMS RANDOM、H-RANDOM 閾値、Scratch 方向を Replay v6 へ変換し、コースリプレイも既存のコース再生経路へ登録します。
  - 大量 import はバックグラウンドで進捗表示・キャンセル・差分スキップを行い、DB 更新をまとめて処理するようにしました。既存の BMZ リプレイスロットは明示的に上書きしない限り保護します。

- G-BATTLE と IR 連携を拡張しました。
  - セッションモードを `NORMAL` / `PRACTICE` / `AUTOPLAY` / `AUTO BATTLE` / `G-BATTLE` に整理しました。G-BATTLE は選曲画面の通常操作から選択でき、相手データ自体はセッションモードと独立して扱います。
  - IR ランキングから相手を選び、公開リプレイを独立した2P側のゴーストとして全キーモードで再生できるようにしました。相手の option・seed・ゲージ・Scratch を再現し、自分の結果は通常どおり保存・送信します。
  - BMZ IR Web にライバル登録・解除、逆ライバル一覧、共通譜面の EX SCORE / MIN BP 比較、自分とライバルのランキング scope を追加しました。
  - 大規模なライバル比較は D1 上で集計・ページングし、Worker の CPU・メモリ負荷を削減しました。
  - Result skin へ送信前の IR 順位を公開し、BMZ IR / rianIR のランキング表示と G-BATTLE 候補をスコア更新後に再取得するようにしました。

- 音声出力、音量正規化、表示遅延を改善しました。
  - Windows に event-driven・MMCSS 対応の WASAPI 排他出力を追加し、対応形式とバッファを endpoint ごとに交渉するようにしました。排他適用に失敗した場合は以前の設定へ戻します。
  - プレビューと system BGM にゲート付き全体音量、最大3秒区間、sample peak による正規化を追加しました。譜面の音量解析はPCMをまとめてmixし、プレイ時の master / key / BGM volume を加味します。
  - 対応済みのPCM WAVは専用decoderで直接読み込み、非対応形式だけFFmpegへフォールバックすることで、多数のWAVを含む譜面のロードを高速化しました。
  - 描画の frame latency を `Auto` / `LowLatency` / `Stable` から選択できるようにし、surface の再構成で即時反映します。

- skin 互換性と診断機能を拡張しました。
  - Lua skin の未推論 callback を永続 VM で実行する fallback と、対応済み function も実行時評価する `--lua-skin-runtime compat` を追加しました。Lua の `print` はスキンパス付きでデバッグログへ記録します。
  - 過去の beatoraja と同じ論理 `package.path`、4 / 8 / 12 枚構成のグルーヴゲージ、Decide の STAGEFILE、Play の PMchara 基本描画に対応しました。filepath の同名候補が別ディレクトリにある場合も、定義したワイルドカード位置から選択します。
  - 初回プレイ、rule mode、LN policy、変換前・実効 key mode、譜面モディファイア、session mode、ランク境界・差分、autoplay、IR 前回順位などを Select / Decide / Play / Result skin へ公開しました。
  - modified LR2 の FAST / SLOW ref、LR2 の乗算判定 overlay、WMII の次ランク表示に対応しました。
  - PeacefulPlay 1.2.0 のキーロガーについて、CHATTERING ALERT、1秒周期のNPS、押下単位の判定集計を元スキンの規則へ合わせました。
  - 日次ローテーション・10世代保持の診断ログを追加し、起動失敗や panic を含むログをコンソールのない環境でも回収できるようにしました。

- Windows の大規模な曲ライブラリで Everything 1.5 の IPC インデックスを利用できるようにしました。利用できない場合は曲 root ごとに通常のファイル探索へフォールバックします。

### 修正

- macOS Intel 版の配布物が High Sierra で起動できない問題を修正しました。
  - Intel 版は macOS 10.13、Apple Silicon 版は macOS 11.0 を最小要件としてビルドし、FFmpeg・音声・TLS が新しい macOS API を強参照しないようにしました。
  - 配布時に Mach-O の最小 OS バージョンと未対応 API の参照を検査し、互換性の後退を検出するようにしました。
- `SPIRAL`、`H-RANDOM`、`ALL-SCR`、`RANDOM-EX`、`S-RANDOM-EX` を Light Assist として扱い、ランプ限定保存でもプレイヤー統計を更新するようにしました。保存後の選曲表示、skin の保存可否表示も修正しました。
- Practice の HS-FIX、判定ランク、再生速度に応じた入力時刻・緑数字・判定幅、GAUGE AUTO SHIFT を beatoraja に合わせました。設定済みPlay skinを使用し、完走・途中終了・失敗・準備中退出でskinの終了タイマーを通って設定画面へ戻るようにしました。
- Practice 設定中だけeguiがプレイ入力を占有し、通常プレイや Select / Decide / Result では未消費入力を正しく各画面へ渡すようにしました。
- アプリ終了時に非同期のリザルト保存が失われる問題と、LN policy・DOUBLE option・rule mode が異なるリプレイスロットを誤って表示・再生する問題を修正しました。保存対象外のキーモード変換は通常リプレイ再生時にも適用しません。
- 移動した譜面を含むコースのリンク修復、変更後のコース制約・履歴・リプレイ検証、RANDOM MIX の BPM フィルター適用を修正しました。RANDOM MIX の結果は IR へ送信せずローカルだけに保存します。
- HS-FIX の MAIN BPM へ不可視ノート・Mine が混入する問題、1未満の正の BPM が丸められる問題、除外レーン適用時のリザルトノート数を修正しました。ノーツ表示時間は重複保存せず緑数字から導出します。
- WASAPI 排他出力がendpointエラー後に無音のままになる問題を修正し、bounded exponential backoffで再接続するようにしました。CPALの出力停止後のゲーム時計と、音量正規化で実際のプレイmixを考慮しない問題も修正しました。
- BSS の終点を元方向のScratch releaseでも判定し、直後の反転入力を二重判定・Mine・キー音へ流さないようにしました。
- DPの2Pターンテーブルが1Pと逆方向へ回転する問題を修正しました。
- Select の scene / bar / option timer を初回present後に開始し、`TIMER_STARTINPUT` を実際の発火フレームで保持するようにしました。destination timerと画像animationの時計を分離し、開始演出中は非表示skinのGPU uploadを保留します。
- cursorが利用できない入力中のskin hover、選曲フォルダ・コースに譜面のkey modeが表示される問題、デフォルトskinのHS-FIXラベル、設定行の編集中表示を修正しました。
- Windows で譜面フォルダを開く際、DBの正規化パスをnative pathへ戻してローカルドライブとUNC pathを正しく開くようにしました。
- プレイスキンが指定していない `BACKBMP` を画面全体へ自動表示する問題と、BMS `#TEXT` をスキン外の固定パネルへ自動表示する問題を修正しました。
- NEXT ランク差分の符号・境界・数値画像、LR2 の乗算判定 overlay、Lua runtime の `nil` draw 判定、WMII の `MAX-` 無効時の次ランク表示を修正しました。
- rianIR ランキングが20件へ減る問題と、Result が今回の送信前順位を失う問題を修正しました。別taskやCLIが送信jobを先に処理した場合も、保存済み送信応答から順位を復元します。

### テスト・開発環境

- アシスト、Practice、キーモード変換、S-RANDOM、キーモード別設定、選曲内設定、キー設定、コース編集・RANDOM MIX、G-BATTLE、beatoraja replay import、WASAPI 排他、音量正規化、BSS、Everything、Lua runtime、skin state の回帰テストを追加・更新しました。
- Replay を v6 へ更新し、過去の v1〜v5 を従来の既定値で読み込める互換性を維持しました。FFmpeg binding は v9 へ更新しました。
- Rust 1.98 の新しい lint に対応し、workspace全体をwarnings-as-errorsで検証できる状態を維持しました。
- macOS の x86_64 / arm64 配布物、FFmpeg source build、最小 OS バージョン、禁止シンボルを release workflow で検証するようにしました。
- `README.md`、`docs/controls.md`、`docs/everything.md`、`docs/hs.md`、`docs/ir.md`、`docs/licenses.md`、`docs/ln.md`、`docs/packaging.md`、`docs/rian-ir.md`、`docs/score-persistence.md`、`docs/skin.md` を更新しました。

## v0.2.1

### 改善

- ノーツ落下の滑らかさを改善しました。
  - 音声 callback の buffer 単位で階段状に進んでいたゲーム時刻を monotonic time から連続計算し、高FPS時のノート移動を滑らかにしました。

- アナログスクラッチの ON / OFF 設定機能を追加しました。
  - ON / OFF、感度、停止閾値を論理 controller slot ごとに保存し、従来の共通設定は両側へ移行します。
  - OFF の場合はアナログ軸の端を通常ボタンとして扱い、回転 tick によるスクロールを無効にします。

- rianIR のライバル機能を拡張しました。
  - 初回 Select 描画後とログイン成功時にライバル一覧をバックグラウンド同期し、選曲中の7キーまたは skin event `79` で対象を切り替えられるようにしました。
  - 選択したライバルの EX score をプレイ target として使用し、未プレイ譜面では通常 target へフォールバックします。譜面再現モード、ライバル名・clear・min BP・lamp も Select skin へ公開しました。

- skin の互換性を改善しました。
  - 未変換の `.lr2font` を Shift_JIS 対応の bitmap font として直接読み込み、`#S` / `#M` / `#T` / `#R` と `DST_TEXT` の描画サイズを扱えるようにしました。
  - 同梱 mz-select / Luxez-Flat の RANDOM option panel を F-RANDOM / MF-RANDOM まで拡張し、既存の画像表示・選択枠・クリック範囲を維持しました。
  - Luxez-Flat Select skin のスコア差分、TIME、TOTAL / NOTES 比率を実行時の譜面・スコア値から描画するようにしました。

- Scratch と鍵盤の判定表示を分離できる BMZ skin 拡張を追加しました。 (S-FAST / S-SLOW)
  - 3つの判定領域と Scratch / Keys を組み合わせた6チャンネルを追加し、最新判定を timer `19010..19015`、PGREAT / FAST / SLOW 状態を option `19020..19045`、タイミング差を ref `19050..19055` で公開しました。
  - 従来の判定 timer・option・ref と800msの表示時間は変更せず、同梱 Antique skin で Scratch / Keys の FAST / SLOW を独立表示できるようにしました。

- プレイ終了時の処理負荷を削減しました。
  - 判定確定後のスコア、リプレイ、DB、リザルトグラフの保存を専用 worker へ移し、描画 thread を停止させず一度だけ保存するようにしました。

### 修正

- 10K / 14K / BATTLE で片側の判定が反対側の表示中コンボを書き換える問題を修正し、判定発生時のコンボを領域ごとに保持するようにしました。
- ランキング対象がない選曲行でも IR 接続状態と provider・ユーザー名を維持し、ライバル未プレイ譜面でもライバル名と比較欄を表示するようにしました。
- Select の検索ヒントが option panel の上へ重なる問題と、同梱 mz-select / Luxez-Flat の RANDOM ラベル、カーソル、枠、既存 option 画像の表示崩れを修正しました。
- READY 中に decide BGM を譜面開始までフェードアウトし、autoplay / replay / battle のモード表示が Play 中に消えないようにしました。
- コースリザルトのゲージグラフへステージ間の区切り線を追加しました。

### テスト・開発環境

- 1P / 2Pアナログスクラッチ、判定領域別timer・option・ref、rianIRライバル同期・target、LR2 bitmap font、バックグラウンド結果保存、同梱skin表示の回帰テストを追加・更新しました。
- `README.md`、`docs/controls.md`、`docs/rian-ir.md`、`docs/skin.md` を更新しました。

## v0.2.0

### 改善

- rianIR に正式対応しました。
  - AUTO を含むコースの実効 LN 種別を正規化し、コーススコアを rianIR へ送信できるようにしました。

- コースプレイのスコアの扱いを改善しました。
  - コーススコア LN policy ごとに保存するよう変更しました。既存データは `ForceLn` として移行します。

- Windows のゲームパッド入力に Raw Input backend を追加しました。
  - 設定画面で gilrs / Raw Input を選択でき、入力 backend の変更をアプリ再起動なしで反映できるようにしました。
  - GameInput は一部 HID コントローラーでプロセスごとクラッシュするため通常ビルドから無効化し、既存設定を gilrs へ移行します。実装は開発用の `experimental-gameinput` feature に残し、Windows 配布物からは削除しました。

- 処理負荷を削減しました。
  - 難易度表更新、IR backlog、更新確認、曲 scan の DB 反映を Select 画面まで保留し、Play 中の描画や DB access との競合を避けるようにしました。
  - skin 設定画面では表示範囲内の property / filepath / offset 行だけを構築し、path context のキャッシュと変更スロットの直接追跡によって毎フレームの複製を削減しました。
  - コース中間リザルト中に次ステージの譜面・WAV・BGA をバックグラウンド preload し、コース選択時の全ステージ集計も非同期化して Play への遷移を高速化しました。
  - コース最終判定が確定した時点でスコア保存と IR 送信を開始しつつ、Play の終了演出は従来のタイミングを維持するようにしました。

- system bgm (選曲BGM/決定SE) が選曲画面に戻るたびに再抽選されるよう変更しました。

### 修正

- 10/14Kプレイ時にコントローラーが反応しない不具合を修正しました。
- beatoraja から取り込んだ MYBEST score に ghost が無い場合、再 import で現在のベスト履歴へ ghost を補完するようにしました。
- リザルトから retry した後、開始前に退出すると選曲画面の clear status が古いまま残る問題を修正しました。
- コースの `NoSpeed` 制約を HS 1.0、SUDDEN / LIFT / HIDDEN 0 で開始し、HS・緑数字・レーンカバー操作と一時状態の保存をすべて無効にするようにしました。
- コースの Decide 表示、stage metrics、結果確定、LN policy の扱いを統一し、Select 以外で option panel の開閉音が鳴る問題を修正しました。
- skin offset の適用対象・順序・原点・ID 条件を beatoraja に合わせ、Notes offset を変更しても通常ノート、LN cap / body、小節線がずれたり分離したりしないようにしました。

### テスト・開発環境

- Raw Input、gilrs controller slot、backend 即時切り替え、GameInput 設定移行、コース LN policy・rianIR送信・preload、skin offset、設定画面 virtualization の回帰テストを追加・更新しました。
- `docs/controls.md`、`docs/ir.md`、`docs/ln.md`、`docs/rule.md`、`docs/packaging.md`、`docs/licenses.md` を更新しました。

## v0.1.13

### 改善

- rianIR 連携を拡張しました。
  - rianIR が提供する難易度表、POPULAR、レビュー、Rivals RECENT、コースを選曲画面へ読み込み、起動時・30分間隔・F5 で更新できるようにしました。
  - 選曲・リザルト skin から global / self and rivals のランキング範囲を切り替えられるようにし、選択中 scope・利用可否・総人数を BMZ 拡張 ref / option `1964..1969`、切り替え操作を click event `-10003..-10005` で公開しました。
  - リザルトランキングをホイール、方向キー、ゲームパッド、スクラッチでスクロールできるようにしました。
  - BMZ IR / rianIR を設定画面の固定 provider として表示するようにしました。
  - F11 から rianIR の譜面ランキングを開けるようにしました。
  - rianIR スコア送信 API を最新のものに更新しました。

- プレイのセッションモードを4種類に拡張しました。
  - `NORMAL` / `AUTOPLAY` に加え、2P側をオートプレイ表示する `AUTO BATTLE` と自己ベストを再生する `BATTLE` を追加しました。
  - 5K / 7K の battle skin、beatoraja 互換の2P側ノート・判定・ゲージ timer、rival ref を接続しました。
  - 選曲 skin から4モードを区別する BMZ 拡張 ref `1970` を追加しました。

- 選曲画面に設定可能な仮想フォルダを追加しました。
  - MY BEST、更新履歴、クリア、ランク、密度、譜面特徴、レベル、NEW の組み込みフォルダを追加しました。
  - プロファイル別の TOML と型付きクエリで、階層、並び順、件数制限、連番・数値範囲をカスタマイズできます。

- BMSON の読み込み互換性を大幅に改善しました。
  - LN 種別と終端キー音、pulse 単位の STOP、`mode_hint` のレーン配置、Mine の damage と音、継続音源の区間再生、同時発音するキー音・BGA layer を保持するようにしました。
  - `chart_name`、level、judge rank、TOTAL を beatoraja 互換で扱い、難易度未指定時の自動判定を改善しました。
  - 長尺共有音源の PCM を区間間で共有し、音源 slice の取り込みを線形化して、ロード時間とメモリ使用量を削減しました。

- Lua / LR2 skin の互換性を改善しました。
  - Lua skin の entry directory と library root を分離し、兄弟 package、`skin/...` alias、`require`、source・font・audio を同じ安全な規則で解決できるようにしました。
  - LR2 の明示解像度、LuaJ / Gdx API を使う judge parts、LR2 skin の LIFT 追従、WMII の play state・autoplay graph・次ランク表示に対応しました。
  - skin が宣言した条件、property、resource ID を優先し、スコア差分、ランク、Notes graph、rhythm timer などの表示を beatoraja に合わせました。

- 描画とプレイ開始時の性能を改善しました。
  - 状態に依存しない skin destination とテキストレイアウトをキャッシュし、Select / Decide / Play / Result の毎フレーム処理を削減しました。
  - WAV の読み込み前に譜面情報と描画キャッシュを skin へ渡し、BMS の二重 parse を解消しました。
  - preload と frame pacing の計測情報を拡充し、present 済みフレームから FPS を算出するようにしました。

### 修正

- 同一位置の BPM 変更と STOP、極端に短い小節、コロン区切りの `#BPMxx` / `#STOPxx` を含む BMS で、時刻や曲長が beatoraja とずれる問題を修正しました。
- HS の範囲を beatoraja 互換の `0.01..=20.0` に統一し、リプレイ開始時にも選曲中の HS-FIX を維持するようにしました。
- 読み取れないファイルを含む曲フォルダで scan 全体が停止する問題と、Windows のパス表記差・拡張パス接頭辞によって曲 root や譜面が重複する問題を修正しました。
- ロード中・READY 中の退出でリザルトやスコアを生成せず、フェード後に選曲画面へ戻るようにしました。
- 外部 skin の省略 `loop`、検索文字列、Judge offset、ゲージ最大 timer、WMII の score graph / panel、LR2 の autoplay 条件などの描画差を修正しました。
- macOS の focus 判定と排他フルスクリーンの refresh rate 選択、補助パネルの位置保持、IR メニュー非表示時のランキング更新を修正しました。

### テスト・開発環境

- player、renderer、skin、chart、audio、gameplay、IR Web の大規模 module を責務ごとに分割し、strict lint / formatting を通る構成へ整理しました。
- rianIR、battle session、仮想フォルダ、BMSON、Lua / LR2 skin、音声共有、BMS timing、frame pacing の回帰テストを追加・更新しました。
- `README.md`、`docs/controls.md`、`docs/hs.md`、`docs/ir.md`、`docs/rian-ir.md`、`docs/select-folders.md`、`docs/skin.md` を更新しました。

## v0.1.12

### 改善

- rianIR 互換の IR 連携を追加しました。
  - ログイン、単曲・コーススコア送信、グローバルランキング取得に対応しました。
  - IR 接続先を BMZ IR / rianIR / Other から選べる preset 設定を追加しました。
  - 配布実行ファイルの SHA-256 client hash manifest を Windows / macOS / Flatpak 向けに生成し、rianIR の allowlist 登録に利用できるようにしました。
  - リザルト skin では primary IR provider の名称を表示するようにしました。

- 難易度表の初回取得をバックグラウンド処理に変更しました。
  - 初回描画後に最大4件を並列取得し、進捗・完了通知と手動取得要求の待ち行列に対応しました。

- CJK フォントを同梱しました。
  - Flatpak などシステムフォントが不足する環境でも、Noto Sans CJK を fallback として UI とスキン文字列を描画できます。

### 修正

- `KEYBOARD SUB` の割り当てが、PRIMARY 未設定時やキー設定の継承・再配置時に失われる問題を修正しました。
- rianIR のコース hash、クイックリトライ後のリザルト IR 応答、primary provider 名の表示を beatoraja 互換の挙動へ修正しました。
- macOS 配布時に bzip2 依存のライセンス判定でパッケージ生成が停止する問題を修正しました。
- クイックリトライ時に、古いプレイ結果がリザルトスキンのランキングに表示される不具合を修正しました。

### テスト・開発環境

- rianIR の provider / score・course payload / hash / リザルト照合、難易度表の並列取得、CJK fallback font、キー設定の回帰テストを追加・更新しました。
- `docs/rian-ir.md`、`docs/ir.md`、`docs/controls.md`、`docs/licenses.md` を更新しました。

## v0.1.11

### 改善

- IR の機能を拡張しました。
  - IR のスコア履歴をローカルの `score.db` へ取り込む機能を追加しました。

- 多言語 UI と文字描画を追加しました。
  - デスクトップ UI と IR Web を日本語、英語、韓国語、中国語（簡体・繁体）など6言語に対応しました。

- スキン互換性をさらに改善しました。
  - MILLIONDOLLAR Result skin に対応しました。
  - LR2 play CSV の signed number、libGDX `.cim` source、未推論 draw callback、observe timer、Result metadata / rotation を扱えるようにしました。
  - Result の target、ゲージ率、stagefile、IR 設定、スコアを Lua skin のロード時・描画時へ正しく渡すようにしました。
  - skin 解像度に応じた描画、dynamic option panel、専用 Select settings row、拡張 random label、各 scene の offset を追加しました。

- スキンの BMZ 拡張を追加しました。
  - key mode / Scratch 有無 / single・double play を ref・option `1903..1915` で参照できるようにしました。
  - E1〜E4 と UI の方向キーを option `1920..1927`、直近の press edge を timer `19000..19007` として skin の draw / runtimeEvent から利用できるようにしました。`triggerAction` による runtime flag 切り替えにも対応します。
  - プロファイル単位の日次統計（プレイ数、clear 数、判定数、EX score、rate、rank、更新回数、直近曲名）を ref `1930..1959` で、コースの stage 数・EX score・gauge・BP・rate を `19100..19149` で公開しました。日次表示だけをリセットする event も追加しました。
  - Select の設定入口・カテゴリ・戻る・閉じる行を専用 row kind / ref `1960..1963` で表現し、`bmz_select_*` の動的 option text、画像不要の単色 `panel`、panel / text の click target を利用できるようにしました。
  - RANDOM の実配置を play / select skin の ref `450..469` から参照できるようにしました。

- ランダム練習とプレイ操作を拡張しました。
  - 7K random trainer と制約付き random trainer、antique random のレーンプレビュー、プレイ中 / 選曲中の random lane ref を追加しました。
  - random pattern を READY 前から表示し、レーン値に応じた色付け、profile option、skin 表示を連動させました。
  - 全 key mode の scratch HS 操作、key mode 別の play HS binding、select utility action、live play speed 設定に対応しました。
  - random seed を beatoraja 互換のものに変更しました。

- 未所持譜面の自動ダウンロード機能を追加しました。
  - IPFS / HTTP での譜面ダウンロード機能を追加しました。

- 音声、入力、描画性能を改善しました。
  - analog scratch の release timing と bounce filter を調整し、DX 9key / charge note 判定、beatoraja 互換 random seed、scratch rotation を修正しました。
  - chart video の frame buffering を制限し、stagefile / backbmp をプレイ開始前に preload するようにしました。
  - play snapshot の chart scan、folder lamp、song document flag、frame pacing、render latency、load spike を削減しました。
  - option / course result sound と declarative skin audio の再生に対応し、PCM の不完全 packet を安全に破棄するようにしました。

- 難易度表と設定画面を改善しました。
  - 難易度表名を設定画面に表示するようにしました。
  - 設定変更を profile、input、skin、score、audio の各状態へ同期し、legacy settings row と bundled option の hitbox を維持しました。
  - フルスクリーン表示、unlimited FPS、デバッグパネルの tracing log 表示を追加しました。

### 修正

- 設定編集中の右クリックの動作をフォルダクローズから設定キャンセルに変更しました。
- CJK / LR2 skin のフォント・数値表示、skin offset、設定行の衝突、スキンと入力設定の同期を修正しました。
- BGA / stagefile の preload、音声 packet、アナログ scratch の release / bounce、リザルト表示の互換性問題を修正しました。

### テスト・開発環境

- IR score import / pagination、6言語の UI・IR Web、CJK font、LR2 / Lua / libGDX skin、random trainer、audio / input、BGA preload の回帰テストを追加・更新しました。
- `README.md`、`docs/controls.md`、`docs/i18n.md`、`docs/ir.md`、`docs/ln.md`、`docs/rule.md`、`docs/skin.md` を更新しました。

## v0.1.10

### 改善

- スキン offset の保存と復元を key mode / slot 単位に分離しました。
  - 12 スロットごとに offset を保存し、旧形式の共通設定と path 単位の履歴は互換移行します。
  - プレイ開始前、セッション生成、ライブ反映で現在のスキン設定を一貫して使用するようにしました。

- FPS 表示を安定化しました。
  - 右上のオーバーレイと skin ref 20 に、beatoraja と同じ秒単位の確定 FPS を表示するようにしました。
  - フレーム間隔の EMA による表示の揺れと、オーバーレイ・スキン間の値の乖離を解消しました。

### 修正

- プレイ開始時に古い skin history の offset / height が復元され、現在の Notes offset と異なる表示になる問題を修正しました。
- Windows release packaging で FFmpeg の Debug 構成までビルドされ、処理時間が増えていた問題を修正しました。

### テスト・開発環境

- スキン slot 分離、旧設定・履歴移行、offset 優先順位、FPS 表示の回帰テストを追加・更新しました。
- Windows 配布用に Release 専用 vcpkg triplet と staging を追加し、packaging / licenses ドキュメントを更新しました。

## v0.1.9

### 改善

- スキン互換性とスキン連携を大幅に拡張しました。
  - WMII RESULT SKIN に対応しました。
  - ModernChic skin に一部対応しました。
  - Luxe Flat を同梱スキンとして追加しました。
  - その他、スキン互換性を高める修正を行いました。

- 音声、動画、リトライ性能を改善しました。
  - Windows 共有出力での音声遅延を抑えるオプションを追加しました。 (IAudioClient3)
  - quick retry では chart、キー音、静止画 BGA、動画 decoder を可能な範囲で再利用するようにしました。
  - 選曲 preview の音量正規化目標をプレイ音声と同じ -6 LUFS に統一しました。

- プレイ操作と譜面互換性を改善しました。
  - 最終ノーツ後の Play 終了演出を Escape でも開始できるようにしました。
  - 先頭ノーツが早い譜面の開始位置を遅らせ、余裕を持って第一ノーツを処理できるようにしました。
  - Judge Algorithm の設定値を Combo / Duration / Lowest に統一し、既存の Score 設定を互換変換するようにしました。
  - preload 中も HS 操作とキービーム表示を反映し、リトライ時の入力・音声・BGA 状態を安定させました。

- 大きな難易度表を扱うときの選曲画面を高速化しました。
  - 譜面、スコア、リプレイスロット、解析情報を複数件単位で取得し、重複ハッシュをまとめて検索するようにしました。
  - 難易度表レベル検索用の複合インデックスを追加しました。

### 修正

- WMII / Luxe Flat / m-select スキンで、Result のパネル、グラフ、ゲージ、CLEAR 分岐、2P RANDOM、LIFT 表示が誤る問題を修正しました。
- コース途中落ち時に未プレイノーツが BP へ含まれない問題を修正しました。
- Windows のスクリーンショットをクリップボードへコピーする処理が遅延・失敗する問題を修正しました。
- 設定画面に実行環境で利用できない音声・映像 backend が表示される問題を修正しました。

### テスト・開発環境

- WMII、Luxe Flat、mz-select、ModernChic の実スキン回帰テストを追加・更新しました。
- IAudioClient3、同曲リトライ、BGA / 動画 timestamp、コース結果、難易度表の一括検索、Result / Select 操作の回帰テストを追加しました。
- `README.md`、`docs/controls.md`、`docs/hs.md`、`docs/packaging.md`、`docs/licenses.md` を更新しました。

## v0.1.8

### 改善

- Windowsのゲームパッド入力にGameInput backendを追加しました。 (main-thread polling方式)
  - ゲームパッドbackendの既定値と自動選択はgilrsを優先し、Windowsでgilrsを初期化できない場合はGameInputへfallbackします。
  - GameInputのreading時刻を判定へ渡し、1P / 2P割り当てをstable device IDで保存するようにしました。
  - GameInputの履歴取得をデバイス単位にし、曲終了後や一時切断後も入力と割り当てが復帰するようにしました。

- IR と難易度表の運用機能を拡張しました。
  - 通常プレイの譜面時間を IR payload へ送信し、日次の成果レポート（clear、EX score、min BP）を確認できるようにしました。

- ハイスピード関連の挙動を微調整しました。
  - NHS / FHS で異なる変更刻みを設定できるようにしました。
  - HS-FIX に応じたモードで開始するようにしました。
    - HS-FIX OFF の場合 NHS で開始されます。
    - HS-FIX OFF 以外の場合 FHS で開始されます。
  - 目標緑数字が変更される条件を変更し beatoraja の仕様に近づけました。

- コース、BGA、音声・動画の互換性を改善しました。
  - 譜面取り込み時に未解決のコース譜面リンクを SHA256 / MD5 で補修し、コース完走 clear を最終ゲージと失敗状態から正しく算出するようにしました。
  - 選曲 preview の音量正規化機能を追加しました。
  - カーソル音の過剰な重複再生を抑えました。
  - READY 前のプレイ intro、タイトル、BGA mode / expand を実セッションと揃え、コース開始時の表示切り替えを滑らかにしました。

### 修正

- GameInput の起動時 stack 使用量、プレイ遷移後の履歴再開、gilrs を既定値とする設定互換性を修正しました。
- OBS の有効 / 無効切り替えが再起動まで反映されない問題を修正しました。
- 自動判定調整が機能していない問題を修正しました。
- 選曲 skin の genre 表示、14K turntable の回転方向、コース開始時の曲タイトル表示を beatoraja 互換へ修正しました。
- 動画 BGA の開始時刻がズレることがある問題を修正しました。

### テスト・開発環境

- GameInput runtime とライセンスを Windows 配布物へ同梱し、入力設定・controls / packaging / HS 仕様書を更新しました。
- IR daily report / difficulty table 同期、GameInput、READY / BGA preload、preview 音量、動画 timestamp、コース clear / link の回帰テストを追加・更新しました。

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
