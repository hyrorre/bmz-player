# Kaleid拡張へのmain統合とBokutachi条件

## 対象と合意

- 継続対象は[PR #82](https://github.com/hyrorre/bmz-player/pull/82)、`codex/kaleidbeat-ed-rian`。
- 統合前headは`1e00f33037e6bdc8dfac58f30bc9dd7ab81834ae`、今回取り込むmainは
  `0110163f73e83cfddbe51070788553e657d8d770`（[Bokutachi PR #86](https://github.com/hyrorre/bmz-player/pull/86)を含む）。
- 公開済みの履歴を維持してmergeする。実装は引き続き1つのPRとする。
- ユーザーはBokutachiについて、推奨した保留条件のうち「HLN→FORCE LNだけは許可したい」と指定した。
  元HLNを残してプレイする場合・FORCE CN/HCN・他のゲームプレイ拡張・部分対応・証拠不足は保留する。
  ローカル／BMZ IRの原本集約、rianIRのED＋サーバー条件、BMS-IRの既存保留条件は変えない。

現行の契約は[拡張仕様](../../docs/kaleid.md)、[Bokutachi](../../docs/bokutachi.md)を参照。

## 上流の読み取り調査

Tachi `adabf2c3aa8d53cde0eeab01f33de9824274d70a`の以下を確認した。
mainの実装記録が参照した`aebb8174`とは区別する。本番デプロイとの一致は未確認。

- [Javaプラグイン](https://github.com/zkldi/Tachi/blob/adabf2c3aa8d53cde0eeab01f33de9824274d70a/java/beatoraja-ir/src/main/java/bms/player/beatoraja/ir/TachiIR.java)は
  単曲を`/ir/beatoraja/submit-score`へ送る。
- [beatoraja parser](https://github.com/zkldi/Tachi/blob/adabf2c3aa8d53cde0eeab01f33de9824274d70a/typescript/server/src/lib/score-import/import-types/ir/beatoraja/parser.ts)は
  BMSにLR2oraja、PMSにbeatorajaというclient prefixを要求する。
- [converter](https://github.com/zkldi/Tachi/blob/adabf2c3aa8d53cde0eeab01f33de9824274d70a/typescript/server/src/lib/score-import/import-types/ir/beatoraja/converter.ts)は
  `chart.lntype != 0`と`hasRandom`を拒否する。可変ノーツ数にはimport frameworkとDB全体の変更が必要という注記がある。
- BMZは独立クライアントとして[Direct Manual](https://github.com/zkldi/Tachi/blob/adabf2c3aa8d53cde0eeab01f33de9824274d70a/typescript/server/src/server/router/ir/direct-manual/router.ts)を使う。
  [parser](https://github.com/zkldi/Tachi/blob/adabf2c3aa8d53cde0eeab01f33de9824274d70a/typescript/server/src/lib/score-import/import-types/ir/direct-manual/parser.ts)は
  [batch-manual parser](https://github.com/zkldi/Tachi/blob/adabf2c3aa8d53cde0eeab01f33de9824274d70a/typescript/server/src/lib/score-import/import-types/common/batch-manual/parser.ts)へ委譲する。
  API受付とKaleidの公平性・受入確認は別の問題として扱う。
- [BMS/PMS実装](https://github.com/zkldi/Tachi/blob/adabf2c3aa8d53cde0eeab01f33de9824274d70a/typescript/server/src/game-implementations/games/bms-pms.ts)と
  [共通検査](https://github.com/zkldi/Tachi/blob/adabf2c3aa8d53cde0eeab01f33de9824274d70a/typescript/server/src/game-implementations/games/_common.ts)は
  `chart.data.notecount * 2`をEX上限・率の分母に使う。採択枝・Kaleid解釈版に応じて分母を変える契約は確認できない。
- [段位受付](https://github.com/zkldi/Tachi/blob/adabf2c3aa8d53cde0eeab01f33de9824274d70a/typescript/server/src/server/router/ir/beatoraja/router.ts)は
  順序付き4曲MD5、LN、制約、配置等を検査するが、各曲の変換後採点数は照合しない。
- 公開仕様の[BMS](https://github.com/zkldi/Tachi/blob/adabf2c3aa8d53cde0eeab01f33de9824274d70a/docs/src/game-support/games/bms-7K.md)・
  [PMS](https://github.com/zkldi/Tachi/blob/adabf2c3aa8d53cde0eeab01f33de9824274d70a/docs/src/game-support/games/pms-Controller.md)も確認。
  旧game表記が残っているため、実通信のgame IDはmainで検証済みの現行形式を維持する。

HLN→FORCE LNは上流のKaleid公式受入を確認したものではなく、ユーザーの指定に沿ったBMZ側の許可条件。
原本SHA256/MD5を維持し、実効HLNなし・ForceLn／LN・登録採点数一致と既存制限を要求する。
他の拡張を変換で消して許可しない。段位も変換HLN曲を事前resolveする。
問い合わせや本番スコア送信は実施していない。

## 統合内容

- mainの採点確定・入力表示、Replay処理済み時刻、FREQ、AUTO単一LN型の保存区分正規化を維持。
  未評価分岐がある間は自然確定しない。動的Replayは実際に処理した時刻で確定する。
  全レーンHLN Replayの固定時計とBATTLEの空白区間スキップを保持する。
- mainの通常譜面用1パス前処理を共通化し、Kaleid構文を含む譜面だけ静的枝解決→動的分岐コンパイルを追加する。
  静的seedを二重に消費しない。スキャン時の資源探索省略を維持する。
  分岐コンパイル・再構築は資源探索をせず、ロード時に確定したBGAのパスを再利用する。
  ロード後にファイルの有無が変わっても、選択枝が参照する事前ロード済み資源を変えない。
- mainの描画キャッシュ・入力解決・非同期処理を維持し、分岐による譜面差替えでキャッシュを更新する。
- library 37とscore 35はmainのmigrationを維持し、Kaleidをlibrary 38／score 36へ移す。
  統合前の本ブランチの同番号DBは列・テーブルから識別して不足列を補い、既存データを残す。
  取込版10、Replay 8、IR Web migration 0017は維持する。
- Bokutachiは各プレイの保存済み解釈情報で判定する。原本・実効の両方を確認し、宣言だけでは拒否しない。
  単曲・各ステージ・段位と再送へ接続し、対象外を通信障害として再試行しない。
  受付済みimport IDのGET確認を継続しつつ、新規POST・再POSTは新条件で判定する。

## 検証

Linux、Rust 1.99.1、`LANG=ja_JP.UTF-8`、`CARGO_INCREMENTAL=0`で確認した。
検証ログは作業環境の`/tmp/bmz-main-*.log`に保存し、コミットには含めない。

- `cargo fmt --check`、`cargo check --workspace --locked --offline`、
  `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`が成功。
- `cargo test --workspace --locked --offline --no-fail-fast`で全crateを実行。
  新規の空DEFAULT終了テストだけ、終了猶予を短く仮定したfixtureで失敗したため、
  現行の`SESSION_END_MARGIN_US`経過後まで進めるよう修正した。
  修正後の`cargo test -p bmz-gameplay --locked --offline`は全325件成功（1件ignore）。
  playerは3,182件、chartは203件＋fixture 29件、audioは174件、renderは1,021件が成功。
- 追加確認は、空DEFAULTから後続枝を採択するAUTO／Replayの採点確定と自然終了、
  分岐時のBGAパス再利用、統合前Kaleid DBのデータ保持とmain列追加、Bokutachiの
  HLN→FORCE LN・登録採点数一致／不一致・原本hash維持・段位各曲の条件・
  旧キューの自動再試行停止・受付済みimportのGET継続を含む。
- `bun run test:ir`は223件成功。`bun x vue-tsc --noEmit`と対象WebファイルのPrettierも成功。
- 同梱submoduleのLuxez-Flat、Rmz-skin、mz-selectをmainの指定commitで初期化した。
  外部素材がない場合に早期returnするテストや既存ignoreは、実機互換性の証明には数えない。

初回のRustリンクは作業ディスクの空き不足で失敗した。古いビルド生成物だけを整理して再実行した。
HTTPモックはローカル待受が必要なため、sandboxの待受制限外で実行した。
本番IR送信・管理人への問い合わせ・実データDBの変更・サーバーデプロイ、
実機GPU／音声プレイ、ED本体との同一入力実測は実施していない。

## 同日の追加main統合

main `e96fb2b6`の更新で生じた6ファイルの競合を解消した。
KaleidのRGB反転pipelineとmainのComposite pipelineを併存させ、
HLNの終端描画条件にmainの独立tail cap対応を統合した。記録索引は両方を保持した。
mainで追加されたBATTLEテストにはKaleidの分岐状態の初期化を補った。
IR送信条件は変更していない。検証ログは`/tmp/bmz-conflict-*.log`。
fmt、workspace check、all-targets Clippy、workspace全テストが成功した。
player 3,222件、gameplay 329件、render 1,040件が成功。既存ignoreは維持した。
