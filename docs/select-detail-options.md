# 詳細プレイオプション / DETAIL OPTIONS

## 目的と設計判断

E2 holdで本体共通のプレイ直前設定を編集する。優先順位は製品方針として
SUDDEN+、HIDDEN+、LIFT、GAS下限とする。使用頻度の実測に基づくものではない。
E1とE1+E2の操作は維持する。既存コードの`detail`はE1+E2を指すため、
新E2のコードは`detail_options`と呼ぶ。

2026-10-03の追加指定により、新E2の表示と入力は対応宣言のあるスキンだけで有効にする。
未宣言スキンを本体の新UIで覆う初期方針と、全スキンで物理操作を統一する初期方針は取り消す。
既存スキンの表示と固定鍵の意味を揃えるため、未宣言時は従来のbeatoraja式Assist操作を維持する。

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
|5|301 / hs-auto|HI-SPEED|HispeedAutoAdjust|OFF / ON|キーモード|
|6|302 / hs-config|HI-SPEED|HispeedMode|NORMAL / CLASSIC / FLOATING / NORMAL+FLOATING / CLASSIC+FLOATING|キーモード|
|7|303 / constant|HI-SPEED|Constant|OFF / ON|キーモード|
|8|401 / ln-mode|LONG NOTE|LnModePolicy|AUTO(LN/CN/HCN) / FORCE(LN/CN/HCN)|profile共通|
|9|501 / scroll-modifier|ASSIST / MODIFIER|AssistScrollMode|OFF / REMOVE|profile共通|
|10|502 / ln-modifier|ASSIST / MODIFIER|AssistLongNoteMode|OFF / REMOVE|profile共通|
|11|503 / mine-modifier|ASSIST / MODIFIER|AssistMineMode|OFF / REMOVE|profile共通|
|12|504 / expand-judge|ASSIST / MODIFIER|AssistExpandJudge|OFF / ON|profile共通|
|13|505 / judge-area|ASSIST / MODIFIER|AssistJudgeArea|OFF / ON|profile共通|
|14|506 / mark-note|ASSIST / MODIFIER|AssistMarkNote|OFF / ON|profile共通|
|15|507 / bpm-guide|ASSIST / MODIFIER|AssistBpmGuide|OFF / ON|profile共通|

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
利用する。`Off` / `Continue` / `HardToGroove`では非適用。方式自体はE1+E2に残す。
コースのCLASS系ゲージは通常ゲージより上のrankだけで推移するため、GAS下限は
非適用と表示する。Practiceでは開始前のGAS方式を基準に表示する。後続のPractice設定で
ゲージを変更する場合（旧AutoShiftからのBEST CLEAR移行を含む）は、そのruntime設定が優先する。
HS AUTO ADJUSTはFLOATING対応HS CONFIGで有効。CONSTANTはPracticeでは非適用。
その他のeffectiveは本体設定を利用する条件を表し、譜面上の対象ノーツの存在や
変換結果による最終Assist判定を推測しない。boolがOFFでもeditable/effectiveは独立する。
CONSTANTは表示時間による制御で、SCROLL REMOVEとは別。
LN MODEはLN解釈、LN MODIFIERは譜面改変で、混同しない。

## 入力と状態

E2+Scratch Up/Downは前/次項目、奇数/偶数鍵は値の次/前。
独立したUI Left/Rightで前/次項目、UI Up/Downで前/次の選択肢へ移る。
横並びの項目・縦並びの候補に軸を合わせるため、初期案から矢印の役割を交換した。
上は前（数値なら減少）、下は次（数値なら増加）。マウスも利用できる。鍵盤との重複は鍵盤を優先し、
同じ入力で項目移動を重複実行しない。奇偶はプレイヤーサイド内で数える。
9Kでも独立UI入力とマウスで移動できる。

bool/enumは押下エッジだけ。値変更鍵は全解放するまで次の変更を受け付けない。
同方向の複数鍵も一変更、反対方向の追加押下は無視（最初の押下が優先）。
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
E2を離れるとdirtyの場合だけ既存保存の共通本体`save_play_options_for_mode`で保存し、次曲開始・終了時の
既存保存にも含める。GAS方式・E1配置等の一時値を巻き戻さないよう、変更したGAS下限だけ
選曲側へ同期する。registryのモード設定同期と、既存のプレイ開始時HS計算を利用する。
E2中の毎フレームのモード同期と閉鎖時の保存には同じ編集先を使う。
閉鎖時は7K fallbackを含む編集先を明示的に渡し、その後通常選曲の対象へ戻す。
これにより未解決時の7K設定とHS-FIXを別モードへ誤転記しない。保存失敗時はdirtyを残す。
Assist変更では変換済み再利用cacheも無効化し、旧固定Assistイベントも同じ扱いとする。
LN MODEの既存event 308とGAS下限event 341は新E2と同じ変更経路を通す。
音声再初期化・曲scanは行わない。LNのscore context変更に必要な一覧再読込は行う。

`select_detail_options.rs`のカタログは安定ID、分類、registry key、型、候補数、
スコープ、副作用分類を持ち、値表示・適用条件を同じadapterで解決する。
初期構成はbool/enumのみ。数値のmin/max/stepとclamp契約は型・resolver・テストで
用意するが、数値行のリピートは今回追加しない。項目追加時はカタログと設定adapter、
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
event 19300..19304と19310..19318、行refは19400..19489。
全フィールド・未取得値・値コードは[Skin API](skin.md#bmz-select-detail-options-v1)に定義する。

`type: 5`かつ`bmzDetailOptions: 1`の選曲スキンだけで新E2の表示と入力を有効にする。
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
区別する。15項目のうち選択項目と前後3項目が横へスクロールする。最初の4項目の順序は維持する。
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
変換で拡大・letterboxする。全画面パネルと行専用eventで入力を分離し、E1とE1+E2は維持する。
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
項目移動には本体の選曲スクロール補間を使う。E2解放時は操作を即終了し、timer 32、300ms、acc 2で
左へ戻しながら暗幕を透明にする。退場には可視7列と説明欄を使い、枠外の補助列を新たに進入させない。
`bmzDetailOptionsClose=true`を宣言したスキンだけに、最後の項目snapshot（Arc）と列変位を描画用に保持する。
編集用snapshotとは分け、退場中のE2設定eventは受け付けない。
E1/E1+E2への切替でもE2の退場を最後まで再生し、切り替え先の登場と並行させる。
逆方向も既存のtimer 31/33による退場とE2のtimer 22による登場を並行させる。
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
項目・入力・保存は本体の既存15項目と変更経路を共用し、Luaはsnapshotの描画とeventの配置だけを担当する。
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
`bmzDetailOptionsClose=true`を宣言し、本体の保持snapshotでE2↔E1+E2の退出・登場を並行再生する。
登場中は描画とhitを同じ座標へ動かし、退場は可視7列と説明欄のみでhitを持たない。
透明な全画面hitと本体のevent制限で背後への入力を遮断する。
項目切り替えの`bmzDetailScroll`、再オープン・モーダル・フォーカス喪失の扱いはmz-selectと共通。

ローダーは新部品が成功した場合だけ対応宣言を伝播する。旧`default_optionpanel`は一部の画像IDを
E2とE1+E2で共用し、E2にはtimerを持たない`mouseRect`もある。そのため旧部品内で
timer 22/32またはoption ±22のdestinationだけに`bmzLegacyAssist`を付け、正常ロード後に除去する。
ID全体や他部品のdestinationを除去せず、E1/E1+E2のtimer・操作を維持する。
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

1. `cargo run -p bmz-player --locked`で起動し、デフォルト選曲スキンを選び、7Kの曲行でE2を保持する。
2. スクラッチ上下と独立矢印で横並びの15項目を一周する。各列の全選択肢と「●」の値が一致すること、
   選択肢クリックでその値に変更できることを確認する。奇数鍵／下、偶数鍵／上でも変更し、
   背後の曲選択・曲開始が起きないことを確認する。9Kでは独立矢印とマウスを使う。
   左右で先頭と末尾を往復し、選択列が中央へ補間され、移動中のクリックが見えている候補へ届くことを確認する。
3. SUDDEN/HIDDEN/LIFTの保存量を設定画面で用意し、独立にOFF/ONを切り替える。
   E2を離れて開き直し、プレイ開始・アプリ再起動後も設定と量が保持されることを確認する。
4. GAS OFFで下限を変え、E1+E2のKEY2でBEST CLEAR／SELECT TO UNDERにして次曲へ渡す。
   LN MODEを変えたときは選曲スコア・ランキング・リプレイ対象が更新されることを確認する。
5. 2Pの奇偶、同方向・反対方向同時押し、鍵を保持しての項目移動、E2→両押し→E2、
   それぞれの解放順序を試す。値変更は全鍵解放後の押し直しを必要とする。
6. E2中にF1、ウィンドウ切替、修飾キー解放を行い、復帰時に値や背後の選曲が暴発しないことを確認する。
7. 未対応選曲スキンで元のAssistパネルと旧クリック領域が使えることを確認する。
   KEY1〜7が各Assistを直接切り替え、新UIが重ならないことを確認する。E1・E1+E2も従来どおり操作する。
8. 日本語／英語、960×540・1280×720・1024×768・1920×1080とOS表示スケールで行・説明・ガイドを確認する。
9. ALLのフォルダやキーモード未解決コースでE2を開き、7Kと表示されること、7Kの設定が変更できることを確認する。
   7K→9K変換ONでも未解決時は7Kを編集し、再起動後も7Kへ保存されることを確認する。
10. mz-selectを選択し、同じ操作を試す。中央の列枠と現在値の発光、非適用の文字表示を確認する。
    左からの登場中も見えているボタンをクリックでき、透明部分では背後の曲が反応しないことを確認する。
    E2を離すと文字・値を保持した7列と説明欄が300msで左へ戻り、暗幕も消えることを確認する。
    退場中のクリック・ホイールで設定や背後の選曲が動かず、終了後は通常操作できることを確認する。
    E2→E1+E2とE1+E2→E2で退出・登場が両方動き、退出側の値をクリック変更できないことを確認する。
    E2→E1+E2→E1→閉じると素早く解放し、E2の退場が再起動せずに終わることも確認する。
    E2の再オープン・F1・フォーカス喪失では古いE2退場表示が残らないことを確認する。
    E2からE1+E2へ移る・解放順序を変える操作で、旧Assist表示が現れないことを確認する。
    `enable.txt`の`default_detailoptions/parts.lua`を一時的に外してスキンを再読込すると、
    元のAssist表示・固定鍵操作へ戻ることを確認し、確認後にその行を戻す。

実行した自動検証と実機確認の区別は[作業記録](../notes/2026/2026-10-01-select-detail-options.md)に残す。
mz-select追加分は[専用記録](../notes/2026/2026-10-02-mz-select-detail-options.md)を参照。
