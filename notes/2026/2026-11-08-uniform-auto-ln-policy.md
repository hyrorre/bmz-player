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

## ローカル互換

`score_db/ln_compatibility.rs`に接続内だけの根拠と参照ビューを置きます。物理行は維持し、
新規保存はcanonical側だけです。解析版が現行で、同一SHAの読めるコピーが検証でき、優先解析が
矛盾せず、RANDOMが無い場合だけ3組の旧キーを参照します。必要な旧記録のSHAだけを検証します。
通常の`data/profiles/<id>/score.db`は対応する`data/library.db`を読み取り専用で参照します。
外部スコア・replay取り込みとスキャン後、過去IR履歴の取り込み完了時には、そのlibrary接続で根拠を更新します。

BESTの独立極値と、実際の代表履歴の辞書順比較を分離します。source minbp、clear-only、
履歴欠損の既存情報を維持します。更新比較の旧snapshotも合成BESTを使います。
回数は物理行ごとの保存回数、履歴は元IDを一度だけ参照します。新旧slotの衝突はcanonical優先、
空slotだけ旧キーへfallbackし、返す行は保存policyを保持して既存replay検証を使います。

コースは全stageの根拠、保存SHA列の一致、旧AUTOとconstraintの整合も要求します。
成績・ランプ・履歴・回数・最終日時・トロフィーは同じattempt集合を参照し、既存courseの
代表attempt選択を維持します。IDからの履歴読み出しは保存policyのままです。

詳細と制約は[保存仕様](../../docs/score-persistence.md#単一実効ln型の旧キー互換参照)に記載します。
古い解析、未所持、読めない／変わったファイル、RANDOMなどは推測統合しません。

## IRの境界

新規単曲・コースは正規化後のpolicyを使います。元譜面flagsは維持します。
現行サーバーの単曲validationと実際のcourse submitに新旧policy受理テストを追加します。
過去投稿は旧区分に残し、過去ランキングの統合は行いません。旧outbox再送とbackfillは保存時の
policy・本文・idempotency/submission keyを保持し、送信済みsnapshot／remote IDも変更しません。
replayの内容・hash・pathも維持します。Bokutachi、本番DB変更、デプロイは対象外です。

## 検証

- Rust: fmt、bmz-playerのcheck / all-targets Clippy / test。
- 固定96組、変更3組、再正規化、pair型・採点対象数・最大EX、constraint順序とコース復元。
- 旧／新／両方のBEST、独立極値と代表履歴、clear-only／履歴欠損、slot衝突、replayバイト列とhash。
- source minbp補完の再取り込み、古い解析／RANDOM／欠損ファイルの拒否、コース根拠とslot。
- 新規IR payloadのsource flagsとeffective mode、旧outbox再送時の本文・key保持。
- Web: 対象Prettier、vue-tsc、IR全テスト。

実機の音声・GPU・入力プレイ、本番IR送信・デプロイは行いません。
Linuxの作業用環境にRustとビルドヘッダーを導入し、固定スキンsubmoduleを初期化して検証します。

初回の英語既定localeでは、変更外の既存4テストが日本語の説明文／設定値を期待して失敗しました。
これらのコードは変更せず、全体検証は`LANG=ja_JP.UTF-8 LC_ALL=ja_JP.UTF-8 LANGUAGE=ja`で
前提を揃えます。素材未初期化による既存テストの失敗は、固定submodule取得後に再検証します。
空DBを開いた後でテーブルを作るexport fixtureと、列を変更するmigration fixtureも維持するため、
互換ビューの列展開はSQLiteへ遅延させます。列リストを接続開始時に固定しません。
必要なテーブルが揃うまではビューを作らず、migration後の初回参照でも初期化できます。

最終結果:

- `cargo fmt --check`: 成功。
- `cargo check -p bmz-player --locked`: 成功。
- `cargo clippy -p bmz-player --all-targets --locked -- -D warnings`: 成功。
- 日本語localeで`cargo test -p bmz-player --locked --no-fail-fast -- --test-threads=4`:
  2,994成功、失敗0、42 ignore。上記の初期化・migration経路の失敗も修正後に通過しました。
- 全体通過後のIR取り込み完了時の根拠更新フックは、check / Clippyと
  `cargo test -p bmz-player --locked --lib ir:: -- --test-threads=4`（200成功）で再検証しました。
- 対象3ファイルのPrettier、`bunx vue-tsc --noEmit`、`bun run test:ir`（218成功、失敗0）: 成功。

ignoreされたGPU・実機・benchmark等は実行していません。音声・入力の手動プレイ、本番IR送信も
未実施です。固定3スキン以外の任意の外部素材を使う早期return型のテストは互換確認済みとは扱いません。
