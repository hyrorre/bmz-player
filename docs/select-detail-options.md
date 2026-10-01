# 詳細プレイオプション / DETAIL OPTIONS

## 目的と設計判断

E2 holdで本体共通のプレイ直前設定を編集する。優先順位は製品方針として
SUDDEN+、HIDDEN+、LIFT、GAS下限とする。使用頻度の実測に基づくものではない。
E1とE1+E2の操作は維持する。既存コードの`detail`はE1+E2を指すため、
新E2のコードは`detail_options`と呼ぶ。

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
未解決コース等ではモード別項目のみ編集不可とし、profile共通項目は編集できる。
曲行は保存済みmode、コースはcommon_key_mode、他の行は明示的なmode filterから
解決する。ALL状態のフォルダなどで7Kを推測して書き込まない。
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
独立したUI Up/Down、Left/Rightとマウスを利用できる。鍵盤との重複は鍵盤を優先し、
同じ入力で項目移動を重複実行しない。奇偶はプレイヤーサイド内で数える。
9Kでも独立UI入力とマウスで移動できる。

bool/enumは押下エッジだけ。値変更鍵は全解放するまで次の変更を受け付けない。
同方向の複数鍵も一変更、反対方向の追加押下は無視（最初の押下が優先）。
項目移動・パネル遷移時に押されている鍵は、全解放後に押し直す。
アナログは既存の感度、閾値、tick蓄積と`analog_ticks_per_scroll`を使い、
パネル遷移時は蓄積・選曲リピート・ドラッグを解除する。
フォーカス喪失・モーダル移行時も解除する。カーソルは選曲セッション中維持する。

## 設定変更と副作用

registryの変更関数を共用し、実際に変化した場合のみdirty・プリロード無効化を行う。
モード別設定を有効化・同期し、GAS下限等は選曲の一時値とprofileを同時更新する。
LN MODEは既存score context同期で一覧・replay・集計・ランキングと再利用cacheを更新する。
HS計算は既存profile/session経路を利用し、パネル専用計算式を追加しない。
保存は既存profile保存機構へ統合し、移動や描画で書き込まない。
E2を離れるとdirtyの場合だけ`save_current_play_options`で保存し、次曲開始・終了時の
既存保存にも含める。GAS方式・E1配置等の一時値を巻き戻さないよう、変更したGAS下限だけ
選曲側へ同期する。registryのモード設定同期と、既存のプレイ開始時HS計算を利用する。
対象mode未解決時は`save_profile_config`で共通設定の変更を保存し、通常保存の7K fallbackと
HS-FIX転記を通さない。無関係なモード設定へ書き込まないための分岐である。
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

## Skin API / 互換性

互換panel 1/2/3、option 21/22/23、timer 21..26 / 31..36の意味は維持する。
旧Assist event 301..307は従来の固定設定操作を維持する。
新パネルAPIには19300帯を使用する。1987はRULE MODE、19200帯はベスト配置履歴であり
再利用しない。負のruntime event、9000以降のdynamic timerとも別の名前空間で扱う。
採番時はschema/constants、number/text/option/event resolver、LR2 bridge定数、
`.local/beatoraja`のSkinPropertyを確認した。19300帯と19400帯に既存の同名前空間の
定義はない。beatoraja custom timer 10000..19999とは名前空間が異なり、新timerは設けない。
確定APIはnumber 19300..19312、text 19300..19309、option 19300..19305、
event 19300..19304と19310..19316、行refは19400..19469。
全フィールド・未取得値・値コードは[Skin API](skin.md#bmz-select-detail-options-v1)に定義する。

`bmzDetailOptions: 1`対応宣言のある選曲スキンだけが自前表示を使用する。未対応スキンとスキンなしは
本体標準オーバーレイを使う。物理入力は共通。フォールバックは不透明背景で旧E2を
覆い、クリックを専有する。対応スキンでも新E2表示中の旧event・背後の曲行クリックを遮断する。
描画は同じ読み取り専用snapshotを使い、7可視行と選択項目の情報を公開する。
実際のデフォルトは`data/skins/default/select.json`（1280×720）で、解像度別の別ファイルはない。
`_select_detail_{text,imageset,panel,destination}.json`をincludeし、全画面比率へ既存のcanvas
変換で拡大・letterboxする。全画面パネルと行専用eventで入力を分離し、E1とE1+E2は維持する。
第三者製submoduleスキンは書き換えない。

## 検証方針

カタログ、循環・数値clamp、保存量、対象モード、GAS適用、入力エッジ・遷移、
snapshot/resolver、viewport、対応宣言・旧skinフォールバックをデバイス不要テストで検証する。
既存Assist変換・score・replay・IRテストを維持する。
日本語/英語、複数画面サイズ、実コントローラー、GPU表示は実施条件と未実施を記録する。

## 動作確認手順

1. `cargo run -p bmz-player --locked`で起動し、デフォルト選曲スキンを選び、7Kの曲行でE2を保持する。
2. スクラッチ上下と独立矢印で15項目を一周する。奇数鍵／右、偶数鍵／左で変更し、
   背後の曲選択・曲開始が起きないことを確認する。9Kでは独立矢印とマウスを使う。
3. SUDDEN/HIDDEN/LIFTの保存量を設定画面で用意し、独立にOFF/ONを切り替える。
   E2を離れて開き直し、プレイ開始・アプリ再起動後も設定と量が保持されることを確認する。
4. GAS OFFで下限を変え、E1+E2のKEY2でBEST CLEAR／SELECT TO UNDERにして次曲へ渡す。
   LN MODEを変えたときは選曲スコア・ランキング・リプレイ対象が更新されることを確認する。
5. 2Pの奇偶、同方向・反対方向同時押し、鍵を保持しての項目移動、E2→両押し→E2、
   それぞれの解放順序を試す。値変更は全鍵解放後の押し直しを必要とする。
6. E2中にF1、ウィンドウ切替、修飾キー解放を行い、復帰時に値や背後の選曲が暴発しないことを確認する。
7. 未対応選曲スキンで本体表示とクリック専有を確認する。E1・E1+E2へ移り、従来操作も確認する。
8. 日本語／英語、960×540・1280×720・1024×768・1920×1080とOS表示スケールで行・説明・ガイドを確認する。

実行した自動検証と実機確認の区別は[作業記録](../notes/2026/2026-10-01-select-detail-options.md)に残す。
