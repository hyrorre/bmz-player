# beatoraja / LR2orajaED-rian の直近4か月の修正取り込み

## 対象と進め方

- 調査期間: 2026-06-06 ～ 2026-10-06。
- beatoraja: `d22ce10bc13e7ddb27805a3adc15bb03312e4c78`。期間内123コミット（merge除外107）。
- LR2orajaED-rian: `4b44283e521a3147152cf70347336681f1f0631f`。期間内162コミット（merge除外125）。
- BMZの開始点: `ea9b39693c3e81b65cf7e8b4adea19a9da36702d`、`main`。
- ユーザー承認に基づき、P1/P2の9件をサブエージェントで順番に修正する。
  各変更を親エージェントがレビューし、検証後に機能単位でコミットする。
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
