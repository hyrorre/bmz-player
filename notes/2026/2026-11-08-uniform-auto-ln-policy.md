# AUTOの単一実効LN型をForce保存区分へ正規化

基点: main `015942f402d551ca3b2a0cc62591a80c175f1881`。作業開始2026-11-08、検証2026-11-09。

## 仕様と実装

これは[LN仕様](../../docs/ln.md)の正規化仕様の拡張です。undefined + defined LNのみの
AUTO(LN)、undefined + defined CNのみのAUTO(CN)、undefined + defined HCNのみの
AUTO(HCN)の3ケースを対応するForceへまとめます。実効pair型の集合を調べ、最高位の型だけを
返す`played_ln_mode`は単一型判定に使いません。96組の固定表で他の設定を維持します。

各ノーツの型を維持するため判定・ゲージのコードは変更しません。元profile・元件数・希望設定も
上書きしません。単曲、preload、選曲、外部取り込み、新規IRは共通の`score_ln_policy`を使います。
コースは元profile集約→constraint fallback→正規化の既存の順序を維持します。

## 既存ローカル記録

当初は`score_db/ln_compatibility.rs`で旧`Auto*`キーを接続内の一時ビューから互換参照する実装を
入れていましたが、レビューで外しました（2026-11-09）。理由は次のとおりです。

- 対象はundefined + defined単一種別の混合譜面だけで、該当譜面はほとんど無い。
- `score_best_compatible`がwindow関数を含むため、`(chart_sha256, ln_policy, …) IN (…)`の
  行値条件ではSQLiteが絞り込みをビュー内へ持ち込めず、全譜面のBEST取得が`score_best`全行の
  集約になった。SQLite 3.45、約3.3万行で200キーあたり約1ms→約370ms（1キーでも約365ms）。
- 選曲フォルダ集計workerなど別接続の根拠が再スキャン後に更新されず、`ScoreDatabase::open`ごとに
  譜面ファイルのハッシュ確認が走るなど、維持コストも大きかった。

そのため旧記録は旧区分のまま残し、移行も互換参照もしません。新規プレイは`Force*`側に保存します。

## IRの境界

新規単曲・コースは正規化後のpolicyを使います。元譜面flagsは維持します。
現行サーバーの単曲validationと実際のcourse submitに新旧policy受理テストを追加します。
過去投稿は旧区分に残し、過去ランキングの統合は行いません。旧outbox再送とbackfillは保存時の
policy・本文・idempotency/submission keyを保持し、送信済みsnapshot／remote IDも変更しません。
replayの内容・hash・pathも維持します。Bokutachi、本番DB変更、デプロイは対象外です。

## 検証

- Rust: fmt、bmz-playerのcheck / all-targets Clippy / test。
- 固定96組、変更3組、再正規化、pair型・採点対象数・最大EX、constraint順序とコース復元。
- 新規IR payloadのsource flagsとeffective mode、旧outbox再送時の本文・key保持。
- Web: 対象Prettier、vue-tsc、IR全テスト。

実機の音声・GPU・入力プレイ、本番IR送信・デプロイは行いません。
Linuxの作業用環境にRustとビルドヘッダーを導入し、固定スキンsubmoduleを初期化して検証します。

初回の英語既定localeでは、変更外の既存4テストが日本語の説明文／設定値を期待して失敗しました。
これらのコードは変更せず、全体検証は`LANG=ja_JP.UTF-8 LC_ALL=ja_JP.UTF-8 LANGUAGE=ja`で
前提を揃えます。素材未初期化による既存テストの失敗は、固定submodule取得後に再検証します。

最終結果（互換レイヤー削除後に再検証、2026-11-09）:

- `cargo fmt --check`: 成功。
- `cargo clippy -p bmz-player --all-targets --locked -- -D warnings`: 成功。
- 日本語localeで`cargo test -p bmz-player --locked --no-fail-fast -- --test-threads=4`:
  2,987成功、失敗1、42 ignore。失敗は`scan_song_roots_skips_unreadable_subdirectory_and_keeps_accessible_files`
  だけで、root権限で実行したため0o000のディレクトリも読めてしまう環境依存の失敗です（今回の変更とは無関係）。
- 対象3ファイルのPrettier、`bunx vue-tsc --noEmit`、`bun run test:ir`（218成功、失敗0）: 成功（Web側は削除前から変更なし）。

ignoreされたGPU・実機・benchmark等は実行していません。音声・入力の手動プレイ、本番IR送信も
未実施です。固定3スキン以外の任意の外部素材を使う早期return型のテストは互換確認済みとは扱いません。
