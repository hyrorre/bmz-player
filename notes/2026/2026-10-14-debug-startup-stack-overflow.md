# Windows debug起動のスタックオーバーフロー

実施日: 2026-10-14（JST）。開発手順は [development.md](../../docs/development.md) を参照。

## 症状と切り分け

- Windows x86_64のdebugビルドが、ウィンドウ表示前にmain threadの
  `STATUS_STACK_OVERFLOW`（`0xc00000fd`）で終了する。ユーザー環境ではreleaseは起動する。
- bootstrapの設定読込・DB migration・曲scanは完了する。
  SelectはWMIX_HD、Decide / ResultはEndlessCirculationを選択していた。
- main threadとbackground workerのログを区別して追跡した結果、WMIXのCSV処理、
  JSON生成、整数正規化までは完了し、`SkinDocument`のデシリアライズ中に停止していた。
  直前に出ていたResultのフォントwarningは別workerの処理である。
- `SkinDestinationDef`の`Self::deserialize`はSerdeのremote deriveが生成する
  inherent helperを呼び出す。trait実装の直接的な無限再帰ではない。

## 原因と修正

`Box::new(WinitApp::new(...)?)`は、debugでは大きな`WinitApp`を値として返すための
一時領域を呼出側にも確保する。この起動frameとconstructorのframeを残したまま、
Selectの同期decodeへ入る。デシリアライズなどの下位frameが重なることで、
Windowsのmain threadに予約された1 MiBのスタックを使い切っていた。

constructorを`Result<Box<Self>>`に変更し、構築した`Box`を直接返すようにした。
呼出側の`Box::new`を除き、全体の一時値を減らす。スキン読込の順序・スレッド責務は維持する。

rustc 1.98.1、Windows x86_64、dev profile（unoptimized + debuginfo）で
`cargo rustc -p bmz-player --lib --locked -- --emit=asm`の`.seh_stackalloc`を比較した。
以下は関数ごとの固定確保量で、呼出先を含む最大スタック使用量の実測ではない。
詳細ログのレベル整理前に計測したため、最終コードの生成量とはわずかに異なる可能性がある。

| 関数 | 修正前（bytes） | 修正後（bytes） |
|---|---:|---:|
| `run_with_options_log_buffer_paths_and_profile`のasync poll | 310,080 | 48,304 |
| `WinitApp::new` | 324,008 | 258,744 |
| 合計 | 634,088 | 307,048 |

合計327,040 bytes（約319 KiB）減った。修正後の実行ファイルのPE headerは
`SizeOfStackReserve=1048576`、`SizeOfStackCommit=4096`であり、予約量は変更していない。

## ログと回帰テスト

- 通常のinfoログに初期スキン・default manifest・同期decode・installの開始を追加。
  既存の完了ログと組み合わせ、停止した段階を確認できるようにした。
- LR2変換・document/font/source処理の詳細はdebug、コマンドと個別アセットはtrace。
  有効化手順は [development.md](../../docs/development.md#windows-debug起動のスタック回帰と調査ログ) を参照。
- `crates/bmz-player/tests/startup_stack.rs`は実際のdebug実行ファイルを別プロセスで起動し、
  Select install、初回present、3フレーム後の正常終了を確認する。
  通常のtest harnessはmain threadより大きなスタックを使うため、子プロセスで再現する。
- 合成LR2スキンは外部素材なしで検証できる。WMIXのテストは素材がなければskipする。
  どちらもnative window / GPU / 音声デバイスが必要なため通常はignoreする。
  一時的な設定・DB・cache・ログを利用し、60秒を超えた子プロセスは停止する。

## 検証

- 修正前のconstructorを一時的に戻した比較: 合成LR2と実際のWMIXの両方が
  `0xc00000fd`で終了し、回帰テストが2件とも失敗した。
- 修正後の同じテスト: 2件成功。WMIXの素材は存在し、skipされていない。
- WMIX SelectとEndlessCirculation Decide / Resultを使う独立したdebug smoke:
  3フレーム描画後に正常終了した。既存のResultフォントwarningは残る。
- `cargo fmt --check`、`cargo check --workspace --locked`: 成功。
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: 成功。
- `cargo test --workspace --locked --no-fail-fast`: 通常のWindows権限で成功。
  `bmz-player`のunit testは2,413成功・22 ignore。起動回帰2件は別途明示実行して成功。

ローカル生ログは`.local/stack-overflow-regression-before.log`、
`.local/stack-overflow-regression-after.log`、`.local/stack-overflow-fix-smoke-console.log`等に
保存した。Git管理しない。sandboxの既定tempではatomic renameがアクセス拒否となったため、
検証時の`TEMP` / `TMP`をworkspace内の`.local/test-temp`へ向けた。
workspace testの最初のsandbox実行では、localhost接続が`os error 10013`で拒否され、
リンク処理等も失敗した。HTTP fixtureの待機が続いたため、そのtest harnessだけを
PIDと実行パスを確認して停止し、通常のWindows権限でworkspace testを再実行した。
最終ログは`.local/stack-overflow-workspace-test-unrestricted.log`に保存した。

releaseの再起動、macOS / Linux、長時間プレイ・全スキンの画面互換確認は今回未実施。
