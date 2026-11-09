# Bokutachiの新規単曲送信

## 起点とAPI契約

- 作業起点は取得時点の最新`main`、`a5044120042c449d9ff7a48199cb2401bd2f6bf8`。PR #84の単一実効LN型の正規化を含みます。
- Tachiの参照commitは`aebb817466213245329ab8bdcab82bebea3f51fc`。作業中の`git ls-remote`でもHEADが同一でした。
- [Direct Manual受付](https://github.com/zkldi/Tachi/blob/aebb817466213245329ab8bdcab82bebea3f51fc/typescript/server/src/server/router/ir/direct-manual/router.ts)、[status](https://github.com/zkldi/Tachi/blob/aebb817466213245329ab8bdcab82bebea3f51fc/typescript/server/src/server/router/api/v1/status/router.ts)、[schema](https://github.com/zkldi/Tachi/blob/aebb817466213245329ab8bdcab82bebea3f51fc/typescript/common/src/lib/schemas.ts)、BMS/PMSのgame定義、譜面resolve、hash検索、import pollの実装を確認しました。
- 現行のgame ID形式（`bms-7k`、`bms-14k`、`pms-controller`、`pms-keyboard`）を採用し、旧`game + playtype`形式は混ぜません。`meta.service = "BMZ Player"`、保存済みclient versionを使用します。
- PMSはhash検索から対象game・primary・CHARTを選び、song IDで再解決します。事前照合とインポートの両方に`tachiSongID + CHART`を使用します。
- 公開API `https://boku.tachi.ac/api/v1/status` の読み取りは実行環境のproxyに拒否されました。稼働APIとの差は実通信で確認できていません。実アカウントへのスコア投稿は行っていません。

## 実装判断

利用者向けの対応範囲と操作は[仕様書](../../docs/bokutachi.md)を参照してください。

新しいBokutachiジョブだけにversion付きの採点contextを保存します。元キーモードをプレイ開始時から引き継ぎ、結果保存時のForceLn・機器区分・本人の採点範囲を確定します。表示用G-BATTLEは既存2P表示マスクと採点ノーツ数を照合し、本人7Kとして扱います。他IRのpayload・リプレイは変更しません。

network DB v7は、固定済みプレイpayloadから独立したimport進捗とoriginごとの送信待機時刻を追加します。受付receiptはAPIキーを含まず、DB保存失敗時の復旧用です。202とongoingは成功に数えず、実際のscore IDがある完了結果だけ成功台帳へ記録します。新規追加なしは別の終端状態、譜面未登録・照合不一致は手動再照会できる対象外状態です。

score DB v35は、履歴がBEST更新対象だったことを記録する列と、譜面・score key・機器区分・履歴IDの部分索引を追加します。既存履歴の成績やLN区分は変更しません。PMSのUpdateScoreは今回より前の対象履歴を使い、5指標を独立に集計します。通常BESTに反映されるコースステージの履歴も既存の比較条件に従いますが、ステージ自体の送信ジョブは作りません。HistoryOnlyは比較元にしません。

設定正規化は既存3枠を維持して4枠目を追加し、カスタム設定は後続へ保持します。Bokutachiはランキング用primaryに選びません。送信キューはprimaryから独立してproviderを選べるようにし、登録確認待ち・対象外・失敗・登録成功・新規追加なしを表示します。

## 検証環境

Linux x86_64、Rust 1.99.1。RustとFFmpeg/ALSA等の開発依存をworkspace内に準備し、`/tmp/bmz-cargo`でCargo/Rustup・pkg-config sysroot・libclangのパスを指定しました。設定・依存展開先・実行ログはリポジトリへコミットしません。

最初の全テストでは、日本語固定の既存テストと環境の英語ロケールの違い、未初期化のmz-select/Luxez-Flat、および旧DBを模擬する3テストのschema前提で失敗しました。日本語ロケールと固定commitのsubmoduleを用意し、旧DB fixtureからv35の列・索引を除くよう修正しました。

## 検証結果

| コマンド | 結果 |
| --- | --- |
| `cargo fmt --check` | 成功 |
| `cargo check --workspace --locked` | 成功 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 成功 |
| `cargo build -p bmz-player --locked` | 成功（Linux、既定feature） |
| `LANG=ja_JP.UTF-8 LANGUAGE=ja LC_ALL=ja_JP.UTF-8 cargo test --workspace --locked --no-fail-fast` | 成功。bmz-playerは3,030成功・42 ignored。workspace全体で失敗0 |
| `cargo test -p bmz-player --locked bokutachi` | 19成功、失敗0 |
| `target/debug/bmz-player --help` | 成功。BokutachiのAPIキー入力・権限の案内を確認 |
| `git diff --check` | 成功 |

HTTPモックと一時DBで、4送信先・保存時LN/機器/配置・表示専用G-BATTLE・空POOR・lamp変換・独立BEST集計・譜面検索と再解決・202/ongoing/4完了結果・アカウント不一致・再起動・DB保存失敗・429の秒数/HTTP-date・poll 404・手動再試行・未対応経路を確認しました。実アカウントへの投稿はありません。

旧DB fixture修正後の再実行では、追加列を既存メタデータ比較から除く必要があるテストが1件残りました。新しい列の既定値を別途検証するように修正し、上記の全workspace再実行で成功しました。

6言語の追加UIキーの一致と重複なし、6言語マニュアルのBokutachi説明、仕様書・作業記録の相対リンクをスクリプトで確認しました。生ログは実行環境の`/tmp/bmz-*-complete.log`、`/tmp/bmz-*-final-tree.log`等に保存し、コミットには含めません。

## 未実施の確認

- 本番APIへの認証・スコア投稿・実際のBokutachiインポート完了。
- 実機でのGUI操作、入力機器、7K/14K/9Kのプレイと表示用G-BATTLE。
- Windows/macOSでのビルドとOS資格情報ストア。
- GPU等を要する既存ignoredテスト。第三者スキンの一部テストは参照資産が無い場合に早期returnするため、全素材の互換確認を意味しません。
