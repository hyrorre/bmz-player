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

## 追記：メモリ上のスキンファイル（2026-11-02）

上の「未対応だった要素」のうち、ioの書き込み・`file_*`・`luajava`の`File`に対応しました。
ディスクへは書かず、書いた内容をメモリに残します。仕様は
[skin.md](../../docs/skin.md#メモリ上のスキンファイル)です。

### 訂正

上の総当たりは読込エラーだけを見ていました。背景ローテーション（Select / Result / Course）と
キャラクターローテーション（Result / Course）も同じ仕組みで、`pcall`内で失敗して背景・キャラクターの
部品がまるごと消えていました（471のSelectでsource・image・destinationが11→10、1438→1436、
1914→1908、Resultは13→9、125→119、194→187）。読込は成功扱いのため、エラー一覧に出ていませんでした。

### 判断

- 1回のロードではスキンを複数回実行します（document VMの本体、callback推論、runtime VMの本体の
  再実行）。すべての書き込みを残すと、履歴の二重追記や、推論で呼ばれた`ENDOFNOTE`の処理による
  ローテーションの消費が起きます。後のロードへ残すのは、成功したdocument VMの本体と、
  実行時のcallbackの書き込みだけにしました。
- 保存領域はappのskin pipelineに1つ置き、アプリ終了まで保持します。保存先の判断が要る
  ディスクへの永続化は行いません。
- `skin_config.get_path`の絶対パスは従来`io.open`で常にnilでした。スキンのroot内に限って読めるように
  しました。相対パスの`..`拒否は維持しています。
- `file_list`のパターンはbeatorajaのJava正規表現ではなくLuaの`string.find`で照合します。
  正規表現エンジンを追加せず、スキンが使う形は同じ結果になるためです。

### 検証

- 回帰テスト: 読み戻し・保存領域での引き継ぎ・推論と再実行の書き込みの破棄・`act`の書き込み・
  失敗時の破棄・ディスク非書き込み、`file_*`、`java.io.File`、`*n`の読み取り
  （[bmz-skin](../../crates/bmz-skin/src/tests/lua_skin_files.rs)）、app経路での引き継ぎ
  （[bmz-player](../../crates/bmz-player/src/app/skin_loading/startup_tests.rs)）。
- 総当たり（一時example、コミットしていません）: 両版の全property 578通りで、読込失敗は
  KeyConfig（471、スキン側の不具合）だけになりました。背景・キャラクターのローテーションは
  既定と同じ部品数になりました。
- 互換比較ハーネス（変更前2回・変更後1回）: 回帰判定0件。差分はSocialSkinだけで、Decide / Resultは
  従来からの乱数による揺れです。Selectは、`main_state.file_mkdir`がある環境を最新版beatorajaと
  判定し、`open_url`のない環境では動かないURLを開くボタンの描画callbackがなくなりました。
- 実アプリ（471、レーンカバーローテーション・詳細モード・履歴保存・背景ローテーションを有効）:
  Select → Decide → Play → Resultを通し、Lua実行エラーはありません。Playで詳細モードの分析表示と
  レーンカバーの使用枚数が表示されました。ディスク上の`io/`・`History/`は更新されていません。
- Play描画準備のCPU時間（`play_plan_profile`、release、471 play7）: 既定0.116 ms、
  ローテーション＋詳細モード1.03 ms。詳細モードのトレンドグラフが640本のLua timer関数を
  毎フレーム評価するためです。

### 残課題

- 詳細モードのグラフ・数値とResultの履歴表示が、beatorajaと同じかは比較していません。
  オートプレイでは瞬間密度が100 notes/sec超と表示され、トレンドグラフの右端が白く塗られました
  （原因と修正は下の追記）。
- 詳細モード有効時に`mixed lua table converted to object at $.destination[55].dst[1]`の読込警告が出ます
  （原因は下の追記）。
- Resultの再decode（IR結果の反映等）が起きると、その分も履歴へ追記されます。
- 戦闘モードの画像`timer`は未対応のままです。

## 追記：Play中のtimer開始時刻の揺れ（2026-11-02）

### 原因

詳細モードの数値が異常だったのは、BMZがLuaへ返すtimerの開始時刻の揺れが原因でした。

- 詳細モードは、レーンごとのtimer（オートプレイはボム50..57、手動はKEYON 100..107）の開始時刻が
  前回と異なるたびに1打鍵と数えます（`Play/lua/sp/detailinfo/bgaareainfo.lua`の`performance`の値callback）。
- BMZのPlayのtimerはgameplay threadのsnapshotの譜面時刻からの経過ミリ秒、`main_state.time()`は
  window threadの画面経過時間です。Luaへの開始時刻は「画面時刻のms − 経過ms」で逆算するため、
  2つの時計の差の端数でフレームごとに1 ms揺れます。
- ModernChicのコピー（`.local`内、診断用の`print`を追加）を実アプリで動かすと、28ノーツの
  サンプル譜面で打鍵を597回検出しました。ボムの開始時刻は6516000 / 6517000 µs、PLAY timer（41）も
  4516000 / 4517000 µsを交互に返していました。
- 瞬間密度100超、トレンドグラフの白い領域（密度50以上で上限の白）、レーン密度の棒のはみ出しは、
  すべてこの数えすぎによるものです。同じ判定方式はModernChicの攻撃演出（`attack.lua`）、
  PeacefulPlayのkey logger（compatモード）にもあります。

`mixed lua table`警告はスキン側の記述（`Play/lua/sp/keyflash.lua`の
`dst = {{0, x = ...}}`の余分な`0`）が原因で、BMZは`0`を無視して残りのfieldを使います。

### 修正と判断

runtime VMがtimerごとに前フレームの開始時刻を覚え、変化が2 ms以内なら前回返した値を返します
（[skin.md](../../docs/skin.md#lua-runtime-compatibility-mode)）。比較を前フレームの生の値と行うため、
時計の緩やかなずれが積み重なって誤った再起動になることはありません。

時計を1本にそろえる案（Playのtimerを画面時刻へ固定オフセットで写す）も検討しました。ボム・KEYON・
判定・HCN等のsnapshot構築と描画テストに広く手が入るため、今回はLuaへの返却値だけで吸収しました。
同じtimerが2 ms以内に再起動した場合は区別しません。全レーン共通の判定timer（46 / 47）では、
ほぼ同時の判定が1回にまとまります。

### 検証

- 回帰テスト: ±1 msの揺れと1 msずつのずれでは同じ開始時刻、再起動・OFF・再ONでは新しい値
  （[read_helpers.rs](../../crates/bmz-skin/src/tests/read_helpers.rs)）。
- 実アプリ（診断用コピー、オートプレイ）: 打鍵の検出が597回から28回（ノーツ数と一致）になり、
  PLAY timerも一定値になりました。表示は区間平均1.04、最大密度3.62、瞬間密度0.98、連打分布28回、
  レーンごとの打鍵数4×7で、白い領域とはみ出しはなくなりました。
- インストール済みLuaスキンの互換比較: 回帰0件。差分はSocialSkin Decideの乱数による揺れだけです。
- 手動プレイ（KEYON）と戦闘モードの攻撃演出の実機確認、beatorajaとの数値比較は未実施です。

## 追記：オプション画面の譜面オプションとResultのIRメニュー（2026-11-02）

利用者の画面写真で、2件の表示不具合が報告されました。

### オプション画面の譜面オプションが6行分光る

- 原因: Selectのオプション画面は、`op.png`（2000×3000）から縦に24 pxずつずらした
  143×240の窓を切り出し、選択中の行だけを光らせます。NORMALの窓はy=2964〜3204で、
  画像の下端を204 pxはみ出します。BMZはJSON / Luaの画像でも切り出し範囲を画像内（36 px）に
  詰め、それをDSTの240 pxへ引き伸ばしていたため、24 pxの選択枠が約160 px（6.7行分）に伸びていました。
- beatoraja: `JsonSkinObjectLoader.getSourceImage`は範囲を詰めずに`TextureRegion`を作り、
  はみ出した部分はlibGDXの既定（ClampToEdge）で端の画素を引き延ばします。
- LR2: 始点が画像外の`SRC`は描かず、終点がはみ出す分は幅・高さを画像内に詰め、
  DSTへ引き伸ばします。BMZの従来の扱いと同じです。
- 修正: LR2の画像に`lr2Clip`の印を付け、従来の扱いを残しました。それ以外の画像は範囲を詰めず、
  UVを0〜1の外まで渡します（スキン画像は個別テクスチャで、samplerはClampToEdge）。
- 影響: インストール済みLuaスキンで画像からはみ出す切り出し範囲は、ModernChicのほか
  GenericTheme（1 px）、WMII_FHD（2 px、画像端からの開始）、SocialSkin、ADFX02等にあります。
  いずれもbeatorajaと同じ描画になります。選択式画像は先頭候補の大きさで判定したため、一部は誤検出です。
- 検証: 描画計画のダンプ（`scene_plan_profile select ... panel`）で選択枠のUVが
  `y=0.988, 高さ0.08`（240 px分）になったことを確認しました。実画面での目視は未実施です。
  始点が画像外のLR2の`SRC`は、LR2では描かれず、BMZは幅0の範囲で描きます（今回は変更していません）。

### ResultでIRメニューが表示されない

- 原因: ModernChicのResultは、メニューを`timer_util.timer_observe_boolean`で組み立てた後に
  `main_state.option(51)`（オンライン）を見てIRメニューを読み込みます。`timer_observe_boolean`は
  `main`の実行中に観測関数を推論し、推論の終わりに`main_state`の値をすべて消していました。
  そのため以降の`option(51)`はfalseになり、IRメニューの部品（画像30、imageset 10、value 68、
  配置約164）がまるごと組み立てられていませんでした。`option(90)`や`number` / `text`も同様に失われます。
- 修正: 推論の前に読み込み時の値と依存関係の記録を退避し、推論後に戻します。
- 検証: 最小構成の再現スキンと、ModernChic Old / 4.71のResultをオンライン状態で読み込み、
  IRメニューの部品が加わることを確認しました。互換比較は回帰0件（差分はSocialSkinの乱数の揺れのみ）。
  IR接続した実環境での表示確認は未実施です。
