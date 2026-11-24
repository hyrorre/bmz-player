# OpenLR2 / BMZ LEGACY 一括比較

OpenLR2のローカルスコアDBにある全譜面と単曲リプレイを列挙し、保存済みリプレイの入力を、このチェックアウトのBMZ LEGACY判定エンジンに与えます。OpenLR2がリプレイに記録したP1/P2判定（op 210/211）とEXスコア・各判定数を比較します。DBとリプレイは読み取り専用で扱い、IRへ接続しません。

## 実行

BMZのビルド環境を設定したPowerShellで、リポジトリのルートから実行します。

```powershell
cargo build -p bmz-gameplay --example legacy_compare --locked --offline
python tools/legacy-compare/compare.py --openlr2 D:\OpenLR2
```

このPCの既存ビルドツール一式を利用する場合、次の1行で環境設定・ビルド・全件比較まで実行できます。

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File D:\BMS\bmz-upstream-legacy-pr\tools\legacy-compare\run.ps1 -BuildToolsDirectory D:\Tools\bmz-build
```

`-EnvironmentScript <ps1>`で別の環境設定スクリプトも使えます。既にビルド済みなら`-SkipBuild`、一覧のみなら`-InventoryOnly`を指定できます。ビルド環境のFFmpeg DLLディレクトリを実行時のPATHにも設定してください。

`target`が別の場所の場合は`--runner <exeの絶対パス>`を指定します。

```powershell
# ビルドなしで全譜面の対応状況を調査
python tools/legacy-compare/compare.py --inventory-only
# 1譜面だけ再実行
python tools/legacy-compare/compare.py --profile lightning_23k --md5 751738dea1169c5c39db935adfc9e85f --output .local/legacy-air
```

出力は`.local/legacy-compare/`の`results.csv`（Excel向け）、`results.jsonl`（判定数・最初の判定列の差分）、`summary.json`、`report.md`です。`traces/`には各プレイの全判定を1行ずつ出し、記録判定・再計算判定・時刻・BMZレーン・ノートID・判定差分時刻を確認できます。個人の曲名・パス・スコアを含むためGit管理対象外です。`--limit N`は実行数のみを制限し、残りは`not_run`として一覧に残します。

## 結果の意味

| status | 意味 |
|---|---|
| match | 完走分の判定列・各判定数・EXスコアがすべて一致 |
| judgement_sequence_mismatch | 判定数・EXスコアは一致するが、判定列が異なる |
| mismatch | 完走分の記録判定数と再計算判定数に差がある |
| incomplete_reference | 記録の判定対象数が譜面ノーツ数と異なる。途中終了等の可能性があり、合否判定から除外 |
| unsupported_or_error | 未対応オプション・形式、MD5不一致、破損、読込エラー等。reasonに理由を記録 |
| missing_replay / missing_chart | 比較に必要な入力または譜面がない |
| not_run | 一覧のみの実行または実行数制限 |

同じ譜面の異なるプロファイルは別行です。`score`と、そこに同一ハッシュがない`imported_score`を対象とします。リプレイのみ存在する譜面も追加します。コースのステージリプレイは対象外です。

`database_best_ex`は集約自己ベスト、`reference_ex`は**対象リプレイの実プレイ**の記録です。別プレイに由来し得るため、集約自己ベストとの差を判定不具合として数えません。

## 対応範囲と比較の限界

- SP 5K/7K・DP 10K/14K、NORMAL/MIRROR/RANDOM（randFixを含む）、標準LNと標準LNのスクラッチに対応します。RANDOMは記録seedとLR2乱数実装で復元し、正解判定からの配置推測はしません。DPはop 210/211を合わせて比較します。
- 地雷は、BMZ LEGACYのレーン内処理で再計算し、OpenLR2が`ApplyJudgeMine`で記録した被弾（op 214/215、値はダメージ）と回数・ダメージを比較します。被弾数が違えば`mismatch`です。`mine_hit_ms_deltas`に各被弾の時刻差、`bmz_mine_hit_list`・`reference_mine_hit_list`・`chart_mines`に全件を出します。押しっぱなしの判定には、リプレイの押下・解放から求めた保持状態を使います。計測ログ（`--poll-trace`）では保持状態を照合できないため、地雷入り譜面は除外します。
- S-RANDOM、ALL-SCRATCH、アシスト、EXTRA、ピッチ変更、動的判定変更などは理由を記録して除外します。
- OpenLR2を新しく起動して採点するツールではありません。記録したOpenLR2の実行バージョンはリプレイだけでは確定できません。
- BMZの実際のimportとLEGACY JudgeEngineを使いますが、GameSession全体、HAZARD等のゲージによる途中終了、スコア保存処理は再現しません。
- LEGACY専用のBMS時刻を使い、1ms周期でレーンごとに入力・見逃しを処理します。元のOpenLR2の全ポーリング時刻は保存されていません。また、記録は時計41、実際の判定は時計142で行われるため、記録の入力時刻をそのまま実際の判定引数とはみなせません。結果の完全一致を確認するには以下の計測経路を使います。
- `match`も両アプリの全面的な一致を意味しません。`mismatch`は譜面タイミング、ポーリング、配置復元、判定処理を調べるための候補です。
- 判定列の順序一致は`sequence_equal`にも別記します。同時刻のレーン処理順だけが異なる場合も、厳格な一致とは数えません。op 210/211には個別レーン・ノートIDがないため、元の判定を1ノート単位で対応付けられない場合があります。

ローカルの結果はPRへ自動投稿しません。

## 実際のOpenLR2判定引数による比較

`build_openlr2_trace.ps1`は既存OpenLR2ソースとビルド済みライブラリを読み、元のソース・本体を変更せず`.local/openlr2-parity-trace/`へ計測版をビルドします。通常は無効で、`OPENLR2_PARITY_TRACE`指定時のみ実際の`ProcNoteOnTiming`引数、ノートの倍精度時刻、判定幅、各判定とEXを記録します。入力のあったポーリングや状態変化だけを記録します。

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/legacy-compare/build_openlr2_trace.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File tools/legacy-compare/start_openlr2_trace.ps1
```

起動スクリプトは`OPENLR2_NO_CD=1`と通常のOpenLR2フォルダを使い、普段の設定・曲・入力で起動します。NORMALかRANDOMで1曲プレイし、通常終了してバッファを保存します。既存capture.csvは上書きしません。再採取には`-TracePath <新しいCSVパス>`を指定します。

BMZのビルド環境を設定してランナーをビルド後、以下で照合します。MD5はCSVから読みます。過去のリプレイが自己ベスト時のままでも、配置seed・判定幅・判定時刻は新しい計測ログを使用します。

```powershell
cargo build -p bmz-gameplay --example legacy_compare --locked --offline
python tools/legacy-compare/compare.py --poll-trace .local/openlr2-parity-trace/capture.csv --output .local/legacy-captured-comparison
```

同じ判定になっただけでなく、実際のノート時刻も一致するか確認します。実測の引数を使うため、スコアから時刻・配置を推測して補正する処理はありません。ゲージ全体の再現はこの比較の対象外です。異なるOpenLR2ビルドのソースとライブラリを混在させず、同じビルドツリーを指定してください。

### 初版のハッシュ未記録CSV

初版の計測実行ファイルにMD5取得箇所の誤りがあり、MD5列が空のCSVが生成される場合がありました。自動的に別譜面を選ぶことはせず、`--md5 <別の記録から確認した譜面MD5>`を必須とします。指定したファイル自体のMD5と、計測ノート時刻・配置seedを照合し、`trace_hash_present=false`を結果に残します。現行計測版はスコア保存と同じ選曲リストのhashを記録します。

## OpenLR2判定オラクルとの直接比較

保存リプレイの記録時刻は時計41で、OpenLR2が実際に判定に使った時刻ではありません。そのため、記録との差だけではBMZの誤りか記録の限界かを区別できません。`openlr2_oracle.cpp`は、ビルド済みの未改変`OpenLR2Lib`をリンクし、画面・音声なしで`ParseBmsFile`と`ProcNoteOnTiming`を1msごとに呼びます。同じキー入力をオラクルとBMZに与えれば、両プログラムの挙動だけを比べられます。

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/legacy-compare/build_openlr2_oracle.ps1
cargo build -p bmz-gameplay --example legacy_compare --locked --offline
python tools/legacy-compare/oracle_vs_bmz.py --results .local/legacy-compare --oracle .local/openlr2-oracle/openlr2_oracle.exe --runner target/debug/examples/legacy_compare.exe --output .local/oracle-vs-bmz --tolerance 0
```

- 入力はテキストで、譜面・鍵数・配置（RANDOM、seed、固定キー、皿配置、DPフリップ）・ゲージ・キーの押下/解放です。`replay_to_oracle.py`が`.lr2rep`から作ります。
- 出力は判定（`J`）、地雷（`M`）、ポーリング後のゲージ6種・EX・コンボ・判定数（`S`）、全ノートの倍精度時刻（`N`）、閉店（`E`）です。判定は`ApplyJudgeNote` / `ApplyJudgeMine`のリプレイ記録フックから取るので、OpenLR2自身が記録する順序と同じです。
- ゲージ確認と閉店はScene04_Playと同じく各ポーリングの前に行います。
- `oracle_vs_bmz.py`は、compare.pyで比較したプレイごとに、同じ入力をオラクルとBMZのランナー（`oracle_output_path`付き）へ与え、出力を1行ずつ比べます。対象は、判定のポーリング時刻・レーン・判定値、地雷の時刻・レーン・ダメージ、各ポーリング後のGROOVE / HARD / DEATH / EASYのHP・EX・コンボ・最大コンボ・判定数・処理済みノーツ数、閉店です。HPは`--tolerance`（既定1e-3、0で完全一致）以内を一致とします。
- BMZ側はGameSessionのLEGACY 1msポーリングと同じく、レーン順に判定と地雷をスコア・ゲージへ反映し、ポーリング後に閉店を確認します。

### CustomIRの送信値

OpenLR2はリザルトでCustomIRモジュール（Stellaverseなど）へ`IRScoreV1`を渡します（`LR2_customir.cpp`）。オラクルは`E`行のあとに、そのうちプレイから決まる値を出力します。

- `R,total` / `R,col0`〜`R,col19`：早/遅のPGREAT〜POOR、コンボ切れ、FAST / SLOW、処理済みノーツ数、ノーツ数（全体とレーンごと）
- `R,score`：最大コンボ、ゲージ種別、スコア（20万点満点）、EX、レート、クリア種別、ノーツ数
- `R,hp`：ゲージ6種の最終値。未処理のノーツが残ると、OpenLR2はすべて0にします
- `R,runtime`：`song_runtime`（グラフの時間軸）
- `G,hp0`〜`G,hp5` / `G,combo` / `G,exscore`：1000点のグラフ
- `R,ghost`：LR2形式のゴースト

BMZ側は`bmz_gameplay::lr2_score::Lr2ScoreRecorder`が、判定イベント（LN始点の、スコアに影響しないイベントを含む）と地雷から同じ値を組み立てます。`oracle_vs_bmz.py`と`fuzz.py`はこれらを許容誤差0で比べ、結果の`submission`に最初の差を書きます。

差を調べるときは、オラクルに`columns 1`を与え、ランナーを環境変数`BMZ_LR2_COLUMNS=1`付きで動かすと、両方がレーンの集計の変わったポーリングごとに`X,ms,レーン,…`を出力します。

## 譜面の読み込みの全数比較

`loader_vs_oracle.py`は、OpenLR2の`song.db`にある譜面を1つずつオラクルとBMZのランナー（`dump_path`付き）で読み込み、ノーツ数・判定幅・ゲージの増減量と、全ノート・LN終点・地雷の時刻（µs単位に切り捨て）を比べます。

```powershell
python tools/legacy-compare/loader_vs_oracle.py --oracle .local/openlr2-oracle/openlr2_oracle.exe --runner target\debug\examples\legacy_compare.exe --output .local/loader-all --jobs 10
```

- `--sample N`で無作為にN譜面、`--only <ファイル>`でファイルに書いたMD5の譜面だけを比べます。
- 結果は`results.jsonl`（譜面ごと）と`summary.json`（集計）です。
- オラクルへ`objects 1`を与えると、OpenLR2が読み込んだ全オブジェクトを、最終的な並び順で`O,番号,チャンネル,時刻,値`として出力します。時刻のずれを調べるときに使います。

## 合成入力での差分テスト

`fuzz.py`は、乱数で作った譜面とプレイをオラクルとBMZのランナーに与え、読み込みとプレイの結果を比べます。

- 譜面には、LR2の読み込みで扱いが変わる要素を混ぜます。BPM・拡張BPM・STOPの変化、小節長、細かい分割、LNチャンネル、`#LNOBJ`、地雷、不可視ノート、同じ位置のオブジェクト、`#RANDOM`、0小節目先頭のオブジェクト、範囲外の文字などです。
- キーモードは、LR2が曲を登録するときの規則をPythonで別に移植して決め、BMZの判定と比べます。
- プレイは、正確な押下・早押し・遅押し・見逃し、LNの保持と早離し、地雷の被弾、無関係な押下を混ぜ、配置（正規・MIRROR・RANDOM）とゲージも乱数で選びます。
- 読み込みは`loader_vs_oracle.py`、プレイは`oracle_vs_bmz.py`と同じ方法で、許容誤差0で比べます。

```powershell
python tools/legacy-compare/fuzz.py --oracle .local/openlr2-oracle/openlr2_oracle.exe --runner target\debug\examples\legacy_compare.exe --output .local/fuzz-1 --count 500 --seed 1 --jobs 10
```

結果の`status`は次のとおりです。一致しなかった件の譜面・リプレイ・両方の出力は`cases/`に残ります。

| status | 意味 |
|---|---|
| `equal` | 読み込みとプレイが一致 |
| `equal_no_notes` | 読み込みが一致したノーツの無い譜面。OpenLR2は判定幅を0のまま残すので、プレイは比べない |
| `oracle_unplayable` | OpenLR2でも時刻が無限大・NaNになる譜面（BPM 0など） |
| `oracle_undefined` | OpenLR2がレーンの配列の外を読んだプレイ。OpenLR2の結果が実行ごとに変わり、ヒープ破壊で落ちることもある |
| `skipped_over_one_hour` | ランナーの1時間の上限を超える譜面 |
| `load_differs` / `play_differs` / `bmz_play_error` | 調べるべき差 |
