# DOUBLE BATTLEの入力・リプレイ・IR送信（#119）

[Issue #119](https://github.com/hyrorre/bmz-player/issues/119)の報告と、DB / DBASの開始・変換・保存・送信経路を確認しました。
保存・ランプの条件は[スコア保存](../../docs/score-persistence.md)、送信形式は[IR](../../docs/ir.md)を参照します。

## 仕様

BATTLE / BATTLE ASは通常譜面と分離してスコア・ランプを保存します。
BMZ IRとBMS-IRへ専用区分で送信し、rianIRとBokutachiへは送信しません。
DBASのスクラッチ自動演奏だけではアシストランプにしません。

## 確認した不具合と修正

| 経路 | 原因と修正 |
| --- | --- |
| 入力・プレイ設定 | アプリのpreload / placeholderは元譜面のSP設定を選び、セッション単体のfallbackもDPをSPへ戻していました。DB / DBASは10K / 14Kの割り当て・HS・カバー・LIFT・判定表示タイミングを使います。選曲中の通常設定・詳細オプションの編集先も揃えます。 |
| DBASリプレイ | scratch autoplayを構築しても、full replay専用の固定判定時計では実行していませんでした。同じ時計で両側のscratch autoplayと自動releaseを進めます。 |
| 通常IR送信 | 複製済み譜面のキー数・ノーツ数を、元のSP譜面hashの情報として送っていました。譜面情報とルールのキー数はSPへ戻し、LN・地雷を含む譜面の内訳も複製前の値にします。採点結果のノーツ数・判定・EXは両側の値を維持します。 |
| 一括IR送信 | ライブラリの元譜面情報と倍増した採点ノーツ数を混ぜていました。通常送信と同様に分離します。 |
| ロード前のベスト参照 | BATTLEとAUTO SCRATCHの併用を、譜面変換時はDBASへ正規化しても、ロード前のscore keyではDBのまま扱っていました。DBASのbestを参照するよう揃えます。 |

G-BATTLEの相手側は表示専用のため、元譜面のSP設定を使う条件を維持します。
既存のスコア・IRキュー・IRサーバーの保存済みデータは自動では書き換えません。
特に既にDPとして登録された共有のIR譜面情報は、今回のクライアント修正だけでは修復できません。

## 検証

Debian 13 / Rust 1.100.0で、次の確認が通りました。

- `cargo fmt --check`
- `cargo check --workspace --locked`
- `cargo clippy --workspace --all-targets --locked -- -D warnings`
- 日本語ロケールで`cargo test --workspace --locked --no-fail-fast`。`bmz-player`は3,334件成功、68件ignoredです。

Rmz-skin / mz-select / Luxez-Flatはsubmoduleの固定commitを取得して検証しました。
Git管理外の第三者製スキンを必要とするテストには未実行・早期returnが含まれます。
Rust 1.100.0では、既存の`bmz-audio`のmanifestに未使用`anyhow`依存の警告が出ます。

回帰テストは5K→10K / 7K→14K、DB / DBASの両方を対象にします。
キーボード・2台のゲームパッドの入力変換、DPのプレイ設定、リプレイの25 / 100 / 300%再生と遅延フレームを確認します。
保存テストでは通常 / DB / DBASのbucket分離とBMZ IR・BMS-IRのキュー内容を確認します。

Windowsの実機・PHOENIXWAN・実スキン上の操作・本番IRへのスコア送信は未確認です。
報告のHTTP 400には送信ログがないため、すべての発生原因が今回のpayload不整合だったとは断定しません。
任意要望の「オートスクラッチの非表示」は新規表示設定として別途検討する範囲とし、今回の不具合修正には含めません。
