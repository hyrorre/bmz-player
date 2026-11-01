# ModernChicの読み込みエラーと未対応要素

実施日: 2026-11-01〜2026-11-02。対象commit: `35eb248d`（調査）、修正はその直後のcommit。
環境: Windows 11、debug / releaseビルド、ウィンドウ1280×720。

利用者から「ModernChicがエラーで動かない」と報告がありました。利用者の多い版（ModernChicOld、
Ver 4.2）と最新版（ModernChic471、Ver 4.71）を`data/skins`に置いて調べました。
第三者製スキンのため、どちらもGit管理外です。

## 調査方法

- [インストール済みLuaスキンの互換比較](../../docs/development.md#インストール済みluaスキンの互換比較)の
  ハーネスで、20個の`.luaskin`を初期ロード・scene状態・compatモードで読み込みました。
- 一時的なexample（コミットしていません）で、全property（2版で578通り）を1つずつ既定外に切り替えて
  読み込みました。filepathの選択肢は対象外です。
- 分離した`BMZ_DATA_DIR`で両版をprofileに設定し、同梱サンプル譜面のオートプレイで
  Select → Decide → Play → Resultを通しました。

既定設定では、両版ともSelect・Decide・Play・Resultを表示でき、実行時のcallback失敗もありません。

## 原因

### Decideの選択曲なしの読込

`decide.lua`はロード時に`CUSTOM.NUM.diffRGB()`（`Root/customnumber.lua`）で難易度色を取り、
nil確認なしで`RGB[1]`を参照します。難易度option 150..155がすべてfalseだと失敗します。
BMZはDecide進入時の再読込だけに選択譜面の難易度を渡していた
（[2026-10-21の修正](2026-10-21-lua-api-compat.md#追記decideのロード時の選択譜面)）ため、
起動時の先読みとskin設定パネルからの再読込は必ず失敗していました。

失敗はskin設定パネルに記録され、パネルはエラー文だけを表示してオプションを隠します。
Decide進入時の再読込が成功するまで、ModernChicのDecide設定を開くとエラーになり、
オプションも変更できない状態でした。報告の「エラー」はこれが最も可能性が高いと判断しました。

### Playのオプションによる読込失敗

総当たりで、次の既定外オプションだけが読込に失敗しました（両版共通）。

| オプション | 原因 |
|---|---|
| レーンカバーローテーション（5/7鍵）、左右のレーンカバーをローテーション表示（10/14鍵） | Oldは`luajava`の`java.io.File`がstubのため一覧が空になり、書いたはずのログを読み戻す`countRecords`で`io.open`がnilを返します。471はbeatoraja最新版の`main_state.file_list`が未定義です |
| プレイ状況詳細モード（5/7鍵） | `nentyakuinfo.lua`がioで作成したファイルを読み戻します。BMZはioの書き込みを永続化しません |

失敗するとPlayは既定スキンで続行しますが、上と同じくパネルがオプションを隠すため、
原因のオプションをパネルから戻せませんでした。

## 未対応だった要素

| 要素 | 状態 |
|---|---|
| destinationの`timer = timer_util.timer_observe_boolean(function() return isLampMenuOpen end)` | Luaの変数を読む観測関数は式へ変換できず、常にOFFでした。Selectのサイドメニュー（LAMP / RANKING / VOLUME / SETTING）・ヘルプ・放置時のキャラ動作・新バージョン表示、Result / Courseのメニュー（判定内訳・IR）が出ませんでした。警告はSelect 420件、Result 88件、Course 96件 |
| ioの書き込み、`file_*`、`luajava`の`File` | 上の2オプションのほか、「プレイ履歴の保存／表示」「サイドメニュー状態の保持」は`pcall`内で失敗し、何も起きません |
| `main_state.http_get_lines`（471） | 新バージョン確認が`pcall`内で失敗します。通信しない方針どおりです |
| 戦闘モード | 攻撃アニメの画像`timer`にLua関数が渡され、BMZは無視します（読込警告あり） |
| KeyConfigの`isOutlineFont`（471） | スキン側の不具合です。BMZはKeyConfigスキンを使いません |

## 修正

ioの永続化・`file_*`（ローテーション・詳細モード・履歴）と戦闘モードは今回の対象外です。

1. Decide進入以外の読込でも難易度の状態を渡します。skin設定パネル・profile切替等は表示中のDecide、
   なければSelectのカーソル位置の譜面、起動時や譜面がない場合は難易度未定義（150）・play level 0・
   空の難易度表textです。仕様は[skin.md](../../docs/skin.md#decideのロード時の選択譜面)です。
2. skin設定パネルは、ヘッダー（設定項目）が読めて本体だけ失敗した場合、エラーの下にオプションを
   表示します。ヘッダーも読めない場合は、別スキンの定義で保存済みオプションを正規化しないよう、
   従来どおりオプションを隠します。
3. destinationの`timer`に指定され、式へ変換できない`timer_observe_boolean`を、runtime VMの
   destination timer callbackとして評価します。`act`と同じVMなので、クリックで切り替えた
   メニュー状態を参照できます。runtime callbackのない位置（`dst`内、imageのアニメーションtimer）は
   従来どおり常にOFFと警告です。使われなくなった常にOFFの`dynamicTimer`は出力しません。
   仕様は[skin.md](../../docs/skin.md#lua-runtime-compatibility-mode)です。

## 検証

- 互換比較ハーネス: 変更前を2回、変更後を1回取得しました。回帰判定18件は、すべてModernChicの
  Select / Result / Courseで常にOFFの`dynamicTimer`が減った意図した差分です
  （Select 420→1、Result 88→0、Course 96→0）。代わりに同数のdestination timer callbackが増え、
  評価失敗は0件です。その他の差分はSocialSkinのロード時`math.random`等による揺れで、
  変更前の2回の間でも内容が変わっています。
- 実アプリ: 両版で起動時のDecide読込失敗と`timer_observe_boolean`の警告がなくなり、Selectの
  サイドメニューのボタン、Resultの判定内訳が表示されることをスクリーンショットで確認しました。
  サンプル譜面のSelect → Decide → Play → Resultも通りました。
- Select描画準備のCPU時間（`scene_plan_profile select <musicselect.luaskin> idle`、release、2000フレーム）:

  | skin | 変更前 平均 / p95 | 変更後 平均 / p95（2回） |
  |---|---|---|
  | ModernChicOld | 0.867 / 1.004 ms | 1.034〜1.039 / 1.134〜1.187 ms |
  | ModernChic471 | 0.875 / 1.015 ms | 1.038〜1.045 / 1.161〜1.174 ms |

  約420件のtimer callbackの評価と、表示されるようになった8件の描画命令の分で、約0.17 ms増えました。
  GPU・present・実FPSは測っていません。

ハーネスの出力、スクリーンショット、総当たりの結果は`.local/modernchic/`と
`.local/lua-api-compat/modernchic/`に置き、コミットしていません。

## 未確認事項・残課題

- サイドメニュー・Resultメニューのクリック操作は実機で試していません。`act`で状態を切り替えた後の
  timerの変化は[回帰テスト](../../crates/bmz-skin/src/tests/cases_03.rs)で確認しています。
- skin設定パネルの表示は単体テスト（egui）で確認し、実ウィンドウでは見ていません。
- 5/10/14鍵とコースリザルトの実機表示、beatorajaとの描画比較、Windows以外のOSは未確認です。
- ioの書き込みをメモリ上で読み戻せるようにし、skin root内に限った読み取り専用の`file_list`等を
  用意すれば、ローテーション・詳細モード・履歴（セッション内）が動きます。保存先の扱いは別途判断が
  必要です。
- 戦闘モードの画像`timer`には、destinationと同じcallbackを使う余地があります。
