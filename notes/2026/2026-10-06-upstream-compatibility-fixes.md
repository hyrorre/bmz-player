# beatoraja / LR2orajaED-rian の直近4か月の修正取り込み

## 対象と進め方

- 調査期間: 2026-06-06 ～ 2026-10-06。
- beatoraja: `d22ce10bc13e7ddb27805a3adc15bb03312e4c78`。期間内123コミット（merge除外107）。
- LR2orajaED-rian: `4b44283e521a3147152cf70347336681f1f0631f`。期間内162コミット（merge除外125）。
- BMZの開始点: `ea9b39693c3e81b65cf7e8b4adea19a9da36702d`、`main`。
- ユーザー承認に基づき、P1/P2の9件をサブエージェントで順番に修正した。
  全9件の実装・自動検証を完了し、各変更を親エージェントがレビューして機能単位でコミットした。
- P3の追加機能候補（追加Lua API、複合モード等）は今回の修正対象外。

## 修正順序

| 順 | 優先度 | 項目 | 上流コミット |
|---|---|---|---|
| 1 | P1 | 空白区切りBMS/PMS小節データ | rian `b332fddb` / `541dcfc1` |
| 2 | P1 | DX 9Kゲージ減少量 | rian `c9808349` |
| 3 | P2 | スキン・システム音のディレクトリ循環対策 | beatoraja `2d380751` |
| 4 | P2 | タイミンググラフの実判定窓反映 | rian `adfce250` |
| 5 | P2 | PracticeのDX初期ゲージ・上限 | rian `e88dd204` |
| 6 | P2 | BMP00の既定ミスレイヤー | rian `0d215459` / `eeb06afc` |
| 7 | P2 | 名前によるスキンプロパティ・否定条件 | beatoraja `532cc93b` / `39351940` |
| 8 | P2 | destinationのclipアニメーション | beatoraja `4d7141b1` |
| 9 | P2 | 破損Practice JSONからの復帰 | beatoraja `2ed69380` |

## 1. 空白区切りBMS/PMS小節データ

`bmz-chart` のimport adapterで、通常のコロンに加え半角空白・タブで区切られた
小節行を解析前に正規化する。PMSレイアウト検出と生ヘッダー抽出も同じ小節行判定を使う。
原文から計算するMD5/SHA-256は維持し、第三者の譜面自体は編集しない。

通常・不可視ノーツ、BGM、BGA全4層、小節長、BPM/STOP、RANDOM、PMS、
既存の4桁小節対応経路（BGA・疎な長大行）を回帰テストで確認。
一般の4桁小節を新たに全面対応する変更ではない。

- `cargo fmt --check`: 成功。
- `cargo check -p bmz-chart --locked`: 成功。
- `cargo clippy -p bmz-chart --all-targets --locked -- -D warnings`: 成功。
- `cargo test -p bmz-chart --locked`: 162 passed / 2 ignored。
- 実PMSのimport: 9K、2101 notes、終端 `162981214 us`、原文ハッシュ一致。
  警告は文字コードfallbackの2件。実プレイの音声・描画確認は未実施。
- 空白区切り小節長14行をコロン化した一時コピーとの比較で、全129小節線、
  2101ノーツのlane/kind/tick/time/damage、34 timing events、終了時刻が一致。
  importごとのsound ID割当は比較対象外。原曲は変更していない。

対応書式は [README](../../README.md) に記載。

## 2. DX 9Kゲージ減少量

`dx_pop_gauge_definition_table` のAssistEasy～ExHardについて、BAD / POOR / EMPTY POORを
上流 `c9808349` の値へ合わせた。回復量、下限2・上限120、クリア境界、Hazard・段位、
9K以外のDX、他ルールは維持する。HCN継続ダメージも既存のBAD半率から補正後の値になる。

ゲージ更新テストでTOTAL・ノーツ数への非依存、クリア境界の前後、下限、HCNを検証。
app側でもreplay入力と見逃し処理を通してBAD / EMPTY POOR / POORの減少量を検証した。
保存済みのスコアは移行しない。古いreplayを現行ルールで採点すると、記録時とゲージ推移が
異なる場合がある。保存・IRの扱いは [rule.md](../../docs/rule.md) に記載した。

- focused `dx_9key` 回帰: 8件成功。
- `cargo fmt --check` / `cargo check --workspace --locked`: 成功。
- `cargo clippy -p bmz-gameplay --all-targets --locked -- -D warnings`: 成功。
- 通常権限の `cargo test --workspace --locked --no-fail-fast -- --show-output`: 成功。
  bmz-player 2227 passed / 19 ignored、bmz-gameplay 236 passed。
  全crate合計3784 passed / 28 ignored（player子プロセス出力の重複1件を除く）。
  ignoredのGPU・実機等の検証を成功件数に含めない。
- 全体Clippy: 未変更の `crates/bmz-player/src/input/capture.rs:111` にある
  `let _ = route_changed_at;` がWindowsで `clippy::let_unit_value` となり失敗。
  この作業では修正・抑制していない。
- 初回の制限付きworkspaceテストはファイル保存・通信で失敗し、更新テストが待機した。
  replay保存の同一テストが制限付き実行では `os error 5`、通常権限では成功することを確認。
  自分が起動したテストだけを中断し、通常権限で全体を再実行した。初回ログは別途保持する。
- 検証ログ: `.local/validation/2026-10-06-dx9k-gauge/`（Git管理外）。

## 3. スキン・システム音のディレクトリ循環対策

カタログ走査ごとにcanonical pathで訪問済みディレクトリを記録し、自己参照・親参照・
同じ実体を指す複数リンクの再走査を止める。リンク経由の明示rootと元の候補パスは維持し、
canonical化できないディレクトリは走査しない。スキンとシステムBGM/SEで共通helperを使う。

Windowsでは実junction、Unixではsymlinkを使う回帰テストを追加した。
自己・親・2つの別名・明示linked rootと通常の階層を併存させ、候補欠落・重複・循環がないことを確認。
同一実体の別名が複数ある場合は、既存の列挙順で最初に見つかった経路を採用する。

- Windows junction回帰: 2 passed / 0 ignored。リンク作成を省略せず実行。
- `cargo fmt --check` / `cargo check -p bmz-player --locked`: 成功。
- 通常権限の `cargo test -p bmz-player --locked`: 2229 passed / 19 ignored。
- all-targets Clippyは既存の `input/capture.rs:111` のunit値束縛だけで失敗。
  変更箇所に新たな診断はなく、既存問題の修正・抑制はしていない。
- 検証ログ: `.local/validation/2026-10-06-scan-cycles/`。
- Unix/macOS上でのリンク作成・実機操作は未実施。

## 4. タイミンググラフの実判定窓反映

Playの `judge.window_set.note` を描画snapshotへ渡し、TimingVisualizerと
TimingDistributionGraphの判定帯に使う。Resultは終了時snapshotに窓を固定し、
非同期保存中に元sessionやprofileが変わっても最終窓を保持する。
コースResultは最終曲の窓を使う。未生成sessionの補完表示もrule/key modeに従う。

DX 7K/9K、LR2oraja、DEFEXRANK、Practice速度50/100/200%、replay視聴速度不変、
非対称BAD幅と描画矩形を検証した。動的EXRANK非適用など既存の採点契約は維持する。
保存済みスコア・replay形式は変更しない。表示単位の詳細は [rule.md](../../docs/rule.md) を参照。

- 対象 `timing_judge` 回帰: player 1件、renderer 4件成功。
- `cargo fmt --check` / workspace all-targets check: 成功。
- 通常権限のworkspace全テスト: 3789 passed / 28 ignored
  （player子プロセスの重複1件を除く）。player 2230 passed、renderer 682 passed。
- renderer all-targets Clippy: 成功。workspace Clippyは既存の
  `input/capture.rs:111` の `let_unit_value` だけで失敗。
- 実GPU上の見た目や上流との実プレイ比較は未実施。
- 検証ログ: `.local/validation/2026-10-06-timing-judge-window/`。

## 5. PracticeのDX初期ゲージ・上限

Practiceのrule/key modeから実ゲージ定義の初期値・上限を解決し、読み込み・マウス・
キー操作・開始直前で共有する。新規DX設定とゲージ切替は実定義の初期値を使い、
保存済みの20を含むcustom値は範囲内なら保持する。DX 9KのAssistEasy～ExHardは30/120、
7K系のAssistEasy～Normalは22/100、Hazard・段位系等は100/100となる。

開始時はGaugeStateの各memberの制限を利用し、GASでもPOPの120とHazardの100を保持する。
変換済みpreloaded譜面は実判定用key modeでUI範囲を同期する。通常のPractice開始に
新たな譜面変換を追加する変更ではない。非DXの新規初期値・ゲージ切替の方針は維持する。
DXのカテゴリ・判定ランク・TOTALは固定表示とし、保存値と採点式は変更しない。
詳細は [controls.md](../../docs/controls.md) と [rule.md](../../docs/rule.md) を参照。

- Practice関連39件: 全件成功 / 0 ignored。
- 新規回帰7件: fresh DX、保存20/117、9Kゲージ切替、固定欄・keyboard、egui、
  7→9 Keys7/Keys9のpreloadedと実session、GAS各memberの上限を確認。
- `cargo fmt --check` / `cargo check -p bmz-player --locked`: 成功。
- 通常権限のplayer全テスト: 2237 passed / 19 ignored。
- all-targets Clippyは既存 `input/capture.rs:111` の `let_unit_value` だけで失敗。
- 実機手動操作は未実施。ログ: `.local/validation/2026-10-06-practice-dx/`。

## 6. BMP00の既定ミスレイヤー

bms-rsは `#BMP00` を通常のBMP一覧ではなく `poor_bmp` に保持していた。
BMS/PMS専用の取り込みで、有効RANDOM分岐のこの値をBMP key 0 resourceと初期Poorイベントへ復元する。
時刻0の明示Poorは優先し、空チャンネル・途中切替・他レイヤーとの共存を維持する。
BMSONのresource ID 0には既定ミス表示の意味を追加しない。譜面原文・ハッシュは維持する。

- import回帰7件: 上流4ケース、空データ・途中切替・空白区切り、未定義の明示Poor、
  他の全BGA層、有効/無効RANDOM分岐、PMS両配置、BMSONを確認。
- player回帰: asset ID 0のミス表示、後続画像への切替、判定時刻での選択、有効期間を確認。
  新規回帰と既存BGA回帰の各1件が成功。
- fmt、bmz-chart check / all-targets Clippy / 全テスト: 成功。169 passed / 2 ignored。
- 環境由来のincremental cache hard-link警告あり。実機での画像表示は未確認。

## 7. 名前によるスキンプロパティ・否定条件

参照beatorajaのfactoryから固定512名・完全一致パターン95名・番号範囲15件を生成し、
種類別の名前解決を `bmz-skin-document` へ集約した。公開名の大小文字・綴りを保持する。
Integer / Index / Rate / Float / String / Booleanを区別し、名前対応と未実装IDの意味対応は分ける。
生成スクリプトは参照commitとJavaファイルの一致、未解決定数・欠落・重複を検証する。
通常ビルドはチェックイン済みの表を使い、参照checkoutやJavaへ依存しない。

型付き `value`、destinationの名前付き `op` / `draw`、Luaの `main_state` 4種の引数に適用。
Booleanの否定、混在opの各個AND、既存の式・数値引数、load時依存捕捉と永続VMを維持する。
`imageset.value` はIntegerとして扱い、負値は非表示、上限外は先頭画像とする。
`ref` は数値のまま。新しいfloatvalue schemaやLua APIは追加しない。
対応範囲は [skin.md](../../docs/skin.md#名前によるプロパティ参照) を参照。

- 追加回帰15件: resolver 6、schema 3、renderer 4、Lua 2。
  全名・番号境界・種類衝突・未知名・否定、Auto/Compat、Lua→JSON→load、状態保持を確認。
- 生成表 `--check`、fmt、workspace check、変更3crateのall-targets Clippy: 成功。
- 通常権限のworkspace全テスト: 3819 passed / 28 ignored
  （player子プロセス出力の重複1件を除く）。
- workspace Clippyは既存 `input/capture.rs:111` の `let_unit_value` だけで失敗。
  条件配列のサイズ増加による新規lintは固定長の格納形式へ変更して解消した。
- 実機UI比較は未実施。ログ: `.local/validation/2026-10-06-property-names/`。

## 8. destinationのclipアニメーション

JSON / Luaの `clip_x` / `clip_y` / `clip_w` / `clip_h` を各キーフレームで継承し、
既存のtimer・loop・acc、個別offsetと全体offsetを通してGPU scissorへ反映する。
4値が揃うまでは無効、幅・高さが0以下なら解除し、正の矩形が画面外なら描画を抑止する。
画像・回転画像・文字・数字・グラフ・ゲージ、notes・songlist・judge全体を対象にする。
上流が直接描く子部品には独自のclipを追加せず、検索入力overlayも上流の扱いを維持する。

Push/Popで切り抜き範囲を復元し、空の交差でも画像・文字・Ambientの内部indexを進める。
AmbientやRectBatchは合成後にclipし、blurの入力は切り詰めない。
clipのみのアニメーションでは完成済みグラフの形状・GPU texture cacheを再利用する。
notesのLua条件にも実Playの文字情報を渡し、clipのためのcallback重複評価を避ける。
対応契約は [skin.md](../../docs/skin.md#destinationのクリッピング) を参照。

pixel変換はfloatで投影してからJavaの丸め規則を使う。正負の半pixelと奇数offsetを確認するが、
正規化・投影のfloat精度により丸め境界で上流との1pixel差が残る可能性があり、
全座標でのbit単位の一致は保証しない。epsilonによる補正は入れていない。
既存の外部スキンにはclip使用例が見つからず、合成fixtureで確認する。

- focused clip: renderer 23件、skin 1件、schema 1件成功。新規通常回帰は計20件。
  ノーツ・LN各部・Mine・小節線・BPM/STOP/time guideの9種を別textureで検証した。
- 新規GPU 2件と既存GPU 4件: 全件成功。新規テストはRTX 5090 / DX12、
  driver `32.0.16.1714` でpixelを比較し、文字のclipなしbaselineも確認した。
- fmt、workspace all-targets check、変更3crate all-targets Clippy: 成功。
- 通常権限のworkspace全テスト: 3839 passed / 30 ignored
  （player子プロセス出力の重複1件を除く）。rendererのignored 6件は上記で別途実行済み。
- workspace Clippyは既存 `input/capture.rs:111` の `let_unit_value` だけで失敗。
- 第三者スキンの実機操作・上流との同画面手動比較は未実施。
  ログ: `.local/validation/2026-10-06-destination-clip/`。

## 9. 破損Practice JSONからの復帰

Practice設定をbytesとして読み、JSONのparseが成功したときだけ保存済み設定として扱う。
null・型不一致・途中切れ・不正UTF-8・読み取りエラーはpathと理由を警告し、新規設定へ戻す。
譜面の区間・判定ランク・TOTAL、DXの実ゲージ初期値、CLI overrideを既存の経路で適用する。
読み込み時には元ファイルもbackupも書き込まない。

保存直前に既存ファイルを再読込し、破損していれば同じフォルダへ
`<SHA-256>.json.corrupt-<UUID>.bak` を排他的に作成し、元のbytesを退避・同期する。
その後、新JSONを同じフォルダの一意な一時ファイルへ書き込み・同期してからrenameする。
既存ファイルの読み取り・退避・書き込み・renameに失敗した場合は元ファイルを維持する。
自分で作成した失敗時の一時ファイルだけを削除し、元ファイル削除によるfallbackは行わない。
保存エラーは退出経路を含め原因のchainをログへ残す。
保存場所と復旧契約は [controls.md](../../docs/controls.md) を参照。

- 新規8件: persistence 5件、初期値・CLI・保存値を確認する入口の回帰3件。
  不正入力5種類、DX 7K/9Kと他rule、保存の失敗境界を含む。
- Practice関連47件: 全件成功 / 0 ignored。
- fmt、player all-targets check: 成功。
- 通常権限のplayer全テスト: 2246 passed / 19 ignored、終了コード0。
  Windowsの実ファイルで既存JSONのrename置換とbackupの完全一致を確認した。
- all-targets Clippyは既存 `input/capture.rs:111` の `let_unit_value` だけで失敗。
- 実アプリの手動操作は未実施。
  ログ: `.local/validation/2026-10-06-practice-json-recovery/`。

## 検証楽曲

[作者の配布ページ](https://www.luzeria.net/?p=387)から「運命論」を取得した。

- 配布ファイル: <https://www.luzeria.net/music/71_unmei_ogg.rar>
- SHA-256: `7B73B588652BAF641E59183AFB4B41CFACB27C6F6C0CA60178A7A8CC80AFE802`
- 配置先: `G:/BMS/OTHERS/71_unmei_ogg`
- 追加PMS: `0x_fumei_x.pms`（初等運命論講義 / ★ Fortune）。
  これは公式アーカイブの同梱譜面ではなく、既存ローカルコレクションから無変更でコピーした。
  当該差分の現在の配布URLは特定できなかった。
- PMS SHA-256: `099ABA2400E85EA28B7DB9DB8DC21F616ACE69DE1683F5B359BE62BC0F80E151`
- 例: `#07502 0.505882352941176` のような空白区切り小節長変更を含む。
- 宣言された1173個のWAV/BMP参照は、WAVからOGGへの拡張子fallbackを含めて取得した本体で解決できる。
- 配置先の `README_BMZ_VALIDATION.txt` に入手元と検証用途を記録。
  楽曲・音源・画像はGit管理対象に追加しない。

取得アーカイブと一時検証ツールは `.local/downloads/upstream-20261006/`、
`.local/notes/inspect_space_bms.rs` / `.exe` に保存したローカル素材であり、リポジトリには含まれない。
