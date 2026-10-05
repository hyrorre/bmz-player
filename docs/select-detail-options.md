# 詳細プレイオプション / DETAIL OPTIONS

## 目的と設計判断

本体設定「選曲 > 実験的な詳細オプション」をONにした場合だけ、対応スキンで通常／詳細の2パネルを使用する。
既定はOFF。保存先はapp configの`[select].experimental_detail_options`で、profileとは独立する。
既存configのキー欠落もOFFとし、設定変更時に選曲スキンを再読込する。
優先順位は製品方針としてSUDDEN+、HIDDEN+、LIFT、GAS下限とする。
使用頻度の実測に基づくものではない。

2026-10-05の仕様変更で、ON時はE2 hold・E1+E2 holdによるパネル割当を廃止し、
E1で開閉、表示中のE2押下で通常／詳細を切り替える。旧詳細の6設定を新カタログへ統合する。
通常パネル内の設定・鍵盤割当は維持する。既存の`detail`関数は旧3パネル用として残し、
新カタログを`detail_options`、開閉の論理状態を`OptionPanelSession`とする。

OFF、未対応スキン、スキンなしでは従来の3パネルを使う。
E1 hold=通常、E2 hold=Assist、E1+E2 hold=旧詳細で、短押し固定表示は行わない。
本書で描画APIを説明する「E2」は互換panel 2（新詳細）の名前であり、ON時の物理holdではない。

基点: `main` / `753f81cbd2f3a8b1dd7c4ea08211c77b4edd7a84`。
作業ブランチ: `feat/select-detail-options`。

参考: [IIDX 26 詳細オプション](https://p.eagate.573.jp/game/2dx/26/howto/play/option_detail.html)。
項目移動と値変更の分離だけを参考にし、描画位置・画像等のスキン設定は追加しない。

## 項目カタログ

順序とIDは別管理。設定値は既存profile / settings registryを唯一の保存先にする。

|順|安定ID / 内部キー|分類|registry|値|スコープ|
|---|---|---|---|---|---|
|1|101 / sudden|LANE|SuddenEnabled|OFF / ON|キーモード|
|2|102 / hidden|LANE|HiddenEnabled|OFF / ON|キーモード|
|3|103 / lift|LANE|LiftEnabled|OFF / ON|キーモード|
|4|201 / gas-bottom|GAUGE|BottomShiftableGauge|ASSIST EASY / EASY / NORMAL|profile共通|
|5|202 / gas-mode|GAUGE|GaugeAutoShift|OFF / CONTINUE / HARD TO GROOVE / BEST CLEAR / SELECT TO UNDER|profile共通|
|6|301 / hs-auto|HI-SPEED|HispeedAutoAdjust|OFF / ON|キーモード|
|7|302 / hs-config|HI-SPEED|HispeedMode|NORMAL / CLASSIC / FLOATING / NORMAL+FLOATING / CLASSIC+FLOATING|キーモード|
|8|303 / constant|HI-SPEED|Constant|OFF / ON|キーモード|
|9|304 / green-number|HI-SPEED|TargetGreenNumber|1..6000、刻み1|キーモード|
|10|401 / ln-mode|LONG NOTE|LnModePolicy|AUTO(LN/CN/HCN) / FORCE(LN/CN/HCN)|profile共通|
|11|601 / bga|BGA|BgaMode|ON / AUTO / OFF|profile共通|
|12|701 / judge-auto|JUDGE|VisualOffsetAutoAdjust|OFF / ON|profile共通|
|13|702 / visual-offset|JUDGE|VisualOffsetMs|-500..500 ms、刻み1|キーモード|
|14|703 / judge-algorithm|JUDGE|JudgeAlgorithm|COMBO / DURATION / LOWEST|profile共通|
|15|501 / scroll-modifier|ASSIST / MODIFIER|AssistScrollMode|OFF / REMOVE|profile共通|
|16|502 / ln-modifier|ASSIST / MODIFIER|AssistLongNoteMode|OFF / REMOVE|profile共通|
|17|503 / mine-modifier|ASSIST / MODIFIER|AssistMineMode|OFF / REMOVE|profile共通|
|18|504 / expand-judge|ASSIST / MODIFIER|AssistExpandJudge|OFF / ON|profile共通|
|19|505 / judge-area|ASSIST / MODIFIER|AssistJudgeArea|OFF / ON|profile共通|
|20|506 / mark-note|ASSIST / MODIFIER|AssistMarkNote|OFF / ON|profile共通|
|21|507 / bpm-guide|ASSIST / MODIFIER|AssistBpmGuide|OFF / ON|profile共通|

ADD系は今回は新E2の選択候補に含めない。`assist.rs`ではSCROLL ADD / MINE ADDは
変換する一方、REMOVEと同じ実効Assist分類にはならず、LN ADDも追加結果に依存する。
UI追加に伴ってscore / replay / IRの分類を変えないための範囲限定であり、設定画面と
既存eventのADD選択は維持する。外部経路で設定されたADD値はそのまま表示し、
VALUE_INDEX=-1とする。新E2で変更した時だけOFF / REMOVEへ移る。

SUDDEN/HIDDENは`with_*_enabled`で他方を保持。LIFTも独立。OFF時も量は保存する。
量0でONはONと表示する。選択項目の補助情報で保存量を示す。
キーモードはプレイ開始と同じ変換後の設定対象を使い、BATTLEの描画用DPモードと区別する。
曲行は保存済みmode、コースはcommon_key_mode、他の行は明示的なmode filterから
解決する。ユーザーの追加指定により、未解決コースやALL状態のフォルダ等では
E2の編集対象を明示的に7Kとする。初期案の「未解決なら編集不可」を置き換える。
未解決時の7Kは直接の編集先なので7K→9K等の譜面変換を重ねない。
解決できた譜面・フィルターには従来通り変換を適用する。スコープ表示も編集先の7Kを示す。
`effective_play_key_mode`を共用し、Select側の既存モード同期にも同じ変換を適用する。
これにより7K→9K等でE2だけが別モードを編集する不整合を避ける。BATTLEはこの
変換関数に含まれないため、設定対象は元のサイドのモードを維持する。

GAS下限はOFFでも編集できる。runtimeの`BestClear` / `SelectToUnder`の下限として
利用する。`Off` / `Continue` / `HardToGroove`では非適用。方式も新詳細で編集する。OFF時の旧3パネルではE1+E2操作を維持する。
コースのCLASS系ゲージは通常ゲージより上のrankだけで推移するため、GAS下限は
非適用と表示する。Practiceでは開始前のGAS方式を基準に表示する。後続のPractice設定で
ゲージを変更する場合（旧AutoShiftからのBEST CLEAR移行を含む）は、そのruntime設定が優先する。
HS AUTO ADJUSTと緑数字はFLOATING対応HS CONFIGで有効。非適用でも次回用に編集・保持できる。CONSTANTはPracticeでは非適用。
その他のeffectiveは本体設定を利用する条件を表し、譜面上の対象ノーツの存在や
変換結果による最終Assist判定を推測しない。boolがOFFでもeditable/effectiveは独立する。
CONSTANTは表示時間による制御で、SCROLL REMOVEとは別。
LN MODEはLN解釈、LN MODIFIERは譜面改変で、混同しない。

## 入力と状態

ONかつ対応スキンでは、閉じた状態のE1押下で通常パネルを即時表示する。
200ms未満で離すと固定表示し、200ms以上保持して離すと閉じる。
固定中は次のE1押下で即座に閉じ、その解放で開き直さない。次回は通常パネルから開く。
開いている間のE2押下エッジで通常／詳細を切り替え、E2解放では閉じない。
閉じているときのE2だけの押下は無操作。開く前から保持したE2も切替を起こさない。
同一イベントでE1/E2が新規押下になった場合は開く→切替の順に処理する。
フォーカス喪失・モーダル・入力再同期で生じた解放は短押しとみなさず、固定表示を作らない。
物理holdはプレイなど他画面にも必要なので書き換えず、論理的な開閉状態を分離した。

詳細表示中のScratch Up/Downは前/次項目、奇数/偶数鍵は選択肢の次/前、数値の増加/減少。
独立したUI Left/Rightで前/次項目、選択式ではUI Up/Downで前/次の選択肢へ移る。
横並びの項目・縦並びの候補に軸を合わせるため、初期案から矢印の役割を交換した。
数値式は上で増加、下で減少する。選択式の縦配置は維持し、数値だけ増加方向を上へ合わせる。
表示する操作ガイドも選択中の値型で切り替える。マウスも利用できる。鍵盤との重複は鍵盤を優先し、
同じ入力で項目移動を重複実行しない。奇偶はプレイヤーサイド内で数える。
9Kでも独立UI入力とマウスで移動できる。

bool/enumは押下エッジだけ。値変更鍵は全解放するまで次の変更を受け付けない。
同方向の複数鍵も一変更、反対方向の追加押下は無視（最初の押下が優先）。
数値は最初の変更から400ms後に60ms間隔でリピートする。遅延フレームでまとめて増減しない。
最初の値変更キーの解放・項目移動・パネル切替で停止し、上限下限ではno-op。
OS側リピートは使わない。同梱3スキンの数値欄は現在値のみを表示し、＋／−ボタンやそのクリック領域を置かない。
判定表示オフセットの説明には、FASTが出る時は−方向、SLOWが出る時は＋方向へ調整する目安を表示する。
項目移動・パネル遷移時に押されている鍵は、全解放後に押し直す。
アナログは既存の感度、閾値、tick蓄積と`analog_ticks_per_scroll`を使い、
パネル遷移時は蓄積・選曲リピート・ドラッグを解除する。
フォーカス喪失・モーダル移行時も解除する。カーソルは選曲セッション中維持する。
項目は環状に配置し、常に中央slot 3が選択項目。先頭の左側には末尾の項目を表示する。
切替時は選曲と同じ低速/高速スクロール時間設定で1列分を線形補間する。
補間中の追加入力は高速時間を使い、残り変位を引き継いで±1列に制限する。
離れた列のクリックは最短方向へ1列分の遷移で選択先に移る。デフォルトの中央マーカーは固定し、列本体を動かす。
mz-selectでは記号を使わず、選択項目の列枠も列本体と共に移動して中央へ収まる。
パネル切替・閉鎖・フォーカス喪失で補間を解除し、カーソルだけ保持する。

## 設定変更と副作用

registryの変更関数を共用し、実際に変化した場合のみdirty・プリロード無効化を行う。
モード別設定を有効化・同期し、GAS下限等は選曲の一時値とprofileを同時更新する。
LN MODEは既存score context同期で一覧・replay・集計・ランキングと再利用cacheを更新する。
HS計算は既存profile/session経路を利用し、パネル専用計算式を追加しない。
保存は既存profile保存機構へ統合し、移動や描画で書き込まない。
詳細から通常へ切り替えるかパネルを閉じると、dirtyの場合だけ既存保存の共通本体`save_play_options_for_mode`で保存し、次曲開始・終了時の
既存保存にも含める。GAS方式・E1配置等の一時値を巻き戻さないよう、変更したGAS方式・下限だけ
選曲側へ同期する。registryのモード設定同期と、既存のプレイ開始時HS計算を利用する。
E2中の毎フレームのモード同期と閉鎖時の保存には同じ編集先を使う。
閉鎖時は7K fallbackを含む編集先を明示的に渡し、その後通常選曲の対象へ戻す。
これにより未解決時の7K設定とHS-FIXを別モードへ誤転記しない。保存失敗時はdirtyを残す。
Assist変更では変換済み再利用cacheも無効化し、旧固定Assistイベントも同じ扱いとする。
LN MODEの既存event 308とGAS下限event 341は新E2と同じ変更経路を通す。
音声再初期化・曲scanは行わない。LNのscore context変更に必要な一覧再読込は行う。

`select_detail_options.rs`のカタログは安定ID、分類、registry key、型、候補数、
スコープ、副作用分類を持ち、値表示・適用条件を同じadapterで解決する。
数値行もmin/max/step、clamp、内部リピートと保存を共用する。項目追加時はカタログと設定adapter、
i18nを追加すればよく、物理入力と7行スキンの変更は不要。

描画snapshotは読み取り専用`Arc`。閉じている間は生成せず、開いている間も値・対象・
カーソル・言語・保存量等の小さなcache keyが変化したときだけ翻訳・行文字列を生成する。
cache keyの項目数はカタログ長から決まり、追加項目の更新を取りこぼさない。
補間の残り変位はSelectSnapshotの独立したスカラーで渡し、アニメーション中も
ラベルや項目配列のキャッシュを再生成しない。閉鎖中は変位0。

## Skin API / 互換性

互換panel 1/2/3、option 21/22/23、timer 21..26 / 31..36の意味は維持する。
旧Assist event 301..307は従来の固定設定操作を維持する。
新パネルAPIには19300帯を使用する。1987はRULE MODE、19200帯はベスト配置履歴であり
再利用しない。負のruntime event、9000以降のdynamic timerとも別の名前空間で扱う。
採番時はschema/constants、number/text/option/event resolver、LR2 bridge定数、
`.local/beatoraja`のSkinPropertyを確認した。19300帯と19400帯に既存の同名前空間の
定義はない。beatoraja custom timer 10000..19999とは名前空間が異なり、新timerは設けない。
確定APIはnumber 19300..19312、text 19300..19309、option 19300..19305、
event 19300..19304と19310..19318、数値増減event 19320..19337、行refは19400..19489。
全フィールド・未取得値・値コードは[Skin API](skin.md#bmz-select-detail-options-v1)に定義する。

本体設定ONかつ`type: 5`、`bmzDetailOptions: 1`、`bmzDetailOptionsNumbers: true`を
宣言する選曲スキンで新詳細を有効にする。数値を描画・編集できない旧v1だけの宣言では有効にしない。
スキン読込時の本体予約option `bmz_detail_options`は文字列`"1"`/`"0"`。Luaは
`bmz.get_option("bmz_detail_options", "0")`で確認し、OFFでは旧Assist部品を残す。
ユーザーのスキンカスタマイズに保存せず、本体が最終値を上書きし、読込キャッシュの識別にも含める。
ON/OFF切替時は入力・固定表示を解除して選曲スキンだけを再ロードする。
未宣言・0・未知versionではスキン本来のAssistパネルと固定鍵操作を使用し、本体の新UIは重ねない。
スキンなしの場合も本体標準の従来のASSIST OPTIONS（7トグル）を使用する。
旧E2はKEY1から順にEXPAND JUDGE、CONSTANT（SCROLL REMOVE）、JUDGE AREA、LEGACY NOTE（LN REMOVE）、
MARK NOTE、BPM GUIDE、NO MINEを切り替える。旧名CONSTANTは表示時間制御とは別機能のまま。
キーボードとゲームパッドは既存の固定鍵対応を使い、event 301..307と同じ設定更新経路を通す。
スクラッチは新項目の移動に使わず、従来どおり旧E2内で消費する。
新パネルのsnapshot/ref/optionは未宣言時には非表示値となり、新eventは受け付けない。
対応スキンでは新E2表示中の旧event・背後の曲行クリックを遮断する。
描画は同じ読み取り専用snapshotを使い、7可視行と選択項目の情報を公開する。
デフォルト選曲スキンでは、この7スロットを横並びの列として描画する。
各列の下にその項目の全選択肢を縦並びで表示し、編集中の列は「▼」、設定中の値は「●」で
区別する。21項目のうち選択項目と前後3項目が横へスクロールする。最初の4項目の順序は維持する。
列見出しのクリックで項目選択、選択肢のクリックで直接設定する。現在と同じ値はno-opとし、
異なる値へ直接変更しても共通の設定変更処理と副作用は1回だけ通す。
destination拡張`bmzDetailScroll: [dx,dy]`で同じ補間を適用し、クリック判定も移動後の位置を使う。
デフォルトスキンは列の両端を不透明なマスクで覆い、その領域のクリックも遮断する。
移動中の端の空白を防ぐため、可視7枠に加えて枠外2枠を描画する。
slot 7は選択項目の4つ前、slot 8は4つ後で、静止時にはマスク外に収まる。
同じsnapshotとイベント経路を使い、見えている補助列もクリックできる。
候補外の既存ADD値は選択肢へ追加せず、現在値を列内に補足する。
選択肢ラベルはregistryのformatter / i18nから本体が供給する。スキン内にenum表は置かない。
追加APIは19500..20075の選択肢セル（8枠/項目、現在の最大はLN MODEの6個）。
補助列追加時に19470..19489、19948..20075、event 19317/19318を現行定数・resolver・
LR2変換・beatoraja SkinPropertyと照合した。同じ名前空間に既存定義はなく、
custom timer上限19999や負のruntime event -20000帯とも意味を共有しない。
新しいカタログ項目もこの枠を共用する。9個以上の候補を持つ項目を追加する場合は
セル容量と描画レイアウトの拡張が必要。既存19300/19400帯を使う縦リストの対応スキンも維持する。
実際のデフォルトは`data/skins/default/select.json`（1280×720）で、解像度別の別ファイルはない。
`_select_detail_{text,imageset,panel,destination}.json`をincludeし、全画面比率へ既存のcanvas
変換で拡大・letterboxする。全画面パネルと行専用eventで入力を分離する。
通常パネルの描画は維持し、旧詳細はexperimental OFF時に使用する。
その他の未対応スキンは従来のAssist表示・操作を使用する。

### mz-select

ユーザー指定により、同梱のBMZ拡張版mz-selectにも専用表示を追加した。
入口は`music_select.luaskin`→`load.lua`→`customize/advanced/enable.txt`で、
新部品`default_detailoptions/parts.lua`をshutter直前に配置する。
本体のカタログ・snapshot・イベント・翻訳を共用し、Luaには設定値や保存処理を持たせない。
新API番号や画像アセットは追加せず、既存のAssistパネル画像からボタン・シアン枠・発光カーソルを
切り出して描画する。画像に焼き込まれた値は黒い矩形で隠し、snapshotのラベルを重ねる。
選択肢は左下のバージョン・IR表示と同じ`m_select_system` bitmap fontを使用する。
元のfontにないLN MODEの括弧2文字だけは既存`m_select_profile_24`のglyphを補う
`choices.fnt`を用意し、英数字・記号のglyphと画像ページは元のfontと同一にする。
項目名・日本語の説明等は既存同梱mgenplusを使用する。

1920×1080の既存canvas幅を使って7列と枠外2列を配置し、1列274pxを`bmzDetailScroll`に渡す。
シアンの縁取り、暗い立体ボタン、白文字を既存デザインに合わせる。三角・丸のマーカーは使わない。
選択列は明るい枠と見出し背景、現在値は旧パネルの発光カーソルで区別する。
OFFも通常の値として示し、編集不可・非適用は状態テキストと説明で区別する。
選択肢外のADD設定は列内の補助値として保持・表示する。全8候補の描画枠を確保する。
背景は既存パネルと同じ最大alpha 168の暗幕と半透明の列・説明欄で、選曲画面を透かす。
全canvas幅に列を広げたため、旧実装の不透明な左右マスクは不要とする。
横長ウィンドウの余白へのはみ出しだけはcanvas外の黒マスクで隠し、canvas内の透過は維持する。
透明な全画面hit領域でクリックを消費し、背後の曲リスト・旧UIへの透過を防ぐ。
ユーザーの追加指定に合わせ、表示情報は一覧・カテゴリ・現在値・説明・保存量/GAS方式・理由に絞る。
タイトル、スコープ、件数、操作ガイド、ナビゲーションボタンは表示しない。
登場は旧Assistと同じtimer 22、300ms、acc 2で左から移動し、クリック領域も同じ座標で移動する。
枠外2列は登場完了後に描画して、右の補助列が開き始めに飛び出すのを防ぐ。
項目移動には本体の選曲スクロール補間を使う。詳細を閉じる／通常へ切り替える時は操作を即終了し、timer 32、300ms、acc 2で
左へ戻しながら暗幕を透明にする。退場には可視7列と説明欄を使い、枠外の補助列を新たに進入させない。
`bmzDetailOptionsClose=true`を宣言したスキンだけに、最後の項目snapshot（Arc）と列変位を描画用に保持する。
編集用snapshotとは分け、退場中のE2設定eventは受け付けない。
通常への切替でも詳細の退場を最後まで再生し、timer 32と21を並行させる。
逆方向もtimer 31による通常の退場と22による詳細の登場を並行させる。
切り替え先のパネルは通常どおり操作できる。パネルなしへ閉じる場合だけ、退場中のクリック・slider・ホイールを遮断する。
E2再オープン、フォーカス喪失、別モーダル・画面への遷移ではE2の退場表示を打ち切る。
既存timerを使って元パネルと同じ退場を小さく実装するため、退場の基準位置は開き終わりの位置とする。
登場途中に解放した場合も、元Assist同様にその基準位置から閉じる。

`load.lua`は部品の正常ロード後にのみ`bmzDetailOptions=1`と退場表示宣言を伝播する。
旧optionpanel3/4のE2専用destinationにはローカルの`bmzLegacyAssist`印を付け、
対応部品が有効なときだけ読み込み結果から除去する。開くtimer 22だけでなく閉じるtimer 32も
対象にして解放直後の旧パネル再表示を防ぐ。E1・E1+E2や旧Assistイベントの意味は変えない。
印はこのスキン内の構築用で、公開Skin APIではない。
部品が無効・欠落・ロード失敗の場合は宣言せず、元のAssist表示と固定鍵操作へ戻る。
BMZ以外では新部品は何も返さず、従来のAssist表示を維持する。

### Luxez-Flat

`select_skinparts/default_detailoptions/parts.lua`を`default_optionpanel`の後、シャッターの前に追加する。
項目・入力・保存は本体の既存21項目と変更経路を共用し、Luaはsnapshotの描画とeventの配置だけを担当する。
新規Skin APIやスキン独自の設定値は追加しない。先頭4項目の順序、7K fallback、GASの適用条件も共通。

7可視列＋補助2列を横に並べ、選択項目を中央へ循環・補間する。幅1920のcanvas内を使い、
`option2_panel_ver1.3.0.png`の紫の角丸枠と黒い値欄、`cursor_ver1.3.0.png`のオレンジの選択素材を切り出す。
枠の角は輪郭に沿う帯として切り出し、元画像の不透明な内側の塗りを含めない。
半透明の列背景も同じ丸みに合わせ、角と直線の境界に四角い塗りが残らないようにする。
角を引き伸ばさず、文字の焼き込み部分は切り出し範囲に含めない。
黒い値欄も、元画像の角丸ボタンだけを横帯に分けて切り出す。四隅と右端・下端の紫の背景を
描画対象から除き、拡大後の帯の境界を共通の座標で丸めて隙間や重なりを防ぐ。
既存画像を加工せず、背景・列・説明欄を半透明にして背後の選曲画面を見せる。
曲情報と同じ`font_sub`（26px）を見出し・説明へ、IRと同じ`font_sub_small`（18px）を状態・理由へ使用する。
候補と現在値は部品内に同梱する`NotoSansCJKjp-Medium.otf`（weight 500）で、
縁取りと影を付けずに描画する。候補の字形が枠の中央に収まるよう、フォントのメトリクスに合わせて配置する。
元のRegularより一段太い実フォントを使い、新しいweight APIは追加しない。
フォントの出典とライセンスは同じ`font/`ディレクトリに保持する。
値欄とオレンジの選択素材には同じ縦横倍率を適用し、元の19pxの発光余白を含む位置関係を維持する。
ラベル・値・カテゴリ・説明・理由・補助情報は本体の翻訳済み文字列をそのまま表示する。
列枠の明るさで選択中の項目を、オレンジのカーソルで現在値を区別し、非適用・編集不可は文字でも表示する。
ユーザー指定に合わせ、▼・●、パネルタイトル・操作ガイド・件数・ナビゲーションボタンは表示しない。

開閉は旧パネルと同じtimer 22/32、300ms、acc 2、x=-1920との往復を使う。
`bmzDetailOptionsClose=true`を宣言し、本体の保持snapshotで通常↔詳細の退出・登場を並行再生する。
登場中は描画とhitを同じ座標へ動かし、退場は可視7列と説明欄のみでhitを持たない。
透明な全画面hitと本体のevent制限で背後への入力を遮断する。
項目切り替えの`bmzDetailScroll`、再オープン・モーダル・フォーカス喪失の扱いはmz-selectと共通。

ローダーは新部品が成功した場合だけ対応宣言を伝播する。旧`default_optionpanel`は一部の画像IDを
E2とE1+E2で共用し、E2にはtimerを持たない`mouseRect`もある。そのため旧部品内で
timer 22/32またはoption ±22のdestinationだけに`bmzLegacyAssist`を付け、正常ロード後に除去する。
ID全体や他部品のdestinationを除去せず、旧3パネル用のtimer・操作経路も保持する。
新部品が無効・欠落・ロード失敗なら宣言せず、旧Assistの表示・固定7鍵・クリックを維持する。
BMZ以外では新部品がnilを返し、元のUIを維持する。
旧EXTRA MODEのADD候補を新E2へ追加しない判断も共通カタログに従う。設定画面・旧eventの経路は維持する。
検証結果とLuxez-Flatでの手動確認手順は[作業記録](../notes/2026/2026-10-03-luxe-flat-detail-options.md)を参照する。

## 検証方針

カタログ、循環・数値clamp、保存量、対象モード、GAS適用、入力エッジ・遷移、
snapshot/resolver、viewport、対応宣言・旧skinフォールバックをデバイス不要テストで検証する。
既存Assist変換・score・replay・IRテストを維持する。
日本語/英語、複数画面サイズ、実コントローラー、GPU表示は実施条件と未実施を記録する。

## 動作確認手順

1. 初期設定OFFでdefault、mz-select、Luxez-Flatを切り替え、従来のE1/E2/両holdと旧Assistクリックを確認する。
2. 本体設定の選曲ページで実験的な詳細オプションをONにする。再読込後、E1短押し（200ms未満）で通常が固定表示され、次のE1押下で閉じることを確認する。
3. E1を200ms以上保持して離すと閉じること、固定中／保持中どちらでもE2を押すたびに通常↔詳細が切り替わり、E2解放では変わらないことを確認する。
4. E2だけでは開かないこと、E2を保持してからE1を押しても通常から開くこと、同時押し・解放順・OSリピート・フォーカス喪失で暴発しないことを確認する。
5. スクラッチ／左右で21項目を一周し、選択式を奇数／下と偶数／上で変更する。SP/DPの奇偶、9Kの独立矢印、マウスも確認する。選択項目は中央へ循環・補間する。
6. 緑数字と表示オフセットで奇数鍵／上が増加、偶数鍵／下が減少することを確認する。400ms後から60ms間隔でリピートし、端で停止し、項目を移動すると止まること、＋／−ボタンと旧クリック領域がないことも確認する。判定表示オフセットにはFAST／SLOWに応じた調整方向を表示する。
7. 保存量を持つSUDDEN/HIDDEN/LIFTを独立にOFF/ONにする。GAS OFF中に下限を編集し、同じパネルでGAS方式を変える。BGA、判定自動調整、アルゴリズム、LN MODEも変更し、次のプレイ・再起動後に値が戻らないことを確認する。
8. キーモード未解決行では7K、解決済み行では譜面変換後のモードを編集し、他モードが変わらないことを確認する。スキン表示・選曲スコア・リプレイ対象も確認する。
9. mz-select/Luxez-Flatの通常↔詳細の両方向で退出と登場が並行再生されること、退出側・透明部分・閉鎖中へのクリックで背後が動かないことを確認する。
10. ONのまま未対応スキンへ変更すると従来操作になること、OFFへ戻すと3パネルへ復帰することを確認する。日本語／英語、960×540・1280×720・1024×768・1920×1080とOS表示スケールも確認する。

今回の検証結果・未実施項目は[2026-10-05作業記録](../notes/2026/2026-10-05-experimental-detail-options.md)を参照。
過去の表示検証は[初期実装](../notes/2026/2026-10-01-select-detail-options.md)、
[mz-select](../notes/2026/2026-10-02-mz-select-detail-options.md)、
[Luxez-Flat](../notes/2026/2026-10-03-luxe-flat-detail-options.md)に残す。
