# OBSコマンド登録と画面ライフサイクル

日付: 2026-10-30

## 対象と参照

- 仕様・操作: [OBS WebSocket連携](../../docs/obs.md)、[設定画面](../../docs/settings.md)
- BMZ調査開始時: `e5378789`。`origin/main`と一致、親リポジトリの作業差分なし。
- 作業ブランチ: `codex/obs-command-rules`
- [ED-rian ObsListener](https://github.com/rian-eimu/lr2oraja-endlessdream/blob/main/core/src/bms/player/beatoraja/obs/ObsListener.java)
  (`dafbd48aa9114494a5731ed7b38785f148484500`)
- [ED-rian ObsWsClient](https://github.com/rian-eimu/lr2oraja-endlessdream/blob/main/core/src/bms/player/beatoraja/obs/ObsWsClient.java)
- [ED-rian ObsControlCommand](https://github.com/rian-eimu/lr2oraja-endlessdream/blob/main/core/src/bms/player/beatoraja/obs/ObsControlCommand.java)
- [ED-rian ObsConfigurationView](https://github.com/rian-eimu/lr2oraja-endlessdream/blob/main/core/src/bms/player/beatoraja/launcher/ObsConfigurationView.java)
  (`0dcc0e957fe1d340b5a66a48ed946db3c8917730`)
- [ED-rian Config](https://github.com/rian-eimu/lr2oraja-endlessdream/blob/main/core/src/bms/player/beatoraja/Config.java)
- [MainController](https://github.com/rian-eimu/lr2oraja-endlessdream/blob/main/core/src/bms/player/beatoraja/MainController.java)
  (`b046eaa2cba5b2560c3ac0e0540694528d7bcd46`)、[BMSPlayer](https://github.com/rian-eimu/lr2oraja-endlessdream/blob/main/core/src/bms/player/beatoraja/play/BMSPlayer.java)
  (`e4d2ddfb696c808c58165d6f2646d2fe4278ce41`)
- [参考PR #254](https://github.com/seraxis/lr2oraja-endlessdream/pull/254): 調査時点はopen。
  head `73fda0d29f43a6486c259c1e7960a7caa72d0ed8`。
- [OBS WebSocket v5 protocol](https://github.com/obsproject/obs-websocket/blob/master/docs/generated/protocol.md)
  (`ebff6d641b4c1e4c1420b210f0866e2449b44157`)

## 参考実装の確認結果と採用範囲

| 項目 | ED-rian | BMZでの判断 |
| --- | --- | --- |
| イベント時点 | MainStateListenerが旧END→新START。BMSPlayer.stopPlayがPLAY_ENDを先行通知しフラグで重複防止。MAIN_STARTは認証確認を500ms間隔、最大20回 | 画面通知・先行終了通知・実プレイの再設置を共通状態機械へ渡す。MAIN_STARTは認証完了後に1回 |
| 登録と順序 | ArrayListの登録順にsend。応答完了は待たない | 新形式は要求IDが一致する応答を待って次へ進む。メインスレッドでは待たない |
| 一覧取得 | GetSceneListの逆順、各シーンへGetSceneItemList | 既存の逆順を維持し、シーン別のソース候補を追加。設定一覧取得も要求IDを検証 |
| ソース操作 | GetSceneItemIdの応答からSetSceneItemEnabled。未完了lookupはMap管理 | 同じAPIを使い、期限・画面世代も検証。IDは設定に保存しない |
| 録画開始・停止 | 接続/認証/録画状態でガードしStartRecord/StopRecord。停止はschedulerでディレイ | 既存の録画状態・保持判定を共有。後続操作があれば停止を先に完了させる |
| リトライ | Play→Playで、録画再開始と擬似MusicSelect→1秒後Play。STARTのみで、PlayEndの再設定も通常遷移とは別 | 新形式は実際のPLAY_END→PLAY_START。録画再開は停止完了後500ms。旧形式の擬似選曲処理は保持し、遅延操作に世代を付加 |
| ファイル保持 | KeepAll以外で、次のSTARTED時に前のファイルを判定。スクリーンショット/リプレイでsaveRequested。リトライは削除 | 既存BMZの判定を維持。終了時の保持指定はShutdownより先にキューへ送る |
| 再接続 | 2秒開始、1.25倍、最大15秒。ExitStartedからcloseするとautoReconnectも無効になる | BMZ既存の再接続を維持。OBS終了後も待機する。切断中の履歴は圧縮、復帰時の録画操作は再生しない |
| 移行 | コマンド一覧が空なら旧形式から構築。全削除しても旧設定が復活し得る | NoneとSome(empty)を区別。初回コマンド編集時だけ移行し、旧設定との二重実行を防ぐ |
| UI | JavaFXの入力欄＋TableView、追加/削除/全削除。ソース候補はシーン選択時に更新 | eguiの行リスト。行内編集、上下移動、未接続時の直接入力と名前保持、6言語対応 |

ED-rianの未処理アクション名を汎用sendRequestへ渡すフォールバックは採用しない。
参考実装の例外処理・scheduler・WebSocketClientをRustへ移植せず、
BMZの既存Tokioタスクと接続状態通知を利用する。依存crateや専用threadは追加しない。

## 実装境界

- `config/app_config.rs`: 12種類のObsTrigger、5種類のObsRuleAction、Option<Vec<ObsRule>>。
  旧scenes/actionsは読み込み・保存を維持する。ルールが存在すれば旧形式を実行しない。
- `obs/lifecycle.rs`: END/START、先行PLAY_END、リトライ、同じPlayでの次プレイを管理する。
- `app/play_flow/launch/start.rs`: 実際にactive_playを設置したときに、
  同じPlay画面でも次の開始を通知できるようapp側の画面重複ガードを解消する。
- `obs/rules.rs`: 1本のsocket上で要求を逐次実行。ソースlookupとsetterを連続した段階として扱う。
  応答期限2秒、シーン準備待ち207のみ最大8回再試行。通常の失敗後も独立した後続処理へ進む。
  未実行の古い表示操作、古いlookup、古い再試行は次の画面へ持ち込まない。
- 再接続は過去のSTART/ENDを再生しない。初回接続ではMAIN_STARTと現在画面のSTART、
  再接続では現在画面のSTARTにある表示操作だけを同期する。
- 設定編集によるCloseと実終了のShutdownを区別する。設定再作成でMAINイベントを発火しない。
  終了の待機はゲームループ終了後にasyncで行い、応答なしでも上限で終了する。
- `sync_obs_controller()`のプレイ/Result中の設定反映保留は維持する。
- UIの一覧取得は実行用controllerと独立した短命接続。接続先変更後の古い応答は表示へ反映しない。

## 自動検証

ルールのTOML往復、明示的な空一覧、旧設定移行、不正なvariant/空名、
START/ENDと重複、5操作の逐次実行、同名ソースのシーン別ID、
不存在シーン/ソース・setter失敗後の続行、207再試行上限、
停止ディレイ、保持条件9組、リトライの削除/500ms再開、
実WebSocketを使う起動/終了・再接続・タイムアウト/遅延応答・一覧取得を追加した。
UIの未接続閲覧・名前保持・全言語のラベル解決も確認する。

最終検証（Windows、ローカル通信を利用可能な実行環境）:

- `cargo fmt --check`: 成功。
- `cargo check -p bmz-player --locked`: 成功。
- `cargo clippy -p bmz-player --all-targets --locked -- -D warnings`: 成功。
- `cargo test -p bmz-player --locked --lib obs::`: 30成功、失敗なし。
- `cargo test -p bmz-player --locked --quiet`: lib 2,727成功、44 ignored、失敗なし。
  startup_stackの2件も既存のignored。合計46 ignoredを手動確認済みとは扱わない。
- 今回追加したテストは22件。設定・多言語カタログの既存テストも全体実行で通過。
- 差分・ローカルMarkdownリンクを確認。Cargoのincremental hard-link失敗はcopy fallbackの警告で、ビルド/テストは成功。

実行環境の制限下では、ループバックの接続がOSエラー10013で拒否され、
既存IR/OBS通信テストも失敗した。同じ全体実行では既存依存`gilrs-core`の
Windows WGI callbackの`SendError`によるpanicとプロセス終了(code 3)も観測した。
ローカル通信を許可した環境でOBS関連30件はすべて通過した。
モックのacceptにも期限を設け、通信を拒否された場合もテストが待ち続けないようにした。
制限下の失敗を、未検証のまま機能の成功とは扱わない。

## 未実施と制約

- 実OBS、実GPUでの画面操作、Windows以外のOSでのbuild/実機検証は未実施。
  自動テストの成功をこれらの確認済みとは扱わない。
- GetSceneItemListはシーン直下だけを列挙する。グループ自体は操作できるが内部再帰は未対応。
  同一シーン内の重複ソースはsearchOffset=0の最初の項目を操作する。
- 要求送信後のOBS側の処理を取り消すAPIはない。世代で防ぐのは未送信の処理、
  lookup応答からのsetter発行、再試行であり、送信済み操作の取り消しではない。
- ルールは512行、未実行キューは2048行で制限する。
- 切断中に発生した録画操作は復帰時に再実行しない。録画状態をGetRecordStatusで確認する。
- 最後の停止済み録画は次の録画開始がなければ残る。これは既存の保存判断タイミングを維持したもの。
- 任意API、リプレーバッファの新制御、フィルター、音声ミキサー、外部プログラム実行は対象外。
