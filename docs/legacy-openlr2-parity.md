# LEGACYとOpenLR2の一致の検証

BMZ PlayerのLEGACYモードが、OpenLR2と同じ譜面の読み込み・判定・ゲージ・CustomIRの送信値になることを示す検証の記録です。
StellaverseなどOpenLR2のCustomIRを受け付けるIRの運営者に、BMZのスコアを同じ基準で扱えるかを判断してもらうために書いています。
LEGACYの仕様は[rule.md](rule.md#legacy)、比較ツールの使い方は[tools/legacy-compare/README.md](../tools/legacy-compare/README.md)、経緯と途中の計測は[作業記録](../notes/2026/2026-11-23-legacy-comparison.md)を参照してください。英語版は[legacy-openlr2-parity.en.md](legacy-openlr2-parity.en.md)です。

## 結論

OpenLR2の本体のライブラリをそのまま動かした結果とBMZを、許容誤差0で比べました。対象範囲の中で、BMZに由来する差は残っていません。

| 比較 | 件数 | 一致 | 差 |
|---|---:|---:|---:|
| 新規の実プレイ（OpenLR2計測版の判定呼び出し） | 1プレイ・2,028判定 | 2,028判定・EX 3,760 | 0 |
| 保存リプレイ（判定・地雷・ゲージ・送信値） | 234プレイ | 233 | 0 |
| 譜面の読み込み（song.dbの全譜面） | 98,051譜面 | 98,036 | 11（キーモードのみ） |
| 合成した譜面とプレイ（判定・地雷・ゲージ・送信値） | 6,500譜面 | 6,095 | 0 |

- 保存リプレイの残る1件は、BMZにないP-ATTACKゲージのプレイです。
- 読み込みの残り4件は、OpenLR2自身が読み込めない譜面です。キーモードの11件は[既知の差](#既知の差と未定義動作)で説明します。
- 合成した譜面の残り405件は、OpenLR2自身が再生できない譜面、1時間を超える譜面、ノーツの無い譜面、OpenLR2の未定義動作です。

## 検証の方法

### OpenLR2のオラクル

比較の基準には、OpenLR2 v261101のビルド済みライブラリ（`OpenLR2Lib.lib`）をそのままリンクしたコンソールプログラムを使います（`tools/legacy-compare/openlr2_oracle.cpp`）。
OpenLR2のソースは変更していません。オラクルは画面・音声なしで、次の関数を呼びます。

- `ParseBmsFile`：譜面の読み込み、配置、判定幅、ゲージ表
- `ProcNoteOnTiming`：1ミリ秒ごとに全レーンを、スクラッチ・鍵盤の順に判定
- `LogGraphPlayData` / `LogGraphPlayerDataToEnd` / `CheckClear`：グラフとクリア種別
- `PLAYSCORE::EncodeGhostData`：ゴースト

判定・地雷は`ApplyJudgeNote` / `ApplyJudgeMine`のリプレイ記録から取ります。そのためOpenLR2自身が記録する順序と同じです。
オラクルにリプレイの入力を与えると、OpenLR2が保存した判定984件と、順序まで一致しました。

### BMZ側

BMZ側は、本体と同じ譜面の読み込み、LEGACYの判定、ゲージ、スコアの処理を使うランナーです（`crates/bmz-gameplay/examples/legacy_compare.rs`）。
本体のLEGACYモードと同じく、1ミリ秒ごとにレーン順で判定し、判定と地雷をレーンごとにスコアとゲージへ反映します。
送信値は`bmz_gameplay::lr2_score::Lr2ScoreRecorder`が判定イベントから組み立てます。

### 比べたもの

| 比較 | 内容 |
|---|---|
| 新規の実プレイ | OpenLR2の計測版で、実際に`ProcNoteOnTiming`へ渡された時刻と入力を記録し、同じ呼び出しをBMZに与えて判定を1件ずつ比較 |
| 保存リプレイ | 保存済みのリプレイ234件の入力を、オラクルとBMZの両方に与えて比較 |
| 譜面の読み込み | song.dbのSP/DP全譜面について、ノーツ数・判定幅・ゲージ表・全ノート・LN終点・地雷の時刻（µs単位）を比較 |
| 合成した譜面とプレイ | 乱数で作った譜面とキー入力を両方に与えて比較。4つのseedで6,500譜面 |

保存リプレイと合成プレイでは、次の項目を許容誤差0で比べます。

- 各判定のポーリング時刻・レーン・判定値
- 地雷のポーリング時刻・レーン・ダメージ
- ポーリングごとのゲージ（倍精度）・EX・コンボ・最大コンボ・判定数・処理済みノーツ数
- 閉店の時刻
- 下のCustomIRの送信値すべて

保存リプレイをOpenLR2の保存判定と直接比べると、234件中158件しか一致しません。
OpenLR2は入力を記録用の時計（41）で保存し、判定には別の時計（142）を使うので、保存リプレイから実際の判定時刻を復元できないためです。
不一致の76件はオラクルでもまったく同じように不一致になり、BMZとオラクルは234件とも一致しました。
このため、BMZの正しさはオラクルとの比較と、計測版による実プレイの比較で示しています。

## CustomIRの送信値

OpenLR2は、リザルトでCustomIRモジュールに`IRScoreV1`を渡します（`LR2_customir.cpp`）。プレイから決まる項目と、比較の状況は次のとおりです。

| 項目 | OpenLR2での出どころ | 比較 |
|---|---|---|
| 早/遅のPGREAT〜POOR、コンボ切れ、FAST / SLOW、処理済みノーツ数（全体と20レーン） | `ProcSinglenote` / `ProcLongnote`の分岐ごとの集計 | 一致 |
| レーンごとのノーツ数（地雷を含む） | `bmsobj_note[lane].count` | 一致 |
| 最大コンボ、スコア、EX、レート | `ApplyJudgeNote` | 一致 |
| ゲージ6種の最終値 | `ApplyJudgeNote` / `ApplyJudgeMine`、プレイ終了時の0化 | 一致 |
| クリア種別 | `CheckClear` | 一致 |
| ノーツ数、`song_runtime` | `ParseBmsFile` | 一致 |
| グラフ（ゲージ6種・コンボ・EX、各1000点） | `LogGraphPlayData` / `LogGraphPlayerDataToEnd` | 一致 |
| ゴースト | `PLAYSCORE::EncodeGhostData` | 一致 |
| レートのグラフ | 自己ベスト・目標スコアの値 | 対象外（プレイの結果ではない） |
| 曲情報・設定・入力機器 | 選曲データ・設定 | 対象外（プレイの結果ではない） |

LR2の集計はBMZの通常のスコアと数え方が違います。BMZは送信値を、LR2と同じ規則で別に組み立てます。

- PGREATはFAST / SLOWに数えません。見逃しのPOORは遅いPOORでコンボ切れ、空POORは早いPOORでノーツ数に入りません。
- LNは始点を押した時点でGREAT〜BADのFAST / SLOWを数え、終点で始点の早遅のまま判定を数えます。
- ゲージは選んだゲージに関係なく6種すべてが動きます。未処理のノーツが残ったまま終わると、6種とも0になります。

BMZ本体からStellaverseへの送信は、まだ実装していません。
実装するときは、LEGACYのプレイで`Lr2ScoreRecorder`に全判定イベントと地雷を渡し、各ポーリング後に`log_graph`、終了時に`finish`を呼びます。これで、ここで比べた値がそのまま得られます。

## 対象範囲

| 区分 | 対象 |
|---|---|
| 鍵数 | 5鍵・7鍵・10鍵・14鍵（SP/DP） |
| 配置 | 正規、MIRROR、RANDOM（固定キーを含む） |
| ゲージ | GROOVE、HARD、DEATH、EASY |
| 譜面 | BMS / BME / BML。LNチャンネル、`#LNOBJ`、地雷、不可視ノート、`#RANDOM`、拡張BPM、STOP、小節長 |
| プレイ | 単曲 |

次は検証の対象外です。

- S-RANDOMなど上記以外の配置、皿の配置変更、アシスト、バトル、DPフリップ、再生速度の変更
- P-ATTACK / G-ATTACKゲージでのプレイ（BMZにゲージがありません。送信値に入る両ゲージの値は、ほかのゲージでのプレイで一致しています）
- コース、GAS、マニアックオプション
- PMS（9鍵）、BMSON、`#LNMODE`のCN / HCN

## 既知の差と未定義動作

### キーモード

song.dbの`mode`とBMZのキーモードが違う譜面が、98,051譜面中11件あります。BMZは5鍵、song.dbは7鍵です。

- 10件はノーツのチャンネルを1つも含まず、1件はスクラッチ（16）だけを含みます。
- OpenLR2の`ParseBMSMETA`（`LR2_songmanage.cpp`）の規則で数えると5鍵になり、BMZと一致します。
- このsong.dbは、OpenLR2より前に登録された値を引き継いでいる可能性があります。原因は確かめていません。
- ノーツの無い10件では判定が発生しません。スクラッチだけの1件では、OpenLR2はsong.dbの7鍵でプレイするため、レーン配置が変わる可能性があります。

### OpenLR2の未定義動作

OpenLR2はレーンのノートを100個単位の配列に置き、空きを時刻-1msのノートとして判定することがあります（地雷だけのレーン、最後のノートのBADから続く判定）。BMZはこの判定も再現しています。

空きを使い切ると、OpenLR2は配列の外を読みます。結果は実行ごとに変わり、出力が壊れたり、ヒープ破壊で落ちたりします。
BMZは同じ場面を検出して印を付けるので、比較ではそのプレイを未定義動作として対象から外しています。合成プレイ6,500件中14件が該当し、実際の譜面で起きるのはまれです。

### 比較できない譜面

- OpenLR2自身が読み込めない譜面（`ParseBmsFile`がアクセス違反で落ちる2譜面）
- BPMが0になり、OpenLR2の時刻が無限大・NaNになる譜面
- ノーツの無い譜面。OpenLR2は判定幅とゲージ表を設定しないので、プレイは比べず、オブジェクトの時刻だけを比べます

## 再現の手順

OpenLR2のソース（`D:\Tools\openlr2-research`）とビルド済みライブラリ、OpenLR2のsong.dbと保存リプレイが必要です。
コマンドはリポジトリのルートで実行します。各コマンドの詳細は[tools/legacy-compare/README.md](../tools/legacy-compare/README.md)にあります。

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools\legacy-compare\build_openlr2_oracle.ps1
```

```powershell
cargo build -p bmz-gameplay --example legacy_compare --locked --offline
```

```powershell
python tools/legacy-compare/oracle_vs_bmz.py --results .local/legacy-parity-mines --oracle .local/openlr2-oracle/openlr2_oracle.exe --runner target\debug\examples\legacy_compare.exe --output .local/oracle-vs-bmz-submission --tolerance 0
```

```powershell
python tools/legacy-compare/loader_vs_oracle.py --oracle .local/openlr2-oracle/openlr2_oracle.exe --runner target\debug\examples\legacy_compare.exe --output .local/loader-all --jobs 10
```

```powershell
python tools/legacy-compare/fuzz.py --oracle .local/openlr2-oracle/openlr2_oracle.exe --runner target\debug\examples\legacy_compare.exe --output .local/fuzz-1 --count 500 --seed 1 --jobs 10
```

`--results`には、`compare.py`で保存リプレイを列挙した結果のディレクトリを渡します。合成プレイはseed 1〜4、件数500・1,000・2,000・3,000で上の表の結果になります。
