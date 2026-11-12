# スキン互換の対応状況

この文書は、互換修正や次の作業を選ぶための索引です。最終確認日は2026-10-06です。
「対応済み」は記載した範囲の実装・テストがあることを意味し、全スキンでの見た目の完全一致を保証しません。
BMZ固有のIDや詳細な動作契約は[skin.md](skin.md)、互換修正の方針は[AGENTS.md](../AGENTS.md)を参照します。

対応範囲を変更したら、根拠となるコード・テストとともに該当行を更新します。
不具合を追加するときは、スキン名・画面・条件・期待結果と実際の差を記載し、未検証の推測と区別します。
第三者製スキンや個人の設定・DBはコミットに含めません。

## 対応済みの主な範囲

判定線とノーツの重なりはスキンの定義順に従います。ECFNの`destination`とQwilightの`pipeline`に対し、
判定線を手前へ移す補正は行いません。ECFN・CR_minimalの実素材での確認範囲は
[作業記録](../notes/2026/2026-11-13-play-judge-line-order.md)を参照してください。

Qwilightのプレイスキンは試験対応です（2026-11-10）。YAML / Lua / ZIPを読み、通常ノーツ・LN・基本HUD・装飾を描画します。
HS・達成率はBMZの表示HSとプレイ中EXスコアレートを小数点以下2桁で表示します。判定画像・回数欄ではBMZのPOORをPOOR、空POORをfailedへ割り当てます。
10種類×8キー数の定義・素材パス検証と、Default / CR_DXの7K、CR_simpleの14KのGPUオフスクリーン描画を確認しています。
カスタマイズは、`set*`で宣言された位置・サイズの調整、`configure-available-*`の条件付き表示、言語別の表示名、
SUDDEN+で描くレーンカバー（スキン同梱・追加画像）に対応します（2026-11-10）。
10種類×8キー数で調整値を変えた読み込みと、CR_DXの7K・Defaultの14KのGPU描画を確認しています。
FAST/SLOW回数・判定誤差ms・PGREAT対その他判定の比率（RATE）、省略形/mode 2の背景、
左右の枠、フレーム番号付きのゲージ素材、判定画像の反復回数に対応しました（2026-11-13）。
判定線位置のスキン調整は削除し、BMZのLIFTへ統合しました。
検証条件は[CR_minimalの表示修正](../notes/2026/2026-11-13-qwilight-play-visuals.md)を参照してください。
曲情報の省略幅・文字配置、NPS、経過/総時間、譜面レベル・難易度表・難易度バッジ、
打鍵タイミング、判定推移、出力PCMのスペクトラムと、26項目の個別表示切り替えに対応しました。
値・判定履歴・音声解析はBMZ側のものです。[追加表示の検証記録](../notes/2026/2026-11-13-qwilight-play-information.md)を参照してください。
打鍵タイミングの座標は判定幅に依存しない固定の±250msとし、0msを中央に置きます。
別テーマの遷移定義は読み込まず、入場・退場に500msの黒フェードを使います。
フルコン・全PGREAT・通常クリアの画像演出、再生倍率・ゲージ%・目標差、判定内訳の帯、進行バー・つまみ、
AUTOPLAYとレーン別FAST/SLOW、キー画像の往復、コンボの画像・拡大、拍連動の装飾とゲージ、CanvasCompositeの合成にも対応します。
レーン別FAST/SLOWはBMZのAuto／閾値設定に従い、表示対象外の判定では直前の演出を解除・延長しません。
時間表示は経過／残りを選べ、追加表示もON/OFFできます。詳しいBMZ値との対応は[スキン仕様](skin.md#qwilightのプレイスキン)を参照してください。
[タイミングとフェードの検証記録](../notes/2026/2026-11-13-qwilight-timing-and-fades.md)を参照してください。
テーマ、選曲・リザルト、Qwilight固有のオンライン・ポーズ・カメラ・設定用UI、専用SE、独自のカバー高さ・開閉操作は対象外です。
[仕様と制限](skin.md#qwilightのプレイスキン)、[実素材テスト](../crates/bmz-player/src/skin_loader/tests/qwilight.rs)、
[実装記録](../notes/2026/2026-11-10-qwilight-play-skins.md)を参照してください。

Resultのお気に入りは、単曲・コース曲間で`act = 89 / 90`とimage/index `ref = 89 / 90`に対応します。
スキンにボタンがなくても選曲と共通のF8 / F9割り当てで操作できます。2状態・コース最終での制限は
[仕様](skin.md#resultのお気に入り)、[入力・保存の回帰テスト](../crates/bmz-player/src/app/tests/result_favorites.rs)を参照してください。

| 項目 | 現在の範囲・確認入口 |
|---|---|
| 曲バーの文字列 | beatoraja / LR2と同じく、JSON / Lua / LR2の全形式で曲バーにフルタイトル（タイトルと半角スペースとサブタイトル）を描画します。`ref=10`はタイトルのみを維持します。LR2の`disablesubtitle`相当の設定はありません。[回帰テスト](../crates/bmz-player/src/app/tests/select/cases_04.rs) |
| LR2選曲バーの分類・装飾 | NEW COURSE・コース一覧・機能フォルダ・検索履歴をLR2の画像番号で描画します。NEWはルートと配下を新曲、設定カテゴリ（音量・音声など）は通常フォルダ（W-MIXの青）、設定入口・戻る・閉じるはライバルフォルダ（W-MIXの赤）、その他の仮想フォルダは分類階層を含めカスタムフォルダの画像とします（BMZ表示方針）。op 1..5、新着譜面のBAR_TITLE切替、各曲のライバルEXに応じたランプとBAR_RIVAL勝敗表示に対応します。BAR_RANKとフォルダの新着タイトル判定は未対応です。[仕様](skin.md#lr2のselect--result)、[回帰テスト](../crates/bmz-render/src/skin/tests/result/lr2.rs) |
| LR2選曲バーの描画順 | 各BAR部品の最初のDST位置をinclude先も含めて保持し、通常画像・パネル・マスクと同じ宣言順で描画します。条件分岐では有効なdestination自身の位置を使います。発光の中央位置とJSON / Luaの行ごとの描画は維持します。[仕様](skin.md#lr2のselect--result)、[回帰テスト](../crates/bmz-player/src/skin_loader/tests/lr2/select_bar_order.rs) |
| JSON / Lua / LR2読込 | JSON、`.luaskin` / `.lua`、LR2 CSV skinを読み込みます。[decode API](../crates/bmz-skin/src/lib.rs)、[app側decode](../crates/bmz-player/src/skin_loader/decode/document.rs)です。LR2全命令の完全互換を意味しない |
| 1件だけのoffset定義 | Lua / LR2の数値正規化で定義配列を保持します。LITONE5 Selectの読み込みと、後続画面へ渡すCONTINUE素材の取得失敗を修正しました。[記録](../notes/2026/2026-10-30-litone5-load-compatibility.md)、[回帰テスト](../crates/bmz-skin/src/lr2/tests/litone.rs) |
| LR2タグ付きCUSTOMFILE | `*\|1P\|`等を別項目として照合し、実パスからタグを除去します。フォルダーの既定・保存・Random選択と、同一ロード内の選択共有に対応します。LITONE5 AC9の異なる1P / 2P追加グラフを確認しました。[仕様](skin.md#lr2の条件分岐)、[回帰テスト](../crates/bmz-skin/src/lr2/tests/litone.rs) |
| LR2選曲のノーツ分布 | Selectの`NOTECHART`を既存の選択曲の分布描画へ接続します。LITONE5の2か所の読み込み、曲切替・解析待ちでの非表示を確認します。[仕様](skin.md#lr2のselect--result)、[変換テスト](../crates/bmz-skin/src/lr2/tests/select_drawing.rs)、[描画テスト](../crates/bmz-player/src/skin_loader/tests/lr2/select_drawing.rs) |
| LR2選曲バーのラベル | `BAR_LABEL`のLN / CN / HCN・地雷・ランダム分岐を描画します。コース内の特性集約、専用画像がない場合のLN画像への切り替えに対応します。[仕様](skin.md#lr2のselect--result)、[描画テスト](../crates/bmz-render/src/skin/tests/select/litone_drawing.rs) |
| LR2選曲バーのトロフィー | `BAR_TROPHY`の銅・銀・金をコースの達成情報と定義順に従って描画します。未所持コースでは非表示です。[仕様](skin.md#lr2のselect--result)、[回帰テスト](../crates/bmz-player/src/skin_loader/tests/lr2/select_drawing.rs) |
| LR2フォルダーのランプ分布 | `BAR_GRAPH`のtype 0を元画像の11セル単位のアニメーション、SRC timer、曲数の比率で描画します。type 1のスコアランク分布は未対応です。[仕様](skin.md#lr2のselect--result)、[描画テスト](../crates/bmz-render/src/skin/tests/select/litone_drawing.rs) |
| スキン / システム音セットの走査 | symlink / Windows junctionの循環と同一ディレクトリの重複走査を防止します。明示したリンクrootは受理し、候補パスとoriginを保持します。[仕様](skin.md#カタログとシステム音セットの走査)、[スキン回帰テスト](../crates/bmz-player/src/app/tests/skin.rs)、[システム音回帰テスト](../crates/bmz-player/src/system_sound.rs) |
| LR2の基準解像度 | `#RESOLUTION`省略時は全画面destinationから推定し、該当がなければ640×480（BMZ拡張）です。プリセットと幅・高さの明示指定、全スキン種別の`LR2 Resolution (BMZ)`による上書きも対応します。[仕様](skin.md#lr2の基準解像度)、[回帰テスト](../crates/bmz-skin/src/lr2/tests/cases_01.rs) |
| LR2の条件分岐 | 入れ子の`#IF`を各階層の条件だけで判定し、分岐のスタックをCSVごとに独立させるLR2の挙動に合わせます。Play以外にも`BGA Size`（op 30/31）を追加します。選曲のDP条件（10..12）、パネル条件（20..29）、コース作成条件（571 / 572）と`SRC_ONMOUSE`のパネル制限も評価します。[仕様](skin.md#lr2の条件分岐)、[回帰テスト](../crates/bmz-skin/src/lr2/tests/result_compat.rs)・[選曲表示](../crates/bmz-player/src/skin_loader/tests/lr2/select_conditions.rs) |
| LR2のnumber / option | beatorajaと意味が違うPOOR（空POOR込み）、スコア・ゲージ・ランク差、選曲のIR順位・人数・クリア率、選曲のクリアランプ・ランク、選曲のIR状態（600..608を単一の状態として排他的に評価）、Resultの新ランク等をLR2の定義で評価します。レベルバー・ゴースト進捗値等は未対応です。[仕様](skin.md#lr2のnumber--option)、[変換](../crates/bmz-skin/src/lr2/tests/refs.rs)・[評価](../crates/bmz-render/src/skin/tests/result/lr2.rs)の回帰テスト |
| LR2 Select / Result | 曲バー・基本設定をBMZ操作へ接続します。オプションbuttonはBMZ設定画面を開かず、LR2にある設定値をLR2の順で直接切り替える（左クリックのみです。BMZに同じ設定が無いbuttonは無反応）です。HS-FIXのMAIN BPMはbutton 55のAVERAGEスロットに対応し、text ref 74ではMAIN BPMと表示します。EFFECT (button 50 / 51, text 84 / 85)は1P / 2Pどちらを操作しても同じ共通設定を切り替え、どちらのtext refも共通値を表示します。ASSIST (button 44 / 45)はAUTO SCRATCH、BATTLE (button 56)はOFF / D-BATTLE / SP-TO-DP / G-BATTLEをSESSION MODE・DP OPTION・KEY CONVERSIONへ対応付けます（2人対戦BATTLEは非対応）。LR2スキン時はBMZ独自の下端オプション表示を重ねません。通常 / コースResultの自分側グラフ、左右反転、timer 150..152と入力待ち、難易度150..155・確定ランクによる素材選択に対応します。WMIXの難易度はアプリのResult読込状態から左右とANOTHER / INSANEを検証します。[回帰テスト](../crates/bmz-player/src/app/tests/result_difficulty.rs)です。特殊設定・相手履歴等の制限は[仕様](skin.md#lr2のselect--result)です。素材を使った[初期検証](../notes/2026/2026-10-03-lr2-select-result.md)と[追加スキンの修正・検証](../notes/2026/2026-10-05-lr2-select-result-review.md)を参照 |
| LR2のDST時刻 | 単一時刻の表示保持、終端を過ぎてからのloop、開始前と負loopの非表示をLR2に合わせます。[仕様](skin.md#lr2のdst時刻)、[回帰テスト](../crates/bmz-render/src/skin/tests/core/cases_05.rs) |
| 画像sourceの形式 | PNG / BMP / JPEG / GIF / TGA / WebP / CIMを読み込みます。2フレーム以上のanimated WebPはbeatorajaと同じ格子のスプライトシートにし、`divx` / `divy` / `cycle`で動かします。格子はbeatorajaと同じくGPUのテクスチャ上限（wgpuの`max_texture_dimension_2d`）で決めます。APNG / アニメGIFも同様にシート化するのはBMZ独自で、beatoraja / LR2は先頭フレームだけを表示します。[仕様](skin.md#image-sources)、[decode](../crates/bmz-render/src/assets/animated.rs)、[格子](../crates/bmz-render/src/assets/sprite_sheet.rs)、[skin loaderテスト](../crates/bmz-player/src/skin_loader/tests/animated_source.rs) |
| 旧DXA内のfont / image | 形式v1〜4の直接読込、既定キー・LZ圧縮に対応します。KCOOLの3種類の専用フォントを実アーカイブでdecode検証です。[仕様と制約](skin.md#lr2のdxaアセット)、[読込](../crates/bmz-skin-assets/src/lib.rs) |
| JSON document | numeric/string IDの正規化、include、property条件等です。[schema / loader](../crates/bmz-skin-document/src/lib.rs) |
| JSONの緩い構文 | libGDX互換で、コメント、カンマの省略・末尾カンマ、Shift_JISのファイル、`wrapping`の文字列真偽値を受け入れます。値の間のカンマ省略は未対応です。[仕様](skin.md#jsonの構文と文字コード)、[回帰テスト](../crates/bmz-skin-document/src/tests.rs) |
| BMS `#TEXT`（BMZ拡張） | チャンネル99の譜面テキストをtext 20100 / option 20101 / timer 20102、`#TEXT00`のミス表示をtext 20103 / option 20104 / timer 20105としてJSON・Lua・LR2 CSVの既存text部品へ公開します。beatoraja / LR2には無い機能で、既存ID・描画順は変更しません。[仕様](skin.md#bmz-chart-text-refs)、[skin loaderテスト](../crates/bmz-player/src/skin_loader/tests/chart_text.rs)、[BMS解析テスト](../crates/bmz-chart/src/import/tests/chart_text.rs) |
| Select DETAIL OPTIONS | 本体のexperimental設定ON（既定OFF）かつ`type: 5`、`bmzDetailOptions: 1`、`bmzDetailOptionsNumbers: true`のスキンで通常 / 詳細の2パネルを有効化します。E1短押し固定 / 長押し一時表示、E2押下で切替します。21項目、数値行と19320..19337の増減eventに対応します。19300/19400帯のsnapshot API、19500..20075の選択肢セルです。デフォルトselect.jsonは中央選択の環状7列＋補助2列、全選択肢を縦並びにします。`bmzDetailScroll`で描画とhitを補間します。未宣言・0・未知version・スキンなしは旧Assist表示・固定7鍵操作を維持し、新UIは重ねません。旧E2 option/timer、Assist固定eventの意味は維持します。[仕様](select-detail-options.md)、[描画・hitテスト](../crates/bmz-player/src/skin_loader/tests/detail_options.rs) |
| mz-select DETAIL OPTIONS | 既存パネル素材とバージョン/IR欄のbitmap fontを再利用し、環状7列＋補助2列を半透明で表示します。timer 22/32、300msの登場・退場に対応し、通常↔詳細の切り替えでは退出と登場を並行再生します。退場中は最後の表示データを保持し、退出側の操作だけを無効化します。パネルなしへ閉じる間は背後のクリックも遮断します。部品の正常ロード時だけ対応宣言し、旧optionpanel3/4のE2開閉表示を置換します。実験的な詳細オプションOFF・部品無効・欠落・失敗時とBMZ以外では旧Assist表示・固定鍵操作を使用します。[仕様](select-detail-options.md#mz-select)、[テスト](../crates/bmz-player/src/skin_loader/tests/mz_detail_options.rs) |
| mz-select G-BATTLE相手情報 | 左下に配置オプション・鍵盤順・CHART REPLICATIONの値を表示します。既存の半透明黒地・左白線・ビットマップ文字・オプション画像・リザルト鍵盤素材を流用します。相手名・項目ラベル・ゲージ名は出しません。相手一覧を開いている間だけ旧ランキング / rival data / ir data本文を隠します。NO DATA / LOADING / NOT FIXEDを区別してNO PLAYと同じ灰色にし、自分の設定で補完しません。[仕様](skin.md#g-battle一覧の相手情報)、[テスト](../crates/bmz-player/src/skin_loader/tests/mz_battle_info.rs) |
| Luxez-Flat G-BATTLE相手情報 | SPはRIVAL DATA欄、10K/14Kは左下のIR DATA / SCORE DETAIL欄へ表示します。DPの左右オプション・配置を上下に分け、右の既存RIVAL DATA / SCORE DATAを残します。既存素材・字体を流用し、鍵盤はResultと同じ25×35px・27px間隔です。未取得・読み込み中・可変配置はNO PLAYと同じ灰色にし、一覧を閉じると元の本文へ復帰します。[仕様](skin.md#g-battle一覧の相手情報)、[テスト](../crates/bmz-player/src/skin_loader/tests/luxe_battle_info.rs) |
| Luxez-Flat DETAIL OPTIONS | 紫の角丸枠・黒い値欄・オレンジのカーソルを再利用します。見出し等は既存bitmap font、値欄はNoto Sans CJK Mediumです。7可視列＋補助2列、半透明、300msの開閉と通常↔詳細の並行アニメーションです。正常ロード時のみ宣言し、旧E2のtimerなしhover領域も置換します。実験的な詳細オプションOFF・部品無効・欠落・失敗時とBMZ以外は旧Assistです。[仕様](select-detail-options.md#luxez-flat)、[テスト](../crates/bmz-player/src/skin_loader/tests/luxe_detail_options.rs) |
| ADFX02 / ECFN DETAIL OPTIONS | 別途導入する拡張済みECFNに対応します。既存option.pngの直線枠・黒い値欄・オレンジのカーソル、同梱源暎エムゴを使用します。7可視列＋補助2列と共通21項目です。200msフェードで通常↔詳細の退出・登場を並行再生し、暗幕は1枚です。正常ロード時のみ宣言し、実験的な詳細オプションOFF・部品欠落・失敗時とBMZ以外は3パネル方式です。本体同梱には追加しません。[仕様](select-detail-options.md#adfx02--ecfn)、[明示実行テスト](../crates/bmz-player/src/skin_loader/tests/ecfn_detail_options.rs) |
| ADFX02 / ECFN基本拡張 | 通常パネルの1P/2P配置を拡張ref 344/345による12種類へ拡張します。既存画像の文字・枠・加算カーソルを再利用し、元の領域に収めます。上部は`bmz_select_mode`で4K/6K/8Kを含むフィルター名、option 19168でFORCEを表示します。クリックevent 11/308とLN/CN/HCNの画像は維持します。experimental詳細パネルとは独立し、モジュール欠落・構築失敗・BMZ以外では元の表示を保持します。[記録](../notes/2026/2026-10-05-ecfn-bmz-extensions.md)、[明示実行テスト](../crates/bmz-player/src/skin_loader/tests/ecfn_extensions.rs) |
| ADFX02 / ECFN G-BATTLE相手情報 | グラフ下の空きにG-BATTLE DATA窓を追加し、配置オプション・鍵盤順・CHART REPLICATIONの値を3列で表示します。SCORE DATA窓の見出し帯・枠・縁の陰影を等倍で組み直し、文字は窓と同じ源暎エムゴ＋黒縁、区切りはPLAY DATAと同じ1px線です。鍵盤は同梱StarseekerのResult鍵盤を31×29px・28px間隔で流用します。相手名・項目ラベル・ゲージ名は出さず、NO DATA / LOADING / NOT FIXEDは数字の淡い0と同じ50%不透明度です。他のBMZ拡張と独立し、欠落・失敗時とBMZ以外は従来の画面です。本体同梱には追加しません。[仕様](skin.md#g-battle一覧の相手情報)、[明示実行テスト](../crates/bmz-player/src/skin_loader/tests/ecfn_battle_info.rs) |
| ADFX02 / ECFN・Starseeker BPM変化予告 | 別途導入する7Kプレイスキンに、拡張ref 19060〜19069の「変化前 » 変化後」をOFF / TYPE A〜Dで追加します。数字はPGREAT時の±ms画像を等倍・最大3桁、「»」は同梱源暎エムゴです。1Pはレーン右、2Pはレーン左へ寄せ、Starseekerではレーン外のfast/slow・スコア差分の上へずらします。BMZ以外・OFF・構築失敗では追加しません。本体同梱には追加しません。[記録](../notes/2026/2026-10-14-adfx02-bpm-preview.md)、[明示実行テスト](../crates/bmz-player/src/skin_loader/tests/adfx02_bpm_preview.rs) |
| 基本描画 | source、image/imageset、value/text、note/gauge/judge、slider、hiddenCover、destinationのtimer/op/draw、keyframe、UV animation等です。[描画評価](../crates/bmz-render/src/skin/document_render/) |
| SESSION MODEの画像とクリック | BMZ拡張`ref=1970`をimage / imagesetでも参照し、5モードを表示します。選曲画面の`act=-10006`は`click=0/1`の左右ボタン、`click=2/3`の左右・上下領域で前後へ循環し、プレイを開始しません。`click=3`は既存互換の上＝次、下＝前です。Lua custom eventの1970は組み込みイベントにしません。[仕様](skin.md#bmz-attempt-session-mode-ref)、[描画テスト](../crates/bmz-render/src/skin/tests/select/session_mode.rs)、[クリックテスト](../crates/bmz-player/src/app/tests/select/session_mode.rs)、[Luaテスト](../crates/bmz-player/src/skin_loader/tests/lua/session_mode.rs) |
| 選曲キーフィルター画像 | beatoraja互換の`ref=11`（4K/6K/8Kは0）を維持し、BMZ拡張`ref=19250` / `bmz_select_mode_index`で4K/6K/8Kを10/11/12として公開します。image・imageset ref、number、event_index、IntegerPropertyに対応します。8/9はbeatorajaのSINGLE/DOUBLE予約です。既存スキンは修正不要です。[仕様](skin.md#bmz-select-key-filter-index)、[描画テスト](../crates/bmz-render/src/skin/tests/select/mode_filter_index.rs)、[JSON / Luaローダーテスト](../crates/bmz-player/src/skin_loader/tests/select_mode_index.rs) |
| 名前付きプロパティ | 型付き`value`、destinationの`op` / `draw`、Luaの`main_state.number` / `float_number` / `text` / `option`を上流名から既存IDへ解決します。Boolean名の`!`否定と混在条件のANDを保持します。固定512名と番号付き名の解決に対応するが、未対応IDの意味実装は別です。[仕様](skin.md#名前によるプロパティ参照)、[対応表](../crates/bmz-skin-document/src/property_names/generated.rs)、[回帰テスト](../crates/bmz-skin/src/tests/property_names.rs) |
| LR2減算合成 | `blend=3`はsource alphaを掛けたRGBを描画先から減算し、描画先alphaを保持します。通常描画とAmbient内の描画に対応 |
| LR2スライダーのalpha | `blend=0`はDSTのalphaを無視し、素材の透過を維持します。WMIX HDの選曲スクロールバーを含みます。開始時刻・ループ・つまみ位置・ドラッグを確認します。[回帰テスト](../crates/bmz-render/src/skin/tests/select/lr2_slider.rs) |
| LR2 DST / SRC animation | 区間ごとの三次accとblend/filter/center、SRC/DSTが同じ時計の場合の画像cycle原点を保持します。数字はSRC時計で更新 |
| LR2 PlayのSRC/DSTと分岐 | 種類別のSRC保持、既定画像の条件付き置換、ロード時の通常 / AUTO等の分岐、複合ELSEの否定を保持します。[仕様](skin.md#lr2のplay描画)、[変換テスト](../crates/bmz-skin/src/lr2/tests/play_compat.rs) |
| LR2 wildcard include | CUSTOMFILE適用後に残るCSVファイル名の単一`*`から抽選します。解像度推定と本変換で選択を共有します。ディレクトリwildcard・DXA内CSVは対象外 |
| LR2数値のセル配置 | 10 / 11 / 24セル単位のcycle、6×4 / 4×6 / 24×1の符号付き素材、空き桁・揃え・桁あふれをLR2用に評価します。[描画テスト](../crates/bmz-render/src/skin/tests/play/lr2_compat.rs) |
| LR2 ResultのCLEAR TYPE・TARGET・DURATION | buttonのセルを横縦共通で選択し、更新後のランプ条件、MAXへの負の差、SPターゲットの最終EX、譜面・コース長を表示します。コース曲間でNoPlayに丸めたランプは、ゲージ種別button 40を描かずに表します。Course Resultの各ステージのレベル（number 250..259、option 580..589 / 700..745）とIR順位・人数に対応します。Playからの`IMAGE` / `LR2FONT`のCONTINUEとResult再読込・Course Resultへの引継ぎに対応します。[仕様](skin.md#lr2のselect--result)、[描画テスト](../crates/bmz-render/src/skin/tests/result/lr2_compat.rs) |
| LR2 ResultのCONTINUE補完 | BMZ独自です。PlayがLua / JSONまたは別テーマのLR2スキンなら、Resultと同じテーマのLR2 Playスキンの素材（自己ベストのランプ、ゲージ画像、LR2FONT）で引継ぎを補います。Decide / Play / Result / Course Resultの空の`CONTINUE`番号も、同じテーマのLR2スキン（Select・Decide・Result等）の素材で補います（W-MIX・ECSEのシャッター、3R / ECSEのDecideのレベル数字、RED_BELT Resultのクリアマーク）。別テーマ前提のOA_DX+は対象外です。[仕様](skin.md#lr2のselect--result)、[回帰テスト](../crates/bmz-player/src/app/tests/skin.rs) |
| LR2 Resultの入力段階 | グラフスキップ→ランク表示→更新演出（timer 152）→退出です。timer 152を使わないW-MIX / ECSE等とランク表示時間0のスキンは更新演出の入力を挟みません。[仕様](skin.md#lr2のselect--result)、[段階テスト](../crates/bmz-skin-document/src/lr2.rs) |
| LR2 Decideのスキップ | LR2と同じく、スキップはtimer 3を開始して`#CLOSE`（無ければ`#FADEOUT`）だけ待ちます。キャンセルと時間切れはtimer 2です。いずれもシーン時計を末尾へ進めません。[仕様](skin.md#lr2のselect--result) |
| Resultの更新option | `330..332` / `335`と同値の`1330..1332` / `1335`は、過去ベストが無い初回プレイでbeatorajaの空ScoreData（EX 0、combo 0、BP上限）と比較します。Result以外ではfalseです。[回帰テスト](../crates/bmz-render/src/skin/tests/result/lr2_compat.rs) |
| LR2ノーツ | `DST_NOTE`の全行・時計・条件・座標・大きさを評価し、`SRC_AUTO_*`を部分AUTO入力レーンへ適用します。全体AUTOPLAYは通常画像 |
| LR2横スクロール | `#HORIZONTAL`で通常ノーツ・LN・小節線を右から左へ移動し、LIFT等のoffsetをXへ適用します。LR2で縦方向の計算が残るLN/小節線はBMZ側で補完します。カバー消失ラインの横向きcropは未対応 |
| LR2画像透過色 | RGB画像の既定緑と`#TRANSCOLOR`（互換綴り2種を含む）を適用します。元からalphaを持つ画像は維持します。DXA内の画像も対象 |
| LR2 system font | `#FONT`の番号・宣言サイズに基づく幅制限とDST高さ、typeの縁取りを適用します。DxLib固有の太さ・AA方式は未対応 |
| LR2 2P数値 | Play ref=120..136を2Pの実スコア・コンボ・判定数・達成率・スコア差へ接続します。ref=120は確定値で、カウントアップ補間は未対応 |
| BGAの色・透明度 | destinationのRGBAをBase / Layer / Layer2 / POORのtintと乗算します。Ambientにも合成前に適用します。[core/resolve](../crates/bmz-render/src/skin/document_render/core/resolve.rs)、[回帰テスト](../crates/bmz-render/src/skin/tests/play/ambient.rs) |
| destination変換 | `center`、`offset` / `offsets`、`filter`の処理があります。[geometry](../crates/bmz-render/src/skin/geometry.rs)、[animation](../crates/bmz-render/src/skin/animation.rs)、[core評価](../crates/bmz-render/src/skin/document_render/core/)です。オブジェクトごとの適用経路は個別に確認する |
| stretch | static imageに加えslider・hiddenCover・judge・BGA等の適用経路があります。[graph/image](../crates/bmz-render/src/skin/document_render/graph/image.rs)、[judge](../crates/bmz-render/src/skin/document_render/play/judge.rs)、[core/resolve](../crates/bmz-render/src/skin/document_render/core/resolve.rs) |
| graph | type `101/102/110..115/140..149`の値解決と、Select / Result等のgraph描画があります。[graph値](../crates/bmz-render/src/skin/state_values/graph.rs)、[graph描画](../crates/bmz-render/src/skin/document_render/graph/) |
| songlistの`graph` | フォルダ行のランプ分布に加え、LR2oraja Endless Dreamと同様に譜面ダウンロード中の曲バーへ進捗を描きます。[描画](../crates/bmz-render/src/skin/document_render/select/graph.rs)、[仕様](chart-downloads.md#進捗と操作) |
| Playターゲットスコア | 数値ref `121/151`は最終値、`153`は進行値との差分です。グラフ`114`は進行値、`115`は最終値を最大EX SCOREに対する割合で返します。value / text / Luaの共通処理で解決します。[仕様](skin.md#play-target-score-refs)、[回帰テスト](../crates/bmz-render/src/skin/tests/graphs_more.rs) |
| Mine sprite | JSON / Luaの`note.mine`を使い、指定がない場合はデフォルトtextureへfallbackします。[note描画](../crates/bmz-render/src/skin/document_render/play/note.rs)、[plan](../crates/bmz-render/src/plan/play/document.rs) |
| Playの`judgegraph` | ノート種別・判定・FAST/SLOWの積み上げ、現在位置の白線、`delay`に対応します。判定は最短750 ms間隔で更新します。[仕様](skin.md#playのノート分布グラフ) |
| Playの`hiterrorvisualizer` | 内部画像の線幅・タイミング位置をdestinationへ拡縮し、FASTを左、SLOWを右に描きます。履歴の減衰・判定色・POOR透過・中央線・EMAの線／三角に対応します。EMAだけの設定も表示します。[仕様](skin.md#playのヒットエラービジュアライザー)、[描画テスト](../crates/bmz-render/src/skin/tests/hit_error.rs)、[実スキン・GPUテスト](../crates/bmz-player/src/skin_loader/tests/lua/hit_error.rs) |
| PMS見逃しノーツ | `note.dst2` / `DST_NOTE2`による判定ライン上の待機・落下・消滅と、処理済みノーツの即時非表示に対応します。キーモードによらず、`dst2`を持つスキンに適用します。[仕様](skin.md#playの見逃しノーツと判定領域)、[描画テスト](../crates/bmz-render/src/skin/tests/play/cases_04.rs)、[snapshotテスト](../crates/bmz-player/src/screens/play_snapshot/tests/cases_01.rs) |
| PeacefulPlayのKEY LOGGER | 押下ノーツ・判定数グラフ・NPS・チャタリング警告を、BMZの入力履歴で描きます。表示列はボタン点灯のkey-on timerから求め、7 to 9版のレーン割り当てに従います。[仕様](skin.md#peacefulplay互換のキーロガー)、[runtimeテスト](../crates/bmz-render/src/skin/tests/runtime/cases_01.rs)、[decodeテスト](../crates/bmz-skin/src/tests/cases_06.rs) |
| 判定領域 | judgeの`index`ごとの領域を、キーモード内のレーン番号から選びます。9KEYSの3領域はKEY1-3 / KEY4-6 / KEY7-9です。5K/7Kの対戦表示は通常スキン使用時も1P/2Pを分離し、通常DPの共通1領域・左右2領域は維持します。[仕様](skin.md#battle-session-skin)、[対戦・通常DPの回帰テスト](../crates/bmz-render/src/plan/tests/judge_regions.rs)、[レーン対応テスト](../crates/bmz-render/src/skin/tests/play/cases_06.rs) |
| RANDOM gauge animation | `type=0`のanimationをシーンごとのruntime stateで更新します。[GaugeAnimationRuntime](../crates/bmz-render/src/skin/runtime/gauge_animation.rs)です。parts・flashの一致確認は下記の未検証項目を参照 |
| text / font | align、overflow、wrapping、shadow/outline、TTF/OTF/TTC、bitmap font等です。beatorajaの`font.fallback`と欠字の代替候補（□ ▢ ☐ ■ ?）に対応します。文字atlas cacheを利用します。[仕様](skin.md#フォントのfallbackと欠字表示)、[text renderer](../crates/bmz-render/src/renderer/text/)、[GPU側cache利用](../crates/bmz-render/src/renderer/gpu/text.rs) |
| 文字の影 | `shadowOffset`だけの指定でも、beatorajaと同じく文字色の半分の明るさで描きます。[仕様](skin.md#text-opacity) |
| フォント解決 | 同梱フォントを優先し、OSフォントへfallbackします。スキンが指定したフォントに無い文字も、既定フォントとOSの記号・絵文字フォントで補います（BMZ拡張）。ビットマップフォント文中の補った文字は、ページ画像から推定した塗りと縁取りで描きます（BMZ拡張、[仕様](skin.md#ビットマップフォント文中のベクター文字の見た目)）。path / memory bytes / TTC indexを扱います。[bmz-font](../crates/bmz-font/src/system.rs) |
| Unicode・カラー絵文字 | BMZ拡張です。書記素クラスタ、ZWJ・国旗・肌色修飾、異体字セレクタ、COLR v0/v1・CBDT・sbixのカラーグリフを描きます。OpenType SVGは未対応です。[仕様](skin.md#カラー絵文字)、[テスト](../crates/bmz-render/src/renderer/tests/unicode_text.rs) |
| SCROLL / SPEED | SCROLLの区間積分と、SPEEDイベント間の線形補間を実装します。見かけ距離にはnote位置のSPEED倍率を適用します。[scroll.rs](../crates/bmz-player/src/screens/play_snapshot/scroll.rs)、[snapshotテスト](../crates/bmz-player/src/screens/play_snapshot/tests/cases_03.rs) |

destinationの`clip_x` / `clip_y` / `clip_w` / `clip_h`は、継承・補間・offsetを含めて
GPU scissorへ反映します。notes・songlist・judgeの外枠、Ambient合成後の制限にも対応します。
子部品と検索入力overlayの適用範囲は[Destinationのクリッピング](skin.md#destinationのクリッピング)を参照してください。

### Luaの推論とruntime

`auto`はfunctionを宣言的なref / expr / draw条件等へ変換できる場合に推論を使い、
推論不能な対応fieldを永続VMへ残します。`--lua-skin-runtime compat`は対応fieldを実行時に評価します。
closure/module stateを維持し、frameごとのmain_stateを参照します。

| field / API | 対応範囲 |
|---|---|
| Luaヘッダのproperty型変換 | `item[].op`の数値文字列・小数・非数値と、数値・booleanの`def`をbeatorajaの`toint()` / `tojstring()`に合わせます。SocialSkinのSelect / Result / Course Resultの一覧登録を確認します。[回帰テスト](../crates/bmz-skin/src/tests/lua_header_compat.rs) |
| destination `draw` | ロード時推論とruntime callbackです。booleanの`draw`はbeatorajaと同じく条件なしです。[仕様](skin.md#lua-runtime-compatibility-mode) |
| Lua文字列の`act` / `timer` | act script、関数・数値を返すtimer式を永続VMへ保持します。destinationの任意timer関数も描画時に評価し、クリックは最後の結果を参照します。[回帰テスト](../crates/bmz-player/src/skin_loader/tests/lua/events.rs) |
| Lua destinationの入れ子`op` | beatorajaと同じく入れ子table・booleanを条件から除外し、他の条件を維持します。ORには解釈しません。SocialSkinのSelectにある条件の合成fixtureと7K Playのロードを確認します。[回帰テスト](../crates/bmz-skin/src/tests/lua_document_conversion.rs) |
| 推論の順序 | tableのfieldを固定順で変換し、`pairs`順に依存しません。Luxe Flat Selectのランク画像とスコアレート帯を確認します。[回帰テスト](../crates/bmz-skin/src/tests/lua_inference_order.rs) |
| Resultのパネル状態 | `Expand_op` / callbackが直接保持する`result_mode`を現在のパネルへ同期します。Luxe FlatのGRAPHとIRの混在を防止します。通常 / compat、左右配置、WMIIの回帰確認は[作業記録](../notes/2026/2026-10-03-result-panel-lua-state.md) |
| value / text / graph / sliderの`value` | ロード時推論とruntime callback |
| Select / Result sliderの`event` | Lua FloatWriterへ0〜1のfloat位置を渡します。actとの入力順序、最後の描画状態によるhit判定とドラッグ対象の区別を維持します。SocialSkinのヘルプ操作に必要です。[回帰テスト](../crates/bmz-player/src/skin_loader/tests/lua/events.rs) |
| Lua数字の状態更新 | 数字のruntime valueをdestinationごとにdraw / timer判定前に1回評価です。cache hitでも実行を維持します。Luxe FlatのResult / Course ResultでMAX+0とランク境界を確認します。[回帰テスト](../crates/bmz-player/src/skin_loader/tests/lua/result_grade.rs) |
| `customTimers[].timer` | ID `10000..19999`です。推論とruntime callbackです。宣言順で1フレーム1回更新 |
| `main_state.set_timer` | 受動custom timerのON/OFFです。組み込みtimerや能動timerへの書き込みには制約がある |
| Luaのフレーム更新 | 入力 → custom timer → custom eventで`main_state`の設定を共有し、callback数に比例したLua関数生成を防ぎます。状態の復元と1フレーム1回の更新を維持します。[回帰テスト](../crates/bmz-skin/src/lua/tests/runtime_resources.rs) |
| `main_state.numbers` | ID / 名前を混在して複数戻り値で取得します。未知名は0、欠損sentinelは維持します。ロード時依存捕捉とruntime参照に対応 |
| Playのロード時の譜面情報 | 曲名・難易度・LN・BPM・判定ランク・難易度表を、変換後の譜面の値でロード時に渡します。[仕様](skin.md#playのロード時の譜面情報) |
| 譜面長のnumber `1163` / `1164` | Playは実分岐・余白・変換後、Practice区間適用前の全体長です。ロード時・callback・数値画像・`numberRef`・Offlineで共通化し、両方に`% 60`を適用します。欠損はLuaでINT_MIN、数値は非表示です。他画面の取得元は維持します。[境界値](../crates/bmz-render/src/plan/tests/cases_01.rs)、[先読み・Retry・Replay・次曲・Practice・Offline](../crates/bmz-player/src/app/tests/play_chart_length.rs) |
| 譜面ハッシュのtext `1030` / `1031` | Select / Decide / Play / Resultで元ファイルのMD5 / SHA256を小文字の16進数で返します。ロード時にも供給し、未所持譜面は既知のハッシュ、欠損は空文字です。[全画面テスト](../crates/bmz-render/src/plan/tests/chart_hashes.rs)、[名前・数値ID](../crates/bmz-render/src/skin/tests/property_names.rs) |
| `main_state.screen_width` / `screen_height` | 全sceneの物理出力pixel寸法です。未接続は0、exportは動画出力寸法です。ロード時に読むskinのcache・resize再decodeと、runtimeだけ読むskinの状態維持に対応します。[回帰テスト](../crates/bmz-player/src/skin_loader/tests/lua/screen_size.rs) |
| 数値ref `17..19` / `30..37` / `333`のロード時値 | Select / Decideのロード時にprofile全体の累計プレイ時間・プレイ数・判定累計・累計ノーツを渡します。mz-selectのプレイヤーランク（経験値・ランク・ゲージ）とプロフィール統計に対応します。プレイ後のSelect再入場で再decodeします。「今日の成果」・プレイ曲ログ・リセットは、BMZ対応版のmz-selectが日次統計ref `1930..1959`とevent `-10100`を参照します。[仕様](skin.md#select--decideのロード時のプレイヤー統計)、[回帰テスト](../crates/bmz-player/src/app/skin_loading/startup_tests.rs) |
| 数値ref `243..249` / `lastplay_*` | Selectの最終保存日時とResultの今回保存日時です。ローカル年月日時分秒です。欠損はLuaでINT_MIN、数字画像は非表示です。BEST日時helperとは分離します。保存条件と移行は[score-persistence.md](score-persistence.md) |
| `main_state.timer_is_on` / `timer_is_off` / `timer_elapsed` / `timer_elapsed_ms` / `timer_elapsed_seconds` | 組み込み / custom timerの現在状態・経過時間を取得します。経過時間はOFF時-1です。custom timerの同一frame更新、開始0・未来開始、64bit時計に対応します。[回帰テスト](../crates/bmz-skin/src/tests/read_helpers.rs) |
| `timer_util` | timer_function / timer_observe_boolean / new_passive_timer等をcustom timer内で利用可能。destinationの`timer`に指定した`timer_observe_boolean`は、Luaの変数を読む観測関数でもruntime VMで評価します（ModernChicのメニュー類）。[仕様](skin.md#lua-runtime-compatibility-mode) |
| `act` / `customEvents` / `main_state.event_exec` | functionの`act`をクリック時、`customEvents`を1フレーム1回（`condition`・`minInterval`）、`event_exec`でcustom event `1000..1999`を永続VMで実行します。クリックは全sceneで受け付け、最後に描画したフレームのLua `draw`とcustom timerで判定します。[仕様](skin.md#lua-actとcustomeventsの実行)、[runtimeテスト](../crates/bmz-skin/src/tests/lua_runtime_events.rs)、[app側テスト](../crates/bmz-player/src/skin_loader/tests/lua/events.rs) |
| `main_state.rate` / `rate_best` / `rate_rival` / `exscore_best` / `exscore_rival` | beatoraja / LR2oraja ED共通のAPIです。float_number 111 / 112 / 115、number 150 / 121と同じ評価経路を使います。[回帰テスト](../crates/bmz-skin/src/tests/lua_api_extensions.rs) |
| `main_state.gauge` | Play中の現在ゲージ、他sceneは0です。推論せずruntime callbackで評価します。[app側テスト](../crates/bmz-player/src/skin_loader/tests/lua/score_accessors.rs) |
| `main_state.audio_*` | ロード時は宣言的な音声命令、`act` / `customEvents`内では実行時に全sceneで鳴らします。音量の省略は1.0です。`audio_preload` / `audio_dispose`は無作用でtrueを返します。[app側テスト](../crates/bmz-player/src/skin_audio.rs) |
| `main_state.volume_*` / `set_volume_*` | runtime VMで現在のマスター / キー / BGM音量を読み書きします。書き込みはSelectの音量スライダーと同じくプロファイルへ保存します。[runtimeテスト](../crates/bmz-skin/src/tests/lua_runtime_events.rs) |
| `skin_config.enabled_options` | ヘッダのproperty順の選択opです。読んだopだけを文書の依存値に記録します |
| file / module解決 | Lua / JSON / LR2共通の`SkinPathContext`の許可rootに限定します。require / dofile / loadfile、include、source、font/page、audio、PMchara、DXA、wildcardを扱う。ライブラリ外の既存指定は[entryごとの再許可](skin.md#スキンのファイル参照と外部スキンの許可)が必要 |
| `filepath`で選択したフォルダ内の素材 | Lua / JSONの`source.path`と`font.path`で選択先に子素材がない場合は、beatorajaと同様に欠損として扱います。[仕様](skin.md#スキンのファイル参照と外部スキンの許可)、[回帰テスト](../crates/bmz-player/src/skin_loader/tests/paths.rs) |
| `os` / `io` | clock / date / time / difftime等の限定API、仮想ファイル読込です。`io`の書き込みはディスクへ書かずメモリに残し、同じ起動中は後のロードでも読み戻せます。`os.date`はLuaJの書式指定子に対応します。任意のOS操作を許可しない。[仕様](skin.md#メモリ上のスキンファイル)、[回帰テスト](../crates/bmz-skin/src/tests/lua_skin_files.rs) |
| `bit32` | LuaJ（Lua 5.2）のbit32です。[LuaJ実行結果との比較テスト](../crates/bmz-skin/src/tests/lua_stdlib_compat.rs) |
| `math.random`の小数引数 | LuaJと同じ整数変換と区間幅検査をロード・runtime両方へ適用します。SocialSkinのDecide / Resultで使う小数範囲に対応します。乱数列自体はLua 5.4です。[比較テスト](../crates/bmz-skin/src/tests/lua_stdlib_compat.rs) |
| API面の互換性固定 | 拡張前の名前・型・戻り値を[fixture](../crates/bmz-skin/src/tests/lua_api_baseline.rs)で固定し、インストール済みスキンは[比較ハーネス](../crates/bmz-skin/src/tests/installed_skin_compat.rs)で変更前後を比較します |

timerの単位・OFF値・書き込み制約・失敗時の値は[skin.md](skin.md)のLua Runtime Compatibility Modeを参照します。
実装入口は[function変換](../crates/bmz-skin/src/lua/conversion/function_field.rs)、
[runtime](../crates/bmz-skin/src/lua/runtime.rs)、[sandbox](../crates/bmz-skin/src/lua/sandbox/)です。
命令数・メモリ・table量の上限は互換修正でも維持します。

## 一部対応・未対応

| 項目 | 残っている範囲 |
|---|---|
| DXA | v5以降、独自キー、アーカイブ内wildcard列挙、動画・音声・Lua/CSV includeは未対応 |
| Selectの曲バーのLua `draw` | クリック判定では評価しません。行ごとに条件を評価するため、最後のフレームの結果を使えません |
| その他のLua function field | 上記以外のwriter等はcallbackの対象外です。対応追加時はfieldと呼び出し契約を個別に定義する |
| beatoraja最新版のLua API | `file_*`はメモリ上のスキンファイルで対応します（ModernChic Ver 4.71のローテーション等）。HTTP通信 / `open_url`は未対応です。非同期HTTPの`http_get_async` / `http_poll` / `http_cancel`はbeatorajaの通信無効時と同じ失敗値を返し、SocialSkinの無限待ちを防ぎます。実通信・ディスクへの書き込みは行いません。[Lua APIの互換範囲](skin.md#lua-apiの互換範囲)、[回帰テスト](../crates/bmz-skin/src/tests/lua_disabled_http.rs) |
| LuaJとLua 5.4の差 | 整数値floatの`tostring`、`string.format("%d")`の小数、`os.date`の既定書式と未知指定子は差が残ります。数値callbackの`nil`はBMZでは未取得、LuaJの`toint()` / `tofloat()`では0です。[仕様](skin.md#lua-apiの互換範囲) |
| function診断 | field path付きwarningやcustom timerのID付き診断はあります。object ID・Lua source file / lineの一貫した提示は改善余地がある |
| PMchara | `--`座標補間と右下pixel色によるchroma keyは未対応です。通常のalphaは対応します。[詳細仕様](skin.md) |

「未対応Lua timer」のような一括表現は避け、custom timerとdestination timer、
ロード時推論とruntime評価を分けて記載します。

## 未検証の互換差分

次の項目は、再現条件と比較結果が揃っていない確認候補です。
確定した不具合や未実装機能として扱わず、同一スキン・同一条件でbeatorajaと照合して判断します。

- `SkinGauge.prepare`相当のタイミングでの`parts`再計算と、モード切り替え時のボーダー位置です。
- Lua側の`P1_grooveflash`等、固定座標flashとBMZ側のgauge表示の一致です。
- destination変換やstretchの、上記で確認した描画経路以外への適用範囲です。

調査の入口は[gauge描画](../crates/bmz-render/src/skin/document_render/play/gauge.rs)と
`.local/beatoraja/`の対応クラスです。参照ソースや外部アセットがない場合は、未確認と明記します。

## 品質改善案

- text outline / shadowの品質改善です。現在のoutlineには周囲8方向へ描く近似があります。
  変更時は字形・サイズ・描画負荷を比較します。
- SDF / 距離場フォントは代替方式の検討候補です。現行のatlas cacheとは別の設計課題として扱います。
  `font.type`（1 / 2）は読みますが描画には使わず、通常の画像フォントとして描きます。
  ModernChic（画像フォント有効時のSelect / Decide等）とMILLIONDOLLARが該当します。

## 調査・検証の入口

- schema / decode: [bmz-skin-document](../crates/bmz-skin-document/src/)、[bmz-skin](../crates/bmz-skin/src/)
- 描画評価 / regression: [skin評価](../crates/bmz-render/src/skin/)、[skinテスト](../crates/bmz-render/src/skin/tests/)
- appの読込 / install: [skin_loader](../crates/bmz-player/src/skin_loader/)、[loaderテスト](../crates/bmz-player/src/skin_loader/tests/)

外部スキンの存在を条件にするテストは、素材が無いと早期returnする場合があります。
テスト成功件数だけで、そのスキンを検証したと判断しないでください。
検証コマンドの選択は[AGENTS.md](../AGENTS.md)に集約します。
