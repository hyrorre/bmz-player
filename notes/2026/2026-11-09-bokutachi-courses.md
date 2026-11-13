# Bokutachiのコース各曲送信とBMS段位更新

## 起点と受信契約

- PR #86の`feat/bokutachi-single-score`、`4122c9b4`を確認しました。作業開始時の未コミット変更はありませんでした。
- 最新mainの`48d48cab`を`bd7d3be9`で取り込みました。単曲・PMS対応と今回の追加は同じPRです。
- TachiのHEADは引き続き`aebb817466213245329ab8bdcab82bebea3f51fc`でした。
- [段位受付](https://github.com/zkldi/Tachi/blob/aebb817466213245329ab8bdcab82bebea3f51fc/typescript/server/src/server/router/ir/beatoraja/router.ts)は4件の`{md5}`を順に連結して対応表を検索します。`clear`・`lntype=0`・制約・packed option全体を検証します。
- [バージョン検証](https://github.com/zkldi/Tachi/blob/aebb817466213245329ab8bdcab82bebea3f51fc/typescript/server/src/server/router/ir/beatoraja/auth.ts)はv2／v3／v4を受け付けます。`X-TachiIR-Version: v4`を使い、クライアント名は偽装しません。
- [対応表の型](https://github.com/zkldi/Tachi/blob/aebb817466213245329ab8bdcab82bebea3f51fc/typescript/db/src/generated/public/BmsCourseLookup.ts)で`set`・`value`が文字列であることを確認しました。段位の割当はサーバーへ委ねます。
- [Java連携](https://github.com/zkldi/Tachi/blob/aebb817466213245329ab8bdcab82bebea3f51fc/java/beatoraja-ir/src/main/java/bms/player/beatoraja/ir/TachiIR.java)も段位送信とコースPB非対応を分けています。
- beatorajaのHEAD `0430bf010e4123159ffc6260ffd76abab54305ab`の[BMSPlayer](https://github.com/exch-bms2/beatoraja/blob/0430bf010e4123159ffc6260ffd76abab54305ab/src/bms/player/beatoraja/play/BMSPlayer.java)、[MusicResult](https://github.com/exch-bms2/beatoraja/blob/0430bf010e4123159ffc6260ffd76abab54305ab/src/bms/player/beatoraja/result/MusicResult.java)、[CourseResult](https://github.com/exch-bms2/beatoraja/blob/0430bf010e4123159ffc6260ffd76abab54305ab/src/bms/player/beatoraja/result/CourseResult.java)で配置値、ステージランプ、コース集計・送信を確認しました。2P／DP設定をpacked値へ含めるのはDP時です。

## 実装判断

コースステージは既存の保存済み結果から単曲ジョブを作ります。Bokutachi context v2でステージ由来・元ランプ・完奏状態を固定し、v1の通常単曲ジョブは引き続き処理します。NO PLAY例外は確認済みステージだけです。途中終了のBP／CBは元の結果から取り、変換ランプによって未処理ノーツ分を失わないようにしました。

COMPLETE SONGは実際の完奏、UPDATE SCOREは各指標の改善を使います。PMSの区分と比較元は各ステージに保存された入力機器です。他IRのペイロード、ローカルBEST、既存コース保存は変更しません。

段位は`ir/bokutachi/dan.rs`へ分離しました。元のコース定義と実際の4曲を照合し、各ステージの保存・実効アシスト条件、元モード、キー変換、LN、ルール、ゲージ、完奏、譜面分岐、配置を確認します。早期の結果確定では既存`result_is_settled`の証明時刻を引き継ぎ、曲数やcourse_clearだけで完奏と判断しません。

LR2orajaはGaugeProperty指定にかかわらずLR2定義を使うため、GAUGE_LR2文字列の有無とは別に実効ルールと段位ゲージを確認します。一方、CN／HCNや非LR2のGAUGE制約は削除せず拒否します。14Kのoption 10／11／100／111は0／1へ丸めず拒否し、単曲送信のF／MF→RANDOMとは独立させました。

ゲージ自動切替では各曲の表示ゲージが異なる場合があります。実際のゲージ持ち越し値も固定し、コース最終ゲージが全4曲で生存していることを確認します。各曲の表示ゲージ一致を要求して正当なクリアを除外せず、持ち越し情報の欠落・重複・0・非数値は拒否します。

専用のversion付き要求を既存Course outboxへ保存します。ローカル固定IDは再enqueue時の上書きを防ぐためのもので、APIヘッダーには送りません。再送で現プロフィール・ライブラリ・コース定義を参照しません。HTTP 200のdescriptionとbodyから更新・変更なし・未クリアを区別します。404は対象外、400等は終端、通信失敗・429・5xxは既存方針で再試行します。単曲のimport状態・スコアID・成功台帳・Replay／attestationは生成しません。

段位の更新は単曲・合計のBESTや単曲送信結果に依存しません。結果画面とキューに段位専用表示を追加し、6言語の説明を更新しました。追加のDB migrationは不要です。

## 検証

Linux x86_64、Rust 1.99.1と前回用意したworkspace内の開発依存を使用します。Cargoコマンドは`sh /tmp/bmz-cargo`で環境を指定し、全テストは日本語ロケールで実行します。

関連テストの初回では、以前の「コース一律未対応」というエラー文を期待するテストが1件失敗しました。旧Course payloadが実プレイ情報不足で通信前に拒否されることを期待するよう修正しました。

| コマンド | 最終結果 |
| --- | --- |
| `cargo fmt --check` | 成功 |
| `cargo check --workspace --locked` | 成功 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 成功 |
| `cargo build -p bmz-player --locked` | 成功（Linux、既定feature） |
| `LANG=ja_JP.UTF-8 LANGUAGE=ja LC_ALL=ja_JP.UTF-8 cargo test --workspace --locked --no-fail-fast` | 成功。bmz-playerは3,057成功・50 ignored。workspace全体とdoc-testで失敗0 |
| `cargo test -p bmz-player --locked bokutachi` | 30成功、失敗0。ゲージ持ち越しの補強後も上記workspace実行で関連30件の成功を確認 |
| `git diff --check` / `git diff --cached --check` | 成功 |

HTTPスタブ・一時DBで、4モードのステージランプ、元のEX・BP・コンボ・全曲ノーツ数、通常NoPlayの拒否、実際の完奏に基づく送信方針、PMSの各ステージの入力機器と独立BEST、段位の全4曲の証拠・順序・制約・配置・ゲージ持ち越し、同期応答の3分類、不正応答、404・400・401・403・429・503、再起動と手動再試行時の要求固定、旧形式・アカウント不一致の拒否、単曲ジョブと段位の独立性を確認しました。

6言語のBokutachi用11キーの一致・重複なし、6言語マニュアルのコース説明、仕様書と本記録の相対リンクも確認しました。最終のゲージ補強後にfmt・check・Clippy・build・全workspaceテストを再実行しています。生ログは実行環境の`/tmp/bmz-course-*-final.log`等へ保存し、コミットには含めません。

## 未実施

本番APIへの段位・スコア投稿、実アカウントでの動作確認、実機GUI／7K・14K・9Kコースプレイ、Windows／macOSのビルドと資格情報ストアは未実施です。HTTP通信はローカルスタブだけです。GPU等を必要とする既存ignoredテスト、一部参照アセットがない場合に早期returnするテストは実機互換の検証に含めません。

現在の条件と操作は[仕様書](../../docs/bokutachi.md)、追加前の検証記録は[単曲・PMS対応](2026-11-09-bokutachi-submission.md)を参照してください。
