# スキン仕様

BMZはbeatoraja JSON / Lua skinの互換を基本にします。既存beatoraja skin typeは
そのまま扱い、BMZ独自のkey modeだけ拡張skin typeを予約します。

JSON / Lua / LR2 skinの対応範囲と残課題は[skin-compatibility.md](skin-compatibility.md)を参照してください。
この文書ではIDや各機能の動作契約を管理します。

## 画像の拡大縮小

画像destinationの`filter=0`（既定）は最近傍、非0は線形補間を使います。
最近傍では切り出し範囲のpixel境界を保持し、整数倍表示でも各画素を同じ幅で描きます。
線形補間では、隣の素材が混ざらないよう切り出し範囲の内側に半画素の余白を設けます。
通常画像・回転画像に共通です。

## Destinationのクリッピング

JSON / Luaのdestinationの各`dst`に`clip_x` / `clip_y` / `clip_w` / `clip_h`を指定すると、
そのオブジェクトの描画を軸に平行な矩形内へ制限します。座標は通常のdestinationと同じ
スキンpixel・左下原点で、オブジェクトの`x` / `y`からの相対座標ではありません。

- 省略した各値は前の有効なキーフレームから継承します。4値が揃う前はclipしません。
- clipはdestinationと同じtimer・loop・accで補間します。補間途中の小数は保持します。
  開始フレームにclipがなければ、次フレームのclipへ向けた補間はしません。
- offsetの移動と幅・高さの変更をclipにも適用します。relativeオブジェクトでは移動を省きます。
  `angle`でclip自体は回転しないが、Playの全体移動・拡縮には追従します。
- offset適用後の幅または高さが0以下ならclipを無効にします。正のclipが画面外にある場合や、
  親clipとの交差が空の場合は描画されません。
- 画像・数字・文字・グラフ・ゲージ等へ共通に適用します。Ambientとキャッシュ済みグラフは
  合成後の出力をclipするため、clip外の元画像もぼかし計算に含まれます。
- `notes` destinationはノーツ・LN・Mine・小節線・BPM/STOP線とラベルをまとめてclipします。
  songlistとjudgeも外側destinationで全体をclipします。上流が子部品を直接描画する経路では、
  小節線や選曲バー・判定画像・コンボ数字の子destination固有clipは適用しません。
  入力中の検索テキストとcaretは別の入力overlayであり、skinのclip対象外です。
- scissorは実際の描画targetとletterboxに合わせて変換します。下原点のx/y/幅/高さを
  beatorajaと同じ四捨五入（負のちょうど半分は正方向）でpixel化してから上原点へ変換します。
  オブジェクト描画後は元のclipへ戻します。

## Ambient背景（BMZ拡張）

JSON / LuaのimageまたはBGAのdestinationに`ambient: true`を指定すると、
そのdestinationの表示内容を低解像度に合成し、GPUで縦横にぼかして描画します。
`filter`（拡大縮小時の補間）とは別の指定で、beatoraja互換仕様には含めません。

| destinationのキー | 値・既定値 | 動作 |
| --- | --- | --- |
| `ambient` | bool、既定`false` | Ambientを有効にする |
| `ambientMode` | `"full"` / `"spread"`、既定`"full"` | 指定したdestination全体 / 実際の映像範囲から拡大 |
| `ambientSpread` | 0〜200、既定20 | Spreadで幅・高さを拡大する割合（％） |
| `ambientBlur` | 0〜100、既定50 | ぼかしの強さです（％）。0は縮小・ぼかしを省略 |

旧`bmzAmbient`は`ambient`の読み込み用aliasとして受け付けます。数値の範囲外は上限・下限へ丸めます。

```lua
-- Full: 全画面のdestinationを用意する
{id = "bga", ambient = true, ambientMode = "full", ambientBlur = 50, stretch = 3,
 dst = {{x = 0, y = 0, w = 1920, h = 1080, a = 210}}}

-- Spread: 通常のBGAと同じ矩形・stretchを指定する
{id = "bga", ambient = true, ambientMode = "spread",
 ambientSpread = 20, ambientBlur = 50, stretch = 1,
 dst = {{x = 1100, y = 284, w = 512, h = 512, a = 210}}}
```

- Fullはdestinationの範囲をぼかします。全画面表示には全画面の矩形を指定します。
- Spreadはstretch適用後の映像範囲を基準に、中心を維持して幅・高さを`1 + ambientSpread / 100`倍します。
  20％なら各辺に元サイズの10％ずつ広がり、letterboxの余白は含みません。
  複数のBGAレイヤーは表示矩形の和集合を基準に、同じ中心・倍率で拡大します。
  拡大後の外周にもぼかしの余白を設けて透明へ減衰させ、画面外はクリップします。
- ぼかし50％は基準の強さ、100％は約2倍のぼかし幅です。0％でもSpreadによる拡大は適用します。
  ぼかし幅は拡大後の映像範囲（Fullではdestination）の長辺に比例し、縦横とも同じ画素幅になります。
- 配置・透明度・timer・optionは通常のdestinationと同じです。BGA無効時はBGA由来のAmbientも非表示です。
- BGAのBase / Layer / Layer2（静止画の黒透過を含む）を合成してからぼかします。
  POOR表示中はPOORへ切り替えます。通常のBGA destinationは引き続き鮮明に描画します。
  destinationのRGBAは各BGAフレームのtintと乗算し、Ambient合成前にも適用します。
- 通常のimageでも利用でき、汎用画像・動画をBGAなしの曲の背景に使えます。
- 画面全体を読み戻す背景ぼかしではありません。文字・ノーツ・その他のdestinationは処理対象外です。
- 中間画像は既定のぼかしで長辺128pxです。弱いぼかしでは縮小による劣化を抑えるため最大1024pxまで増やします。
  ぼかし0％では通常の画像描画を使い、中間画像を作りません。
  同時に最大8 destination、各64画像・矩形までです。
  GPU内で処理し、動画textureの更新後に合成・ぼかし・通常描画を同じsubmissionで実行します。
  対象がないときは専用pipeline・中間画像を確保せず、使用済みのものも解放します。

同梱antiqueの「Ambient」は既定OFFです。旧名称の保存設定・スキン履歴は読み込み時に移行します。
「Ambient表示方式」は「全体」（既定） / 「Spread」、
「Spread範囲 (%)」は0〜200％（10％刻み、既定20％）、
「Ambientぼかし度 (%)」は0〜100％（10％刻み、既定50％）です。
ON時は画面の背面へAmbientを描き、「Ambientパネル透明度」でフレーム・グラフ背景の
透明度を0〜100％の10％刻みで設定します（既定40％、OFF時は適用しない）。
レーン背景は対象外とし、不透明度100％を維持します。
数値・テキスト、ノーツ、判定、グラフ本体の透明度は変えません。
フレーム画像に含まれる装飾やラベルはフレームと一緒に透過します。
Ambient ONではAmbient OFF時の暗いcover-fit BGA背景を描かず、鮮明なfit-inside BGAを重ねます。
「BGAサイズ＝背景(1920x1080)」では鮮明なBGAも描かず、選択したFull / SpreadのAmbientだけを表示します。
Ambient OFFではスキン本来のBGA描画を行います。
曲BGAと汎用BGA/BGIの選択条件は既存の設定に従います。

antiqueのBGA明るさは次の2つのoffsetで独立して調整します。範囲は-255〜0、既定0です。

| offset名 | ID | 対象 |
| --- | --- | --- |
| 前面BGAの明るさ(-255 ~ 0) | 54 | 余白を除くfit-insideの前面BGA / 汎用BGA・BGI |
| 背景/Ambient BGAの明るさ(-255 ~ 0) | 65 | Ambient OFF時の暗いcover-fit背景とFull / Spread Ambient |

BGAサイズが「背景(1920x1080)」の場合は、Ambient OFFの鮮明なBGAも背景側の設定を使います。
明るさは各destinationのRGBを`255 + 設定値`にすることで適用し、alphaを維持します。
前面を暗くしても背景が透けず、前面映像の外側や別のパネルを黒い矩形で覆いません。
既存の黒矩形は開始500msのフェードだけに使い、最終alphaは0にします。
同梱antiqueの旧「BGAの明るさ(-255 ~ 0)」保存値とスキン履歴は前面側へ移行し、
背景側は既定0から開始します。新しい前面設定が保存済みならそちらを優先します。

## LR2の基準解像度

LR2 CSV skinで`#RESOLUTION`を省略した場合は640×480として読み込みます。
`#RESOLUTION,0`も640×480、`1`は1280×720、`2`は1920×1080、
`3`は3840×2160です。`#RESOLUTION,幅,高さ`の正数による明示指定も受け付けます。
include先の描画座標も、ヘッダーで決まった同じ基準解像度で変換します。

BMZ独自の互換拡張として、`#RESOLUTION`を省略したスキンは基準解像度を推定します。
LR2 HD向けのテーマ（EndlessCirculation、W-mix HD等）は解像度を宣言しないことが多いため、
ロード時のoptionで有効な分岐とinclude先の`#DST_*`のうち、`x,y,w,h`が
`0,0,640,480` / `0,0,1280,720` / `0,0,1920,1080` / `0,0,3840,2160`に一致する数を数え、
最も多い解像度を使います。同数なら小さい解像度、該当がなければ640×480とします。
beatorajaは推定せず640×480として読むため、この挙動は互換仕様と区別します。

すべてのLR2スキン（Play / Decide / Select / Result / Course Result等）に
`LR2 Resolution (BMZ)`を追加します。既定の`Auto`は`#RESOLUTION`と上の推定を使い、
640×480 / 1280×720 / 1920×1080を選ぶと宣言・推定より優先します。
以前の保存値`Skin default`は`Auto`として扱います。
この上書きは`#RESOLUTION,幅,高さ`形式の明示指定とは区別し、
LIFTに追従するボム・キービーム等の扱いを変えません。
解像度を宣言しないHDスキンでも、外部CSVを変更する必要はありません。

## LR2の条件分岐

`#IF` / `#ELSEIF` / `#ELSE` / `#ENDIF`はOpenLR2の`ReadSkin`と同じく、
各階層を自分の条件だけで判定します。外側が偽のブロック内でも、内側の`#IF`が真なら実行し、
`#ENDIF`で外側の状態へ戻ります。描画時に評価するopの条件も、最も内側の階層の分を付けます。
閉じられていない`#IF`はファイル末尾まで有効とします。
分岐のスタックはCSVファイルごとに独立します。include先の余分な`#ENDIF`や閉じ忘れは
呼出元の分岐を変更しません。`#SETOPTION`の値とSRC/DSTの宣言状態はinclude間で共有します。
呼出元の描画時条件はinclude全体へ引き継ぎ、include先の条件と合わせて評価します。
beatorajaは入れ子を持たず`#IF`ごとに判定し直すが、閉じた入れ子を除けば同じ結果になります。

Playの通常 / AUTO、Replay、保存可否、難易度・譜面モード等は、ロード時の状態が
渡されている場合、その値で`#IF`を確定します。通常用とAUTO用のincludeを同時に取り込みません。
状態を変えてロードするときはdecode cacheも無効化します。ロード時の状態がない低水準APIでは
通常プレイを既定とし、それ以外の未取得条件は描画時に評価します。
Resultの同じ固定条件とゲージ種別は、未取得ならfalseとして分岐を確定します。
条件付き`#IMAGE`を二重に読み込んで後続の画像番号をずらしません。
描画時条件の複合`#ELSEIF` / `#ELSE`は、条件全体の否定を保持します。
`#SETOPTION`で上書きした条件は参照したCSVの依存として扱い、元の選択値とは区別します。

`#INCLUDE`のwildcardは、まず`#CUSTOMFILE`の選択・既定ファイルを適用し、残った
ファイル名の`*`に一致するCSVから抽選します。名前はASCII大文字小文字を区別しません。
同じロードの解像度推定と本変換では同じ選択を使い、新しいロードで再抽選します。
抽選を含む文書はdecode cacheへ保存しないが、画像・GPU textureのキャッシュは利用できます。
CSVファイル名の単一wildcardが対象で、ディレクトリ名のwildcardとDXA内CSVは対象外です。

## LR2のnumber / option

LR2とbeatorajaで同じ番号の意味が違うnumber / optionは、変換時にLR2専用の参照へ移し、
LR2の`GetNumber` / `GetOptionFlag_dst`の定義で評価する（number `93_000+ID`、
option `94_000+ID`、否定は符号を維持）です。意味が一致する番号はbeatorajaの参照のまま使います。
変換の表は[refs.rs](../crates/bmz-skin/src/lr2/refs.rs)に集約します。

| 種類 | ID | 画面 | LR2の定義 |
|---|---|---|---|
| number | 100 / 107 / 108 / 109 / 114 | Play、Result（1P / 反転後の自分側） | LR2式スコア、2%刻みのゲージ、2P（SPではターゲットの現在進捗）とのEX差、次ランクまでの差（負値、MAXは0）、POOR＋空POOR |
| number | 154 | Result / Course Result | 自分のEX − 次ランクの整数境界です。AAAではMAXとの差を負値で表示し、MAXは0です。JSON/Luaの同番号の正値とは別 |
| number | 163 / 164 | Result / Course Result | 終了した譜面・コースの長さの分 / 秒です。Playの残り時間とは別 |
| option | 100..105 | Result / Course Result | 更新後の自己ベストのクリア種別（未取得なら今回のクリア）です。ASSIST系をEASY、EX-HARDをHARD、PERFECT / MAXをFULLCOMBOへ畳む |
| number | 92 / 93 / 94 / 95 | Select / Decide | 選択中の曲のIR順位、参加人数、EASY以上のクリア率、ライバルとのEX差です。フォルダでは非表示 |
| number | 12 | Select / Decide | 判定タイミング（ソート変更中も同じ） |
| number | 45..49 | Select / Decide | 選択中の譜面と同じフォルダにある各難易度（BEGINNER..INSANE）の最高レベルです。難易度が無い欄は非表示 |
| option | 500..519 | Select / Decide | 同じフォルダに各難易度が無い / ある / 1譜面 / 複数譜面 |
| option | 520..565 | Select / Decide | 各難易度のレベルバーのランプ（`520 + 10 × 難易度 + LR2ランプ`）です。同じ難易度が複数あれば最良のクリア |
| option | 70..79 | Select / Decide | 各難易度のレベルが点滅しきい値以下（70..74）/ 超過（75..79）です。しきい値はLR2の既定値（5 / 10鍵9、7 / 14鍵12、9鍵42） |
| bargraph | 105..109 | Select | 各難易度のレベル ÷ 点滅しきい値（LR2のアニメーションは省略） |
| option | 118..149 | Select / Decide | 選択中の譜面を過去にクリアしたオプション（LR2の`op_history`）です。スコア履歴のオート以外のクリアから、NORMAL以上はゲージ（118..123）とRANDOM（126..131）、EASY系はEASYゲージ（121）、アシスト付きは142だけを立てます。HIDDEN / SUDDEN（134..141）は記録が無いため立ちません。BMZではアシスト付き（AUTO SCRATCHを含む）とH-RANDOM / ALL-SCRのクリアはランプだけを更新し、スコア履歴に残りません（[スコア保存](score-persistence.md)）。そのため142とH-RANDOM / ALL-SCRのbit（130 / 131）も実際には立たない |
| number | 250..259 | Select / Decide | 選択中のコースの各ステージのレベルです。未解決のステージは非表示 |
| number | 70 / 73 | Select / Decide | 選択中の曲の自己ベストのLR2式スコア / レート（%） |
| number | 210..219 | Select / Decide | IRのFAILED / EASY / CLEAR / HARD / FULLCOMBOの人数と率です。ASSIST系はEASY、EX-HARDはHARD、PERFECT / MAXはFULLCOMBOへ畳む |
| number | 212 / 214 / 216 | Result / Course Result | 拡張LR2のFAST / SLOW / コンボブレイク数（Playの210 / 212 / 214と同じ扱い） |
| number | 50..56 / 60..66 | 全画面 | EQ / FX / PITCHです。BMZに設定が無いため非表示（beatorajaの54 / 55はDPオプション / HS-FIX） |
| number | 290 / 291 | 全画面 | 最高 / 最低BPM（90 / 91と同じ） |
| text | 190..199 | Select / Decide | ランダムコースの設定です。BMZに該当が無いため空文字 |
| text | 60..65 / 70 / 74 / 76 / 80 / 84..85 | Select / Decide | BMZで変更できる設定の現在値（キーフィルター、ソート、難易度、RANDOM 1P / 2P、ゲージ、FLIP、HS-FIX、BGA、自動調整、レーン効果）です。text 84 / 85は同じ共通設定を表示します。文言はBMZの表記（例: NORMAL / HARD / HAZARD、H-RANDOM / ALL-SCR、SCORE、AUTO）を使います。キーフィルターはBMZ全体でLR2と同じ`ALL KEYS` / `7KEYS`等の表記にそろえています。変更できない設定（GHOST、SCORE GRAPH、BGA SIZE、画面設定、REPLAY等）は空文字 |
| number | 150 / 151、155..158 | Play | 自己ベスト / ターゲットのゴーストの現在進捗でのEXと率です。自己ベストはゴーストがあれば使い、無ければ線形換算です。ターゲットは線形換算 |
| number | 120..136（シングルプレイ） | Play | 2Pはターゲットのゴーストです。BMZはEXの進捗換算だけを持つため、EX・率・差・次ランク差・ノーツ数以外は0 |
| number | 120..136（シングルプレイ） | Result / Course Result | 自分と反対側はターゲットの最終EX・率・差・次ランク差・ノーツ数・取得済み最大コンボを表示します。判定数・ゲージ等の未取得値は非表示です。左右反転に追従する |
| number | 272 / 273 / 274 | Select / Decide | ライバルの理論値EX・レート・ノーツ数（同じ譜面のノーツ数から計算） |
| option | 52、620 / 622 | 全画面 | EXTRA OFF、ランキング表示OFF、ゴーストバトルOFFとして常にtrue（53、621 / 623はfalse） |
| option | 54..57 | 全画面 | 1P / 2PのAUTO SCRATCH OFF / ONです。プレイ画面は実際に自動演奏しているスクラッチレーン、他の画面は設定値 |
| option | 100..105 / 110..117 | Select / Decide | 選択中の曲のクリアランプ（beatorajaのクリア種別をバーのランプと同じ5段階へ畳む）とランク（プレイ済みのみ）です。描画時に評価する |
| option | 340..347 | Result / Course Result | 今回のスコアのランク |
| option | 20 | Select / Decide | オプションパネルを開いていません。描画時に評価する |
| option | 10 / 11 / 12 | Select / Decide | STYLEフィルターのDP / BATTLE / DPまたはBATTLEです。14 / 10鍵フィルター、またはBATTLEがD-BATTLE / SP-TO-DPのとき10と12がtrueです。選択譜面の鍵盤数からは判定しません。2人対戦BATTLEはBMZ未対応のため11はfalseです。`#IF`内も描画時に評価する |
| option | 21..29 | Select / Decide | 対応するオプションパネルを開いています。`#IF`内も描画時に評価する |
| option | 600..608 | Select / Decide | LR2の単一の`IRstatus`です。常にどれか1つだけが成立します。IR未設定・非対象の行は600、取得待ちは606、取得中は607（LR2の接続中）、取得済みで参加者ありは602、参加者0人（未登録の譜面を含む）は603、取得失敗は604です。BMZに無い一時読み込み（601）、BAN曲（605）、IRビジー（608）はfalseです。beatoraja / JSON / Luaの600..608は従来どおりです（0人で602と603、失敗で604と608が同時に成立する）。`#IF`内も描画時に評価する |
| option | 571 / 572 | Select / Decide | コース作成中 / コース作成中ではありません。BMZにコース編集画面が無いため571はfalse、572はtrueです。572を条件とする選曲スコアバーも表示する |
| option | 62..66 | Select / Decide | 現在の設定で取れる最高ランプ（クリア不可 / EASY / NORMAL / HARD / FULLCOMBO）です。オート・練習はクリア不可、H-RANDOM / ALL-SCR / 等速はEASYまでです。ゲージはA-EASY / EASY→EASY、HARD / EX-HARD→HARD、HAZARD→FULLCOMBO |
| option | 640..645 / 650..657 | Select / Decide | 選択中ライバルのクリアランプ（5段階）とランク |
| option | 352..354 | Play | 2P（シングルプレイではターゲットのゴーストの現在進捗）との勝ち / 負け / 引き分け |

Select / Decideの`#IF`に書かれたoption 50 / 51（IRオフライン / オンライン）は、読み込み時に固定せず描画時に評価します。
bargraph 140..145（判定率）はSelectでは選択中のスコアを譜面のノーツ数で割ります。

同じフォルダ内の各難易度は、選択中の譜面の`folder_path`が同じ譜面をライブラリから読み、
難易度名（数字またはBEGINNER..INSANE / LEGGENDARIA）で振り分けます。難易度名が無い譜面は数えません。
結果は選曲一覧の再読み込み（プレイ後のスコア更新を含む）まで保持します。

ライバルのIR順位（number 292 / 293）等は、
必要な値の供給が無いため未対応です。EQ / FX / PITCH、パネル2..7、スキン設定・コース作成・ヘルプの埋め込み、
`README` / `MOUSECURSOR`、IRランキング一覧の`BAR_RANK`はBMZに同じ機能が無いため対応しません。

## LR2のSelect / Result

type 5の選曲、type 7の通常リザルト、type 15のコースリザルトを既存の各スロットから選びます。
ライブラリ、プロフィール設定、スコア保存、IR、リプレイ、リトライはBMZの処理を利用します。
LR2固有のbutton / numberは変換時に内部参照へ分離し、beatorajaの同番号の意味を変えません。
`LR2files/Theme/<theme>/...`形式のincludeは、同じスキンライブラリ内にある
指定テーマの実ファイルを優先します。存在しない場合は自テーマ内で解決します。
CSVの引用符はフィールド先頭で開きます。引用されていない画像パス末尾の余分な引用符は
パス正規化で除去し、後続のカンマやコメントを画像名へ混入させません。

選曲は`BAR_BODY_ON/OFF`、`BAR_TITLE`、`BAR_LEVEL`、`BAR_LAMP`、
`BAR_MY_LAMP`、`BAR_RIVAL_LAMP`、`BAR_RIVAL`、`BAR_FLASH`、`BAR_CENTER`、`BAR_AVAILABLE`を変換します。
子部品はバーの左下を原点とし、スロット番号は条件分岐でずらしません。
バー本体はLR2の番号を保持し、コースは`SRC_BAR_BODY`の8、ランダムコースは9を使います。
コース一覧フォルダは6、NEW COURSEは7を使います。通常の曲フォルダは1、
仮想フォルダの分類階層・抽出項目・検索履歴は2、NEWのルートと配下は3を使います。
設定フォルダとその中の「戻る」は4のライバルフォルダ画像を使います（BMZの表示方針）。
beatorajaのバー番号をLR2へ流用しません。
未所持譜面・未所持コースは5の灰色バーを使い、ライバルフォルダ用の4は使いません。
これはBMZの表示方針であり、5はLR2ではライバルモードの曲バーに相当します。色や装飾はスキンの画像定義に従います。

Selectの`songlist`は、LR2形式・JSON/Lua形式ともに、現在の描画位置・速度から目標位置への
三次Hermite補間を使用し、終点速度を0にします。LR2の選曲バーの線形補間とは異なるBMZの表示仕様です。
移動途中の再入力では符号付きの残り行数とその時間微分を引き継ぎ、1行に制限しません。
設定時間を基本とし、目標へ向かう速度が大きい場合は初期接線が距離の3倍を超えない時間へ短縮します。
反転・残量0の再入力では、現在位置と目標の間から外へ進む量を約1/4行以内に抑える時間へ短縮します。
位置や速度のclampで不連続を作らず、所要時間だけを調整します。時間0と直接指定のリセットは即時反映です。
補間は残量全体に1回だけ適用し、行ごとの区間にeaseを適用しません。

選択行の条件op 1..5もLR2として評価します。1はフォルダ、2は譜面、3は保存済みコース、
4はNEW COURSE / RANDOM MIX、5は譜面・保存済みコース・RANDOM MIXで、NEW COURSEを含みません。
BMZ独自のRANDOM SELECTは2の画像を使い、op 5で決定可能な操作として扱います。
`BAR_TITLE`は譜面の初回登録日時により0（通常）/ 1（新着）を選びます。
profileの`[select].new_title_hours`は既定24時間、0で無効です。NEWフォルダの抽出期間とは独立します。
フォルダや未所持譜面には登録日時を補完せず、通常タイトルを使います。
ライバルのEXが1以上ある曲だけ`BAR_MY_LAMP` / `BAR_RIVAL_LAMP`に分け、
それ以外は`BAR_LAMP`を使います。`BAR_RIVAL`は各行の自分とライバルのEXから
勝ち0・負け1・同点2・自分のスコアなし3を選びます。

整数部分は表示行の組み替え、小数部分は隣接スロットのdestinationの座標・幅・高さの補間に使用します。
この空間上の補間は線形であり、整数pixelへの丸めとスキン配置の折れ曲がりは残ります。
非等間隔・左右に曲がる配置にも同じ計算を適用し、本文・タイトル・ランプとクリック領域を揃えます。
`songlist.flash`は中央スロットの原点に固定し、表示行の組み替えで発光位置が飛ばないようにします。
描画順は各destinationの最初のDSTの宣言順（include先を含む行順）とします。LR2のDSTによる全体の並べ替えに対応し、
曲バーの部品（`DST_BAR_BODY_OFF` / `ON`の各スロット、`TITLE`、`LEVEL`、`LAMP` / `MY_LAMP` / `RIVAL_LAMP`、`RIVAL`、`FLASH`）は
それぞれ最初のDSTの位置で全行分まとめて描かれ、間に宣言した通常の画像・パネル・マスクとも前後します。
変換時は部品の各条件付きdestinationに、その位置のsonglistの目印（`lr2BarPart`）と共通の`order`を付けます。
描画時は行ごとに有効なdestinationを選び、その目印の位置で部品を出します。別の`#IF`分岐やSRCからの定義は
それぞれの位置を保ち、同じ定義の連続DSTはアニメーションのkeyframeとして同じ目印へまとめます。
目印の無い部品は最初の目印の位置で描きます。クリック判定は従来どおり最初の目印の位置で1回だけ行います。
JSON / Luaスキンのsonglistは目印を持たず、従来どおり1か所で行ごとに描きます。
描画準備では目印一覧をdocumentの構造cacheへ保持し、`order`を持つLR2の部品は
フレーム内で1回だけ描画位置へ振り分けます。目印の重複やclipで見えない目印も扱い、
`order`の無い旧形式の目印は従来の部品名・indexによる照合を維持します。
別オブジェクトのDSTをkeyframeの間へ挟み、アニメーションに応じて前後関係を変えるLR2の`sortID`補間は対象外とします。
表示位置が選択から複数行遅れる場合は、その位置の周囲の行を別途供給します。
選択中の譜面・number/text/option参照・決定操作は入力直後の選択に従い、選択変更タイマーと
補間時計は独立します。フォルダ切替や直接指定では補間を破棄します。
`DST_BAR_TITLE`の予約列（通常DSTのop1..3相当）は無視し、`#IF`の表示条件は維持します。
画像・スライダーの`blend=0`はDSTのalphaを無視します。画像素材自体の透過は維持します。
スライダーも表示開始時刻・ループ・移動位置を評価した後、共通の画像描画へ渡します。
クリアランプは表示だけLR2の同系列へまとめ、保存されたクリア種別は維持します。
`ONMOUSE`の局所範囲と、`SRC_ONMOUSE` / buttonのパネル制限も評価します。
`SRC_ONMOUSE`のpanel=0は全パネル共通、1..9は指定パネルでだけ描画します。
この条件はDSTの表示条件と併せて評価し、別パネル用の発光が重ならないようにします。

共通設定は意味で対応付けます。LR2のbuttonはBMZ設定画面を開かず、profileの値をLR2の順に直接切り替えます。

| button | 設定 | LR2の順（BMZ値） |
|---|---|---|
| 10 / 91..96 | 難易度フィルター | 10は常に次へです。91..96は直接選択 |
| 11 | キーフィルター | ALL KEYS / 7KEYS / 5KEYS / 14KEYS / 10KEYS / 9KEYS（常に次へ） |
| 12 | ソート | LEVEL / TITLE / CLEAR / SCORE（常に次へ） |
| 40 | ゲージ | NORMAL / HARD / DEATH(HAZARD) / EASY |
| 42 / 43 | 1P / 2P配置 | NORMAL / MIRROR / RANDOM / S-RANDOM / H-RANDOM / ALL-SCR |
| 44 / 45 | ASSIST 1P / 2P | OFF / AUTO SCRATCH（両側共通のAUTO SCRATCHを切り替える） |
| 46 | LANE COVER | OFF / ONです。BMZはLR2のSUDDENとSUDDEN+を1つのSUDDEN+として扱うため、レーン効果のSUDDEN成分（SUDDEN / HID+SUDでON）を表示・切り替える |
| 50 / 51 | EFFECT 1P / 2P | OFF / HIDDEN / SUDDEN / HID+SUD（同じ共通レーン効果を切り替える） |
| 54 | DP FLIP | OFF / FLIP |
| 55 | HS-FIX | OFF / MAX / MIN / MAIN BPM（LR2のAVERAGEスロットを使用） |
| 56 | BATTLE | OFF / D-BATTLE / SP-TO-DP / G-BATTLE（下記） |
| 57 | ハイスピード | 押した側で上下 |
| 72 | BGA | OFF / ON / AUTOPLAY |
| 74 / 75 | 判定タイミング / 自動調整 | ±1ms / OFF・ON |
| 77 | TARGET | NO TARGET / AAA / AA / A / IR TOP / IR NEXT |
| 1..6 | パネル | 対応するパネルを開きます。開いているパネルの同じbuttonで閉じる |
| 170..185 | スキン設定の対象 | 7 / 5 / 14 / 10 / 9KEYS、SELECT、DECIDE、RESULT、BATTLE7 / 5、COURSERESULTを選ぶ（KEYCONFIG / SKINSELECT / SOUNDSET / THEME / BATTLE9は対象外） |
| 190 | スキン変更 | 対象のスキンをBMZの既定と導入済みスキンの順で切り替える（端で折り返す） |
| 220..229 | カスタマイズ | 表示中の行の値を切り替えます。選択肢は端で折り返し、ファイル選択はRANDOMと候補ファイルを順に選ぶ |

button 50 / 51は1P / 2Pどちらの項目を操作しても共通のレーン効果を切り替えます。text 84 / 85も同じ共通値を表示します。
難易度フィルターを切り替えると、選択中の曲フォルダにある該当難易度の譜面を選択します。その難易度が無い場合は、一覧に残るフォルダ内の譜面を選択します。
HS-FIXのBMZ MAIN BPMは、LR2 button 55の選択状態ではAVERAGE（値3）に対応させ、text ref 74ではMAIN BPMと表示します。LR2固有のMAINBPM（値5）とは分けて扱います。

BATTLE（button 56 / text 69）はBMZに単独の設定を持たず、SESSION MODE・DP OPTION・KEY CONVERSIONから値を決めます。
LR2の2人対戦BATTLE（値1）は選べません。

| LR2 BATTLE（値） | 条件（上から優先） | 選んだときの書き込み |
|---|---|---|
| G-BATTLE (4) | SESSION MODE = G-BATTLE | SESSION MODEをG-BATTLEにする |
| D-BATTLE (2) | DP OPTION = BATTLE / BATTLE AS | DP OPTIONをBATTLE（AUTO SCRATCH ONならBATTLE AS）にする |
| SP-TO-DP (3) | KEY CONVERSION = SP TO DP | KEY CONVERSIONをSP TO DPにする |
| OFF (0) | 上記以外 | — |

どの値を選んでも、他の値に当たる設定は解除します（G-BATTLE / AUTOPLAY BATTLEはNORMAL、BATTLE系のDP OPTIONはOFF、SP TO DPはOFF）。
PRACTICE・AUTOPLAY・FLIP・7K TO 9K / 6KはLR2に無い値のため、置き換える場合を除いて維持します。
優先順はプレイ開始時の扱い（バトルがKEY CONVERSIONより優先）と同じです。譜面のキー数で使えない値は、これまでどおり開始時に無効化します。
D-BATTLE中はFLIPがOFFとして表示されます（button 54 / text 70）。option 10 / 12はDPのキーフィルターに加え、D-BATTLE / SP-TO-DPでも成立します。

ASSIST（button 44 / 45、text 67 / 68）はプロフィールのAUTO SCRATCHを切り替えます。D-BATTLE中に切り替えると、BATTLEとBATTLE ASも連動して切り替わります。
option 54..57（1P / 2PのAUTO SCRATCH OFF / ON）は、プレイ画面では実際に自動演奏しているスクラッチレーン、選曲・リザルト等の他の画面ではLR2の`p1_assist`と同じく設定値で判定します。
option 62..66（このまま取れる最高ランプ）は、AUTO SCRATCHをD-BATTLE以外でEASY止まり、SP-TO-DPを保存なし（0）として扱います。

マウスはLR2と同じく左クリックだけを受け付け、増減はbuttonの指定（plus only / minus only / 左右半分）で決めます。
10..12はLR2と同じくクリック位置に関係なく次へ進めます。
現在値がLR2の候補にないBMZ独自値（EX-HARD、R-RANDOM、START BPM等）の場合、
次へは候補の先頭、前へは末尾へ移ります。BMZ独自値はbuttonから選ばず、BMZ設定画面で変更します。
7..8のパネル、FX/EQ/PITCH、2P側のゲージ・HS、
SCOREGRAPH・GHOST(70/71)、BGA SIZE(73)、既定ターゲット率(76)、
システム設定(80..83)、favorite tag(90)等、BMZに同じ意味の設定が無いbuttonは何もしません。
LR2スキンのoption panelを使うときは、BMZ独自の画面下部オプション表示を重ねません。
E1 / E2 / E1+E2はスキンのパネル1 / 2 / 3を開きます。パネル2以降はマウスで操作し、
BMZのASSIST / DETAILパネルはLR2スキンでは表示しません（BMZ メニューで変更します）。詳細は[操作](controls.md#lr2選曲スキン)を参照してください。
button 1..6のクリック開閉は通常の鍵盤・ゲームパッド入力をまたいで維持します。

スキン設定は「BMZ メニュー → スキン」と同じプロファイルのスキン設定を変更し、変更したスキンをその場で再読み込みします。
カスタマイズの行は選択肢（`#CUSTOMOPTION`等）、ファイル選択（`#CUSTOMFILE`等）の順に並び、
保存値が無い項目はスキンの既定値を表示します。位置調整（offset）は一覧に出しません。
text 50 / 51は対象スキンの名前・作者、100..109 / 110..119は表示中の行の項目名・値、
slider 7は一覧のスクロール位置です。BMZ メニューでの変更も一覧へ反映します。
表示行数はスキンが定義する行（button 220..229・text 100..119の最後の行）から決めます（ECBEは7行、上限10行）。
スクロール量は項目数からこの行数を引いた値です。
LR2のsliderは、つまみを含む移動範囲全体で押下を受け付け、値はつまみの中心位置で決めます（LR2の`SliderByTime`）。
押したsliderはカーソルが範囲外へ出ても追従し、端の値で止まります。
行・customize button・slider 7の移動範囲を合わせた領域はスキン設定の領域として扱い、
その上のマウスホイールは一覧を1行ずつスクロールし（BMZ拡張です。LR2は常に曲リストを動かす）、
クリックは下にある曲バーへ通しません。
customizeで表示中の選曲スキンを再読み込みしたときは、開いているパネルと画面のタイマーを維持します。
対象のbuttonはLR2本体と同じく172を14KEYS、173を10KEYSとして扱う（LR2同梱のskinselect画像とOpenLR2の
`op - 170`による種別番号で確認）です。ECBEは14K表示のタブに173、10K表示のタブに172を割り当てているため、
LR2本体と同様にタブの表示と編集対象が入れ替わります。スキン側の割り当てであり、BMZでは補正しません。
E1/E2のhold変化で物理操作へ戻し、modal・フォーカス喪失・画面切替では解除します。

リザルトは`GAUGECHART_1P/2P`の自分側と`SCORECHART` index 0を実際の履歴から描画します。
`FLIPRESULT` / `DISABLEFLIP`に従って自分側のグラフ・number ref 100..116 / 120..136・ランク条件300..318を切り替えます。
timer 150はグラフ開始、151は描画完了またはスキップ、152は更新演出開始です。
`STARTINPUT`の待ち時間に従い、グラフのスキップ→ランク表示→更新演出→退出を進めます。
保存対象外のプレイと、timer 152を使う部品が無いスキン（W-MIX、ECSE等）は更新演出を挟まず、
ランク表示後の1回の入力で退出します。`STARTINPUT`のランク表示時間が0なら、beatorajaと同じく
timer 152を画面開始と同時に始め、更新演出を待つ入力を挟みません。演出段階とは独立して既存の保存処理を実行します。
Play / Resultのbutton 40 / 41（ゲージ種別）は、LR2に無いゲージをA-EASY→EASY、EX-HARD・段位ゲージ→HARDの画像で表示します。
Selectの設定表示では従来どおり表示しません。
Course Resultのoption 100..105は、通常Resultと同じく今回の結果を保存した後のコースベストを使います。

LR2のDecideはOpenLR2の`ProcI_Decide`に合わせ、スキップでtimer 3を開始し、`#CLOSE`の時間
（`#CLOSE`が無いスキンは`#FADEOUT`の時間）が過ぎたらPlayへ進みます。timer 2は開始しません。
キャンセルと`#SCENETIME`の経過は従来どおりtimer 2と`#FADEOUT`を使います。LR2のDecideでは
JSON / Lua向けのシーン末尾への時計の補正を行わず、timer 2 / 3の部品で退出演出を描きます。
clear / failed・難易度150..155・確定ランク300..308のロード時分岐と解像度指定はdecode cacheの依存関係に含めます。
難易度のロード時条件は終了した譜面のmetadataを使い、描画時と同じ変換で評価します。
数値1..5とBEGINNER / NORMAL / HYPER / ANOTHER / INSANEを151..155へ対応付け、
未定義は150とします。通常ResultとCourse Resultの両方へ渡し、前回の難易度条件を残しません。
ランクのロード時条件は`FLIPRESULT`適用前の自分の成績を使います。相手側310..318の成績は補完しません。
`#IMAGE,CONTINUE`と`#LR2FONT,CONTINUE`は直前のPlayと同じ番号の素材を引き継ぎます。
抽選・CUSTOMFILEを適用した実際のパスと透過色を保持し、Result入場時に引継ぎ元を固定します。
Playの読込時には同じScoreKeyの自己ベストのクリア種別をop 100..105へ渡し、
クリア種別ごとの引継ぎ画像を選びます。未プレイはNOPLAYとします。
IR更新などのResult再読込でも引継ぎ元は変わりません。ステージResultからCourse Resultへ
進む場合はそのステージResultを引き継ぎます。引継ぎ元に存在しない画像は透明とし、
同じ番号の別素材で補完しません。引継ぎ素材の変更はdecode cacheの依存関係に含めます。

BMZ独自の補完として、PlayがLua / JSONスキン、またはResultと別テーマのLR2スキンだった場合は、
Resultと同じテーマ（スキン置き場の最上位フォルダ）のLR2 Playスキンを引継ぎ元の代わりに使います。
ECSE・W-MIXの自己ベスト（更新前）のランプ、W-MIXのゲージ画像、LR2FONTの文字が対象になります。
補完元は一覧のLR2 Playスキンから、今回の鍵盤数の種別、7K、その他の順にパス順で最初のものを選びます。
スキン設定は既定値、op 100..105はPlay読込時と同じく保存前の自己ベストのクリア種別とします。
画像・フォントは読み込まず、パスだけを解決します。補完元の空の`CONTINUE`番号は元の引継ぎ素材を残します。
同じテーマのLR2 Playで遊んだ場合と、同じテーマにLR2 Playスキンが無い場合は補完しません。
Course Resultはこの補完を含むステージResultを引き継ぎます。RED_BELT（OA_DX+のPlayが前提）と
Decide経由の引継ぎ（コースモードのランプ等）は対象外です。

LR2ではop 30/31（BGA Size）がグローバル設定として全画面で有効なため、
Play以外のLR2スキンにもスキンオプション`BGA Size`（Normal / Extend、既定Normal）を追加します。
3R等はこれをResultのレイアウト選択に使います。Playスキン個別の設定とは連動しません。
`#CUSTOMOPTION_ADDITION_SETTING`で非表示にしたResultでは、Normal（30=true / 31=false）として渡します。

現時点ではLR2専用のFX/EQ/音声設定パネル、組合せキーフィルター、ディレクトリソート、
特殊S-RANDOMの挙動、BAR_RANK装飾は未対応です。
`README`の埋込描画と独自`MOUSECURSOR`も未対応です。文書表示はBMZの既存機能、カーソルはOSの表示を使います。
リザルトの対戦相手の確定値・履歴、過去ベストやライバルのSCORECHART index 1/2は未対応です。
シングルプレイの2P数値はLR2のターゲットゴーストとして扱い、最終値から履歴を作りません。
スキンが参照する別配布の壁紙・マウス素材等は利用者が用意します。

## LR2のDST時刻

LR2 CSVから生成したdestinationは内部属性`lr2Timing: true`を保持し、
OpenLR2の`SetDSTdrawByTime`に合わせて時刻を評価します。JSON/Luaの既定評価は変更しません。
通常の画像、ゲージ、判定画像、コンボに共通で適用します。

- ループ前の経過時刻が最初のDST時刻に達するまで非表示です。
- `loop < 0`は終端時刻まで表示し、終端を過ぎたら非表示です。
- 最初と最後のDSTが同時刻で`loop >= 0`なら、開始後は最後のフレームを保持します。
- `loop == 終端`は最終フレームで停止します。
- `0 <= loop < 終端`は終端を**過ぎてから**ループし、終端ちょうどでは最終フレームを表示します。
  ループ後の時刻が最初のDSTより前なら、最初のフレームを表示します。
- 複数時刻で`loop > 終端`は評価時刻を0とします。

例えば`time=1000, loop=0`のDSTが1つだけなら、1000ms以降の表示を保持します。
タイマーがOFFなら通常どおり非表示です。ループ・タイマー等のdestination属性は最初のDST行から取得します。

## LR2のPlay描画

SRC/DSTはIMAGE・NUMBER・LINE・NOWJUDGE等の種類ごとに最新のSRCを保持します。
別の種類のSRCやincludeを挟んでも、DSTを無関係な画像・数字へ結び付けません。
DSTを持たない既定SRCの後に条件付きSRCが続く場合は、最後に成立したSRCだけを描画します。
既定の難易度画像とANOTHER画像、Free StageとDEMO画像などを同時に重ねません。
変換内部の`destination.lr2Not`は、置換・複合分岐の否定条件を保持するための属性です。

LR2数値は`value.lr2Number`で区別し、SRCのセルを連続10 / 11 / 24セル単位の
フレームとして扱います。24セルの符号付き素材は12×2だけでなく6×4、4×6、24×1にも対応し、
正側0..11、負側12..23の位置を使います。cycleのフレームはSRC時計から選びます。
空き桁・左右 / 中央揃え・桁あふれ・符号なし素材の負数非表示もLR2の数値描画に従います。
複数フレームの符号付き素材では、LR2と同じく数値部分の共通行と符号セルを使います。
JSON/Luaの通常数値は従来のレイアウトを維持します。

`SRC_BUTTON`は`image.lr2Button`で区別し、状態番号を分割画像の先頭から数えたセルへ
直接対応させます。横並び・縦並び・複数行を同じ規則で評価し、SRCのcycleで別セルへ進めません。
JSON/Luaの通常buttonは従来の行選択を維持します。

`#HORIZONTAL`は`note.lr2Horizontal`へ変換し、右から左へスクロールします。
画像の向き・レーンのY座標は維持し、移動距離は初期DST_NOTEのXからキャンバス右端までです。
LNは両キャップ間に横長の胴体を描画し、小節線・補助線も同方向へ動かします。
LIFT・lane cover・hidden coverのoffset（3..5）はX軸へ適用し、通常UIの座標は維持します。
`DST_NOTE2`は横スクロール時には落下先Xとして扱います。
参照実装で縦計算が残るLN/小節線も一貫して横へ流す点はBMZの補完動作です。
カバーの`disapearLine`による横向きcropは未対応で、縦方向の定義を使います。

`#FONT`は`#LR2FONT`と別の番号表を使います。同番号のbitmap font宣言を優先し、
system fontは`text.lr2SystemFont`に宣言サイズ・thickness・typeを保持します。
描画高はDSTの高さ、幅上限はDST幅×DST高/FONTサイズで、中央・右揃えもこの幅を使います。
typeのedge bitは黒の縁取りへ変換します。字形はBMZの既定フォントを使い、
DxLib固有のthicknessとアンチエイリアス方式の再現は未対応です。

LR2の`#IMAGE`は既定の緑（0,255,0）を透過色とし、`#TRANSCOLOR`と互換綴り
`#TRANSCLOLR` / `#TRANSCLOLOR`で以後の画像の透過色を変更できます。
RGB画像だけに適用し、alphaを持つ画像は元のalphaを維持します。
画像ごとの`source.lr2ColorKey`をCPU/GPUキャッシュにも含めます。DXA内の画像にも適用します。

LR2 Playのnumber ref=120..136は2Pの実プレイ状態へ接続します（127は既存のゲージ変換）。
内部ref=19220..19236を予約し、beatorajaのtarget関連refと分離します。
122/123は判定済みノート基準の率、135/136は全ノート基準の率、128は2P EX−1P EXです。
120はLR2判定配点からのスコア確定値を表示します（LR2の数値カウントアップ補間は含まない）。

LR2の`SRC_AUTO_*`は`note.lr2Auto`へ分離し、部分AUTO入力の対象レーンだけに使います。
全体AUTOPLAYとG-BATTLEの表示専用側は通常画像を使います。AUTO画像がない箇所は通常画像へ戻ります。
`DST_NOTE`は`note.lr2Dst`に全行を保持し、時計・条件・座標・大きさを毎フレーム評価します。
初期レーン高をスクロール距離の基準に保ち、判定位置の移動による速度変化を避けます。
ノーツRGB/角度を反映し、blendとalphaはLR2のplay-area描画と同様に通常合成・ゲーム側alphaを使います。

LR2 CSVから変換したdestinationは、各dst行の`lr2Style`にblend/filter/centerを保持し、
その区間の開始行を使用します。accも区間ごとに評価し、1は三次加速、2は三次減速、
0は線形、その他は区間開始値を維持します。JSON/Luaのacc規則は変更しません。
画像のSRC/DSTタイマーが一致する場合、SRC cycleは最初のDST時刻を原点にします。
数字のcycleはSRCタイマーを使い、DST開始時刻は差し引きません。

LR2の画像合成`blend=3`は減算を使います。描画先RGBからsource RGB×alphaを引き、
描画先alphaは保持します。`blend=2`の加算、`blend=4`の乗算と区別します。

## LR2のDXAアセット

旧DXArchiveのファイル形式バージョン1〜4を直接読み込みます。KCOOL 1.72の
`barfont.dxa` / `SystemFont.dxa` / `title.dxa`はバージョン3です。
例えば`Font/barfont/font.lr2font`を`Font/barfont.dxa`内の`font.lr2font`として解決します。
CSVやアーカイブを書き換えず、展開フォルダーも作成しません。

- フォント定義（`.lr2font` / `.fnt`）と参照画像、スキンの静止画像を対象とします。
  ベクターフォントも既存の対応拡張子で読み込めます。動画・音声・LuaモジュールやCSV includeの
  アーカイブ内読込、アーカイブ内wildcard列挙は対象外です。
- 通常ファイルを優先し、存在しない場合に同名フォルダーの`.dxa`を探索します。
  ASCIIの大文字小文字を区別せず、旧形式のShift_JISファイル名に対応します。
  v4のコードページは932 / 65001に対応します。
- v1/v2の反転、v3/v4の既定キー、LZ圧縮に対応します。独自キー付きアーカイブとv5以降は未対応です。
  形式バージョンはDxaEncode.exeの製品バージョンとは異なります。
- font/sourceキャッシュは仮想ファイル名を区別し、背後の`.dxa`の更新日時・サイズを追跡します。
  フォントの参照画像もキャッシュ依存に含めます。
- Luaの既存`SkinPathContext`制約を維持し、アーカイブ実体の正規化パスが許可root外なら拒否します。
  フォントの参照画像にも同じroot制約を適用します。JSON/CSVのパス解決はこの制約の対象外とします。
- 索引16MiB、1ファイルの格納・展開サイズ256MiB、10万entry、ディレクトリ深さ64に制限します。
  不正なoffset・循環参照・展開範囲外のLZ参照をエラーにします。

読込はOS固有ライブラリに依存しない`bmz-skin-assets`で行い、画像decodeとGPU uploadは
既存のrenderer / app側経路を使います。

## LR2 Play 表示参照

LR2 play skinの`SRC_BUTTON 40/41`は、1P/2Pの実際に有効なゲージ種類を
LR2の画像順（NORMAL / HARD / HAZARD / EASY / P-ATTACK / G-ATTACK）へ変換します。
ASSIST EASYはEASY、EX HARDと段位ゲージはHARDの画像へフォールバックします。
表示用refとクリック用actは分離します。HS-FIX (`55`)は分離の対象外で、同じ番号を参照します。

`SRC_NUMBER 10/11`は共通の現在HSを100倍して整数化します（例: `2.50 → 250`）。
桁数はスキンの指定を維持するため、3桁欄ではHS 10以上の上位桁は表示できません。
`127`は2Pゲージ残量の整数部です。BATTLE skin type `12/13`の`121`は2P実EX SCOREを
参照し、ターゲットスコアの有無に依存しません。通常playの`121`とJSON/Luaの参照には
この扱いを適用しません。2P不在の数値は非表示とし、2Pの残量・スコアが0の場合は0を表示します。

## Selectのクリア状態・IR統計

Selectのoption `100..105` / `1100..1104`は選択中スコアのクリア状態を参照します。
Luaのロード時にもoption `50/51`とtext `1020`へprimary IRの接続設定を渡すため、
接続の有無で生成するimageset等を切り替えるスキンも利用できます。

IRのクリア種別人数・割合（number `202..219`, `222..242`）は、全件取得が確認できる
ランキングから計算します。フルコンボ人数はFULLCOMBO/PERFECT/MAXの合計で、割合は
整数部と小数第1位を提供します。一部の順位だけ取得した場合、人数や小数部分は非表示です。
提供元の整数クリア率がある場合は`227`に表示し、人数への逆算は行いません。

Selectのnumber `350/351`は鍵盤の通常ノーツ/LN数で、スクラッチ/BSS (`352/353`)を
含めません。END密度は時刻順のノーツ分布とTOTALから求め、保存済み分布の読み込み時にも
再計算してキャッシュするため、この表示修正のための曲再スキャンは不要です。

## 名前によるプロパティ参照

beatorajaの名前付きプロパティを、BMZが既に扱う数値IDの評価経路へ解決します。
名前は大文字小文字を区別し、上流の綴りをそのまま使用します。
対応表は上流の固定名512件と番号付きの名前を含みますが、名前からIDへの解決と
そのIDが表す値の実装は別です。未対応IDの意味を追加するものではありません。

| 入力 | 名前を解決する種類 |
|---|---|
| JSON / Luaの`value[].value`、`imageset[].value` | IntegerProperty（例: `score`） |
| JSON / Luaの`text[].value` | StringProperty（例: `title`） |
| JSON / Luaの`slider[].value`、`graph[].value` | RateType（例: `music_progress`） |
| destinationの`op`、`draw` | BooleanProperty（例: `bgaon`、`!bgaon`） |
| `main_state.number` / `text` / `option`の引数 | 各Integer / String / BooleanProperty |
| `main_state.float_number`の引数 | FloatPropertyの番号付き名、FloatType、RateTypeの順 |

`ref`は数値です。特に`imageset.ref`のIndexTypeと
`imageset.value`のIntegerPropertyは異なる種類として扱います。
`imageset.value`が負なら非表示、画像数以上なら先頭画像を選びます。
JSONのFloatPropertyにあたる`slider.value` / `graph.value`はRateTypeだけを使い、
Luaの`main_state.float_number`が扱うFloatType名へは拡張しません。

Boolean名の先頭の`!`は1個ごとに否定します。未知名は否定してもtrueにしません。
`op`配列は数値と文字列を混在でき、各条件をANDで評価します。
文字列内のOR等はその条件の中で評価し、隣の条件とは結合しません。
型付き`value`や条件の未知文字列は既存の式評価へ渡します。
Luaの`main_state`への未知名は式として実行せず、number / float_numberは0、
textは空文字、optionはfalseを返します。既存の数値・数値文字列引数も使用できます。
ロード時の依存値捕捉と永続VMの実行時評価は同じ名前解決を使用します。

`main_state.numbers(...)`は各引数を`number`と同じ規則で解決し、同じ順序の
**複数戻り値**を返します。テーブルではありません。数値IDと名前を混在でき、
未知名は0、取得先の欠損sentinel（`-2147483648`等）はそのまま返します。
引数なしなら戻り値も0個です。テーブルが必要なら`{ main_state.numbers(...) }`とします。

```lua
local state = require("main_state")
local score, best = state.numbers("score", 150)
```

callback内で呼ぶと現在フレームの値を取得します。`local numbers = state.numbers`
のように関数を捕捉しても追従します。ロード時に取得して変数へ保存した数値自体は更新されません。

対応表は[生成スクリプト](../scripts/generate_skin_property_names.py)で
`.local/beatoraja`の参照ソースから生成しています。
`python scripts/generate_skin_property_names.py --check`で参照ソースとの一致を確認できます。
通常のビルドではチェックイン済みの表を使い、Javaや参照チェックアウトを必要としません。
`floatvalue`等の新しいスキーマやLua APIはこの対応には含みません。

BMZ拡張の名前（`bmz_`で始まる名前）は生成表とは別の表で解決し、生成表に同名がある場合は
beatorajaの名前を優先します。現在は[BMZ Chart Text Refs](#bmz-chart-text-refs)の名前があります。

## 最終プレイ日時の数値ref

| ref | 名前 | 値 |
|---:|---|---|
| 243 | `lastplay_timestamp` | Unix秒です。1..2147483647 |
| 244 | `lastplay_year` | ローカル時刻の年 |
| 245 | `lastplay_month` | 月、1..12 |
| 246 | `lastplay_day` | 日、1..31 |
| 247 | `lastplay_hour` | 時、0..23 |
| 248 | `lastplay_minute` | 分、0..59 |
| 249 | `lastplay_second` | 秒、0..59 |

Selectでは選択中の譜面またはコースの最終保存プレイ日時を返します。BEST更新が
なくても更新し、Failedや履歴を作らないclear-only保存も含みます。Resultでは
今回保存したattemptの日時です。Autoplay、Replay、Practice、保存禁止時のResult、
フォルダ、Play / Decideで日時がない場合は欠損です。

欠損はLuaの`number` / `numbers`では`-2147483648`、数字画像では非表示です。
`243`は2038年の32bit上限を超えると欠損になりますが、`244..249`はOSが
ローカル日時へ変換できる限り引き続き取得できます。値は起動環境のタイムゾーンを使います。
`243`が`2147483647`（INT_MAX）ちょうどのときも、数字画像は既存のsentinel契約で
非表示になります。Luaの`number` / `numbers`はその値を返します。
`main_state.score_date_sec_time()`は選択中の**BESTの日時**を返し、
この最終日時とは分離します。保存・既存DB移行の詳細は[score-persistence.md](score-persistence.md)を参照します。

```lua
local state = require("main_state")
local function last_play_text()
    local y, m, d = state.numbers("lastplay_year", "lastplay_month", "lastplay_day")
    if y == -2147483648 then return "プレイ記録なし" end
    return string.format("%04d/%02d/%02d", y, m, d)
end
```

選曲変更に追従する表示は`text[].value`などのcallback内で取得します。
ロード時はそのロード要求の日時を使い、選択情報のない初期ロードは欠損です。

## Lua Runtime Compatibility Mode

`main_state.screen_width()` / `main_state.screen_height()`は現在の描画先の物理pixel数を
整数で返します。Select / Decide / Play / Resultで共通です。スキンの`w` / `h`、
DPI補正後の論理寸法、内部描画解像度とは別で、動画exportでは出力動画の幅・高さになります。
描画先が未接続のときは0です。

headerと本体のロードは同じ寸法を使い、callback内では現在の寸法を参照します。
`local width = main_state.screen_width`と関数を捕捉してもリサイズへ追従します。
ロード時に寸法を読んで配置を構築したskinだけ、寸法変更時に再decodeします。
runtime callback内でだけ読むskinは再decodeせず、closure stateを維持します。
初回window生成前のロードで寸法に依存した場合やLuaロードに失敗した場合は、
描画先の確定後に再試行します。非同期ロードの古い寸法の結果は適用しません。
単独のheader走査・変換など描画先のない用途では0も扱えるように記述してください。

通常の`auto`モードはLua functionをロード時に宣言的なref/式へ変換し、推論できない
対応済みfieldだけ永続Lua VMのruntime callbackへ残します。スキン開発・beatoraja比較では
`--lua-skin-runtime compat`を指定すると、次のfunction fieldを推論せず実行時に評価します。

- destination `draw`
- `value[]`, `text[]`, `graph[]`, `slider[]`の`value`
- `customTimers[].timer`（ID `10000..19999`）

数字のruntime `value`はbeatorajaの`SkinNumber.prepare`と同様、destination順に、
その数字の`draw` / timer判定より先に評価します。同じvalueを複数destinationで使う場合も
destinationごとに1回評価し、その結果を描画・数値画像cacheへ渡します。非表示やcache hitでも
Luaの状態更新を省略せず、同じdestination内で二重実行しません。失敗・非有限値・整数の
非表示sentinelでは数字を描画しません。
描画用の構造cacheはdestinationとobject・検索欄textの対応を保持し、条件・timer・
callbackの結果は固定しません。同じIDを複数定義した場合の各objectの優先順位も維持します。
LR2数値画像のcacheは経過msではなく実際のsource frameを使い、値・座標・色・回転・
LR2のblend/filter/center・素材が変わると作り直します。1フレーム素材にcycleが指定されても
同じ画像を再利用できます。clipは数値画像のcache外で毎フレーム適用します。
Luxe Flat Result型の`rank_diff_count` / `rank_diff_*`が共有`rank_plus`を捕捉する場合、
value / drawは`auto`でもLua実行を維持し、
ランク差分の戻り値だけを組み込み式へ置き換えて`scorerate`等の更新を失わないようにします。

custom timerは描画前に宣言順で1フレーム1回更新します。`auto`では固定遅延・別名の
推論を維持し、推論できないtimerを永続VMへ残します。VMを使うスキンでは推論済みの
custom timerも同じVMで更新し、先行するtimerの変更を同じフレームで参照できます。
`main_state.set_timer(id, start_us)`は受動custom timerをON/OFFにします。組み込みtimerの
書き換えは拒否し、functionを持つ能動timerへの書き込みは無視します。
Luaの`main_state.timer(id)`はシーン基準の開始時刻（マイクロ秒）、`main_state.time()`は
現在時刻（マイクロ秒）を返します。描画側では経過ミリ秒へ変換します。OFF判定には
`main_state.timer_off_value`を使います。timer callbackのnilはbeatorajaと同じく開始時刻0、
例外・非有限値・命令数上限超過はOFFとして扱い、失敗の診断はcallbackごとに1回出します。

次の参照用helperも同じtimerと時計を使います。組み込みtimerとcustom timerの両方を扱い、
同一フレーム内の先行callbackや`set_timer`による更新も反映します。

| API | 戻り値 |
|---|---|
| `main_state.timer_is_on(id)` | ONならtrueです。開始時刻0もON |
| `main_state.timer_is_off(id)` | OFFならtrueです。未知timerもOFF |
| `main_state.timer_elapsed(id)` | 現在時刻から開始時刻を引いた整数マイクロ秒です。OFFなら-1 |
| `main_state.timer_elapsed_ms(id)` | 同じ経過時間を整数ミリ秒で返します。0方向へ切り捨てします。OFFなら-1 |
| `main_state.timer_elapsed_seconds(id)` | 同じ経過時間を小数付き秒で返します。OFFなら-1 |

開始時刻が未来なら経過時間は負になるため、ON/OFFを経過時間の符号だけで判定しません。
これらは`auto` / `compat`とも現在の状態を参照し、32bit範囲を超える時計でも動作します。
既存のBMZの`timer_off_value`（`-2147483648`）は変更しません。

`timer_util.timer_function` / `timer_observe_boolean` / `new_passive_timer`をcustom timer内で
使う場合も実行時の時計と状態を参照します。destinationへ直接指定した任意timer functionや
`customEvents`の汎用runtime実行はこの対応には含まれません。

Luaファイル全体をフレームごとに再ロードするのではなく、ロード時に作成したclosureと
module stateを保持します。callbackからは現在フレームの`main_state`を参照できます。
各callbackの命令数上限、フレーム全体の命令数上限、Lua VMのメモリ上限を適用し、失敗時は
draw=false、数値/文字列は未取得として扱います。runtime callbackを含むdocumentはVMをclone
できないためdocument cacheとLua-to-JSON変換の対象外になります。

Resultのパネル切替は、推論済みの`result_panel(...)`とLua runtime callbackで同じ
現在状態を参照します。既存のパネル互換処理が認識するグローバル`Expand_op`
（0=非表示、1=IR、2=GRAPH）と、callbackが直接保持するローカル`result_mode`
（0=GRAPH、1=IR）を評価中に同期します。GRAPH/IRを表示しない状態では後者に2を渡します。
`auto` / `compat`の両方で、配置設定や別のローカル変数を含む複合条件を維持します。
Lua評価が入れ子になった場合や例外時には元のパネル値を復元し、他のclosure stateは
リセットしません。これは既存パネル切替の互換処理であり、任意の`act` / `customEvents`
を実行する汎用イベントruntimeではありません。

Lua sandboxはロード時の互換APIとして`os.clock` / `os.date` / `os.time`を提供します。
`os.time`の日付tableは標準Luaと同様にローカル時刻として正規化し、正規化後の値を
tableへ書き戻します。引数なしでは現在時刻を返し、`os.date`は`!` prefixでUTCを選べます。libGDXの
`Controllers.getControllers()`は入力のない空配列として`size` / `get()` / `first()`を
提供します。`io`の書き込みstubは内容を永続化しません。初回document構築は、蓄積データを
Luaで解析するResult skinも通せる専用の命令数上限を持ち、runtime callback単位の上限は
それより小さい値とします。

判定の存在を表すoption `2241..2246` (PGREAT / GREAT / GOOD / BAD / POOR / EMPTY POOR)
はリザルト画面だけでなくプレイ中も現在の判定数を参照します。これにより、LITONE11などが
`-2244` / `-2245`で制御するフルコンボ演出はBAD/POOR発生後に表示されません。

skin catalogのJSON候補はinclude・条件・数値正規化後に数値の`type`を明示した文書だけを受理します。
これによりskin配下のplayer dataやparts JSONが既定値`type=0`のPlay 7KEYS skinとして
候補へ混入することを防ぎます。Selectの曲行テキストIDは区切り文字を無視して`bartext`を
照合し、`bar_text` / `folderbar_text`形式もbar textへ割り当てます。Lua skinのslider / graph
で使う`isRefNum`はJSONのbooleanに加えて互換表現の整数`0` / `1`も受理します。

### カタログとシステム音セットの走査

スキンカタログとBGM / SEセットの再帰走査では、走査rootごとにcanonical pathで
ディレクトリの実体を記録し、同じ実体を一度だけ読みます。symlinkとWindows junctionによる
自己・親への循環や、同じディレクトリの別名による候補の重複を防ぎます。
root自体がリンクでも走査し、候補のパスには最初に到達した元の経路を使います。
複数の別名がある場合にどの経路が残るかは走査順によります。
canonical pathを取得できないディレクトリは走査しません。

スキンの`resource:skins/` / `data:skins/`の相対パスとorigin、名前順の候補ソートは維持します。
resourceとdataのrootが異なる場合はそれぞれ独立して走査します。
サウンドセットは対応するBGM / SE音源が最初に見つかったディレクトリをルートとし、
その直下の音源付きフォルダをバリエーションとして1つ選びます。補完用SEセットは`clear`マーカーで検出します。
対応する固定名と`.wav` / `.ogg` / `.flac` / `.mp3`はASCIIの大文字小文字を区別せず検出します。
親子の補完、RESULT BGMと全SEの優先選択、`.loop`指定は[サウンドセット仕様](system-sound.md)を参照します。
`bgm_dir` / `se_dir`設定キーは維持します。親子両方に音源がある構成は、独立した複数セットではなく1セットとして扱います。

### Lua Module Path

skin library root配下のLua skinへ公開する初期`package.path`は、過去のbeatorajaと同じ
`?.lua;skin/<skin entry directory>/?.lua`形式にします。sandbox内の実ファイル解決はこの論理
パスをlibrary rootに対応付け、`require` / `dofile` / `loadfile`がroot外へ出ないことを
別途検証します。library rootを持たない低レベルAPIは、entry directoryの絶対
パスを使うfallbackで解決します。

## Skin Type

beatoraja互換の主なplay skin type:

| type | meaning |
| ---: | --- |
| 0 | 7KEYS |
| 1 | 5KEYS |
| 2 | 14KEYS |
| 3 | 10KEYS |
| 4 | 9KEYS |
| 12 | 7KEYS BATTLE |
| 13 | 5KEYS BATTLE |
| 14 | 9KEYS BATTLE |
| 16 | 24KEYS |
| 17 | 24KEYS DOUBLE |
| 18 | 24KEYS BATTLE |

BMZ独自拡張:

| type | meaning | status |
| ---: | --- | --- |
| 19 | reserved | 未使用 |
| 20 | reserved | 未使用 |
| 21 | 2KEYS | 予約のみ |
| 22 | 4KEYS | 実装済み |
| 23 | 6KEYS | 実装済み |
| 24 | 8KEYS | 実装済み |

`22=4KEYS`, `23=6KEYS`, `24=8KEYS`はScratchなしの`Key1..KeyN`用play skin typeです。
beatorajaには対応するskin typeが無いため、BMZ専用skinとして扱います。

`#8K` BMSのU_E/BMSE系channelは、7Kの表示順`S1234567`を8Kの
`Key1..Key8`として扱います。つまり`Scratch -> Key1`, `Key1 -> Key2`, ...,
`Key7 -> Key8`に正規化してからskinへ渡します。

## Battle Session Skin

`SessionMode::AutoplayBattle` / `SessionMode::GBattle`では、5Kはskin type `13`、7Kは
skin type `12`の専用スロット (`skin.battle5` / `skin.battle7`)を優先します。
未設定なら同じ譜面モードの通常スロット (`skin.play5` / `skin.play7`)へフォールバックします。
5K/7K以外のG-BATTLEは通常play skinを使います。

SessionModeのbattleは、Select上のkey modeとスキン選択キーを5K/7Kのまま維持します。
Play内部ではbattle skinへ2P側レーンと対戦表示を公開するが、通常のDP OPTION `BATTLE`の
10K/14Kスキン選択とは区別します。

2P側のノート、LN、key-on/off、hold、bomb、HCN、judgeは通常の2Pレーンref/timerで
公開します。次の対になっているtimerは1P/2Pを別状態として扱います。

| 1P | 2P | meaning |
| ---: | ---: | --- |
| 42 | 43 | gauge increase |
| 44 | 45 | gauge max |
| 46 | 47 | judge |
| 446 | 447 | combo |
| 48 | 49 | full combo |
| 143 | 144 | end of note |

対戦側の進行中EX score / max combo / BP / judge countsは、beatorajaのrival number
ref `271`, `275`, `276`, `280..284`に公開し、judge rateは`285..289`に公開します。
G-BATTLEの相手判定は
表示専用で、1P側のスコア、ゲージ、キー音、保存用リプレイには混ぜません。

## Static Image Sources

beatoraja skinの`source.path`はPNG / BMP / JPEG / GIF / TGAに加え、libGDX
`PixmapIO`のCIMを読み込めます。CIMはzlib stream内のwidth / height /
Gdx2DPixmap formatとpixel bufferをRGBA8へ展開します。MILLIONDOLLAR RESULTの
主要atlasは配布時点から`.cim`のため、PNG fallbackへ書き換えずそのまま扱います。

## Play Gauge Source Layout

`gauge.nodes`が4 / 8 / 12枚だけ定義されたskinは、beatorajaの
`JsonSkinObjectLoader`と同じindex mapで36段階へ展開します。これによりgauge typeごとの
通常色・警告色・点滅色を取り違えずに表示します。36枚定義はそのまま直接対応させます。

## PMchara

JSON / Lua play skinの`pmchara`をdecodeし、sourceが指すディレクトリまたは`.chp`を
Shift_JISで読み込みます。type `0..15`、`Pattern` / `Texture` / `Layer`、frame時間、loop開始、
source / destination矩形、alpha、angleを通常のskin sourceとrender itemへ展開します。
type 0は通常・判定・曲終了状態からモーションを選び、type 1..15はbeatorajaの固定
モーション対応を使います。filepathのユーザ選択と既定値は通常画像と同じ優先順位で解決します。

現状はPMcharaの`--`座標補間と、画像右下色を透過色にするchroma-key処理には未対応です。
PNG等が持つalphaは通常どおり反映します。

## Text opacity

BMZでは、解決済みdestinationの`a`を文字本体・縁取り・影に共通して適用します。
`outlineColor` / `shadowColor`自身のαには、この共通透明度を乗算します。
`a: 0`ではすべて透明になり、アニメーションやoffset、後段の画面フェードにも追従します。
これはBMZの統一方針であり、beatorajaの距離フィールド描画と厳密に同一の挙動ではありません。

## Text Overflow

beatoraja互換のtext `overflow`は次の値を使用します。

| `overflow` | behavior |
| ---: | --- |
| `0` | destination幅を超えて描画する |
| `1` | destination幅へ収まるよう縮小する |
| `2` | destination幅へ収まる位置で文字列を打ち切る |

`overflow: 1`の既定動作はbeatorajaと同じ横方向のみの縮小で、文字の高さとY位置を
維持します。BMZ拡張の`shrinkMode: 1`を指定すると、縦横等比縮小へ
切り替わり、縮小後のtextはdestination内で上下中央に配置されます。

| `shrinkMode` | behavior |
| ---: | --- |
| 省略 / `0` / その他 | 横方向のみ縮小（beatoraja互換） |
| `1` | 縦横等比縮小（BMZ拡張） |

`shrinkMode`は`overflow: 1`かつ`wrapping: false`の場合だけ参照します。
`wrapping: true`は縮小より優先されます。JSON skinとLua skinで同じフィールドを使用できます。

```json
{
  "text": [
    { "id": "beatoraja-title", "size": 30, "overflow": 1 },
    { "id": "uniform-title", "size": 30, "overflow": 1, "shrinkMode": 1 }
  ]
}
```

## BMZ Default JSON Skin

`data/skins/default/`のデフォルトスキンはJSON skin documentを主経路にします。
選曲 / 決定 / リザルトは`select.json`, `decide.json`, `result.json`、プレイ画面は
key modeごとに`play4.json` / `play5.json` / `play6.json` / `play7.json` /
`play8.json` / `play9.json` / `play10.json` / `play14.json`を読みます。

BMZ default JSONでは、digit atlasを同梱せずに既存フォントで数値を表示するため、
`text`要素にBMZ拡張の`numberRef`, `judgeRegion`, `judgeColor`, `judgeTimingRegion`,
`judgeTimingColor`, `prefix`, `suffix`を使えます。
`numberRef`は既存の`value.ref`と同じ`SkinDrawState` number refを文字列化し、
未取得時は空文字として扱います。外部beatoraja JSON skinの`value` sprite表示には影響しません。
`judgeRegion`は最新判定の表示領域index (通常は`0`)を文字列化し、判定タイマーが
非アクティブなら空文字として扱います。`judgeColor`は`judgeRegion`用の表示色を判定種別で
切り替え、PGREATは水色、GREAT / GOODは黄色、BAD / POOR / EMPTY POORは赤で表示します。
`judgeTimingRegion`は同じ判定領域のFAST / SLOWだけを文字列化し、`judgeTimingColor`で
FASTを青、SLOWを赤に切り替えます。

### Play Target Score Refs

beatoraja互換のターゲット数値とグラフは、最終値と進行値を区別します。

| 種類 | ID | 値 |
| --- | --- | --- |
| 数値 | `121` / `151` | 曲全体の最終ターゲットEX SCORE |
| 数値 | `153` | 現在EX SCORE − 現在のノート位置に対応するターゲット |
| グラフ | `114` | 現在のノート位置に対応するターゲット / 曲全体の最大EX SCORE |
| グラフ | `115` | 最終ターゲット / 曲全体の最大EX SCORE |

`121` / `151`は`value.ref`、`text.numberRef`、Luaの`main_state.number()`で同じ値を参照します。
ノート進行では変化しないが、IR取得等でターゲット自体が更新されれば新しい値を返します。
0点は有効な値です。未設定時は数値画像を非表示、`text.numberRef`を空文字、Luaを`0`とします。
総ノーツ0でも設定済みの最終ターゲットを返します。デフォルトスキンのTARGET表示は`numberRef: 121`を使います。
LR2 CSVのPlay ref `120..136`は読込時に2P専用内部refへ変換されるため、その`121`は2Pの現在EX SCOREを維持します。

### BMZ Arrange Refs

beatoraja互換の`ref` / `event_index` / `number` `42` (1P RANDOM)と`43` (2P RANDOM)
は既存スキン互換のため0..9の値を返します。BMZ独自ARRANGEの`F-RANDOM`と
`MF-RANDOM`は、非対応beatoraja skinでoption panelが崩れないよう、`42` / `43`
では`RANDOM`と同じ`2`として扱います。

BMZ対応skinで新ARRANGEを区別したい場合は、BMZ拡張refを使います。

| ref | meaning |
| ---: | --- |
| 344 | 1P ARRANGE extended index |
| 345 | 2P ARRANGE extended index |

extended indexはbeatoraja互換値`0=NORMAL`, `1=MIRROR`, `2=RANDOM`, `3=R-RANDOM`,
`4=S-RANDOM`, `5=SPIRAL`, `6=H-RANDOM`, `7=ALL-SCR`, `8=RANDOM-EX`,
`9=S-RANDOM-EX`に加えて、`10=F-RANDOM`, `11=MF-RANDOM`を返します。

ADFX02/ECFNのBMZ拡張版も通常パネルの1P/2P選択枠に344/345を使い、元の領域へ
12種類を表示します。選択操作は既存の本体入力に任せ、eventの344/345へ置き換えません。

### BMZ Attempt Session Mode Ref

beatoraja互換のassist `ref` / `event_index` `73`はbeatorajaと同じ2値を返します。
`NORMAL` / `PRACTICE` / `G-BATTLE`は`0`、`AUTOPLAY` / `AUTOPLAY BATTLE`は`1`とし、
既存skinの2行option panelを崩しません。

BMZ対応skinで5種類を区別する場合は、BMZ拡張ref `1970`を使います。
`number(1970)` / `event_index(1970)`は`0=NORMAL`, `1=PRACTICE`, `2=AUTOPLAY`,
`3=AUTO BATTLE`, `4=G-BATTLE`を返します。BMZデフォルトスキンのplay mode panelも
このrefを使用します。Selectではこれから開始するモード、Decide / Play / Resultでは
試行開始時に固定したモードを返します。

### Practice Configuration Position

beatoraja JSON / Lua play skinの`practice`オブジェクトをdecodeします。`practice.id`と同じ
`destination.id`の先頭`dst`にある`x` / `y` / `h`を、BMZのプラクティス設定ウィンドウの
初期位置として使います。指定がないskinでは画面左上に配置します。設定内容はBMZのeguiウィンドウで
表示し、ウィンドウは初期位置からドラッグ移動できます。

### Play Gauge Type Ref

`ref=44`は、Lua skinのimageset `value`が`main_state.gauge_type()`を返す場合に、BMZが
JSONのimage indexへ変換するための内部bridgeです。beatorajaの公開NumberPropertyや
スコア互換Rule Modeではなく、image / imagesetの`ref`専用として扱います。値は現在適用中の
gauge typeで、`0=ASSIST EASY`, `1=EASY`, `2=NORMAL`, `3=HARD`, `4=EX HARD`,
`5=HAZARD`, `6=CLASS`, `7=EX CLASS`, `8=EX HARD CLASS`です。JSON `gauge.type`の
ゲージアニメーション種別とも別の値です。

### BMZ Rule Mode / LN Policy Refs

スコア互換ルール、選曲設定のLN方針、ScoreKeyへ正規化されたLN方針を別の状態として公開します。
JSON skinではnumber/value・imagesetの`ref`、textの`ref`、destinationの`op`から参照できます。
Lua skinでは`main_state.number()` / `event_index()` / `text()` / `option()`から同じ値を参照できます。

Rule Modeは次のindexとlabelを使います。

| index | text | exact option |
| ---: | --- | ---: |
| 0 | `BEATORAJA` | 1988 |
| 1 | `LR2ORAJA` | 1989 |
| 2 | `DX` | 1990 |

`ref=1987`はnumber / event index / imageset refでは上表のindex、textではlabelを返します。
Selectでは現在のprofile設定、Decide / Playでは開始する試行のScoreKey、Resultでは終了した
単曲またはコース試行に保存されたRule Modeを使います。Replayとコースでもprofileの現在値へ
読み替えず、試行開始時に固定された値を維持します。

LN方針のindexとlabelは、設定値と正規化済みScoreKeyで共通の次の順序を使います。

| index | text | setting exact option | score policy exact option |
| ---: | --- | ---: | ---: |
| 0 | `AUTO(LN)` | 19161 | 19151 |
| 1 | `AUTO(CN)` | 19162 | 19152 |
| 2 | `AUTO(HCN)` | 19163 | 19153 |
| 3 | `FORCE(LN)` | 19164 | 19154 |
| 4 | `FORCE(CN)` | 19165 | 19155 |
| 5 | `FORCE(HCN)` | 19166 | 19156 |

`ref=19160`はSelectで現在のprofileに設定された`LnPolicySetting`を返します。譜面内容による
AUTO解決前の値で、19167はAUTO系、19168はFORCE系のときtrueになります。Select以外では
settingのnumber / imageset ref / textは値なし、setting optionはすべてfalseになります。

`ref=19150`は譜面内容、コース制約、Replay情報を反映してScoreKeyへ正規化された
`LnScorePolicy`を返します。19157はAUTO系、19158はFORCE系、19159は値を取得できるときtrueになります。
Selectの曲行では選択譜面から正規化し、コース行ではコース共通ScoreKeyを使います。フォルダ・設定行
では値なしです。Decide / Play / Resultではその試行に固定されたScoreKeyを使います。値なしの場合、
number / imageset refは未取得、textは空文字、exact / AUTO / FORCE optionはfalseになります。
`event_index()`だけはAPI仕様上`0`へフォールバックするため、未取得とAUTO(LN)を区別する場合は
必ず`option(19159)`または`op: [19159]`を併用します。

既存のbeatoraja互換`ref=308`は、設定やScoreKeyではなく、変換後の実効譜面を描画する
`0=LN`, `1=CN`, `2=HCN`の3値を引き続き返します。AUTO/FORCEの区別には19160または19150を使います。

LNの有無はbeatoraja互換option `172=OPTION_NO_LN` / `173=OPTION_LN`で判定します。
Selectでは選択中のライブラリ譜面、Decide / PlayではLN policy適用後の開始譜面、Resultでは
終了した実効譜面を使います。フォルダ・設定行や開始譜面未確定時は172/173の両方がfalseになります。
Selectで個数も必要な場合は`ref=351`（通常LN数）と`ref=353`（ロングスクラッチ数）を使います。
`ref=308`はLNが存在しない場合でもLN種別indexを返し得るため、LN有無の判定には使いません。

### BMZ Score Grade Refs

ランク差分の標準表示は固定NEXTとします。`NUMBER_NEXT_RANK_EXSCORE` (`ref=154`)は、
次の正式なDJ LEVEL境界までの距離を`次境界 - 現在のEX SCORE`で返すため、未到達時は
正、MAX時は0になります。select / play / resultのいずれでも、現在のEX SCOREと譜面全体の
ノート数を使用します。スコアが無いselect行または総ノート数0では値を返しません。
Result / Course ResultのLuaロード時にも同じ計算値を渡します。ロード時に`number(154) == 0`
で満点用の数字画像を選ぶスキンでも、描画時の値と一致します。

総ノート数を`N`、MAX EX SCOREを`M=2N`とし、境界は整数演算で次のように求めます。

| grade | minimum EX SCORE |
| --- | ---: |
| F | `0` |
| E | `ceil(2M/9)` |
| D | `ceil(3M/9)` |
| C | `ceil(4M/9)` |
| B | `ceil(5M/9)` |
| A | `ceil(6M/9)` |
| AA | `ceil(7M/9)` |
| AAA | `ceil(8M/9)` |
| MAX | `M` |

beatorajaの実装に含まれる浮動小数点の丸め誤差、正式なDJ LEVELではない`1/9`境界、
play中だけ経過ノート数を差分量に使う挙動は引き継ぎません。現在値が境界と一致する場合、
NEXTはさらに上の境界を指します。MAXではMAXを指して差分0を返します。

BMZ拡張は次をselect / play / result skin共通で公開します。grade indexは
`0=F`, `1=E`, `2=D`, `3=C`, `4=B`, `5=A`, `6=AA`, `7=AAA`, `8=MAX`です。
1974〜1976はnumberに加えて同じ値をevent index / imageset refとして使用でき、textでは
grade labelを返します。

| ref / option | kind | meaning |
| ---: | --- | --- |
| 1974 | number / event index / text | 現在到達済みのgrade |
| 1975 | number / event index / text | 現在より高い次のgradeです。MAX時はMAX |
| 1976 | number / event index / text | 最も近いgrade |
| 1977 | number | `EX SCORE - current border` |
| 1978 | number | `EX SCORE - next border`です。符号付きです。正値の`ref=154`とは独立 |
| 1979 | number | `EX SCORE - nearest border`です。次側なら負 |
| 1980 | number | `abs(ref 1979)` |
| 1981 | option | NEARESTが現在側です。完全一致と同距離を含む |
| 1982 | option | NEARESTが次側 |
| 1983 | option | grade境界に完全一致 |
| 1984 | option | 現在側と次側から同距離 |
| 1985 | option | grade計算に必要なスコアが存在する |

NEARESTは現在側までの距離が次側以下なら現在側を選ぶため、同距離では現在側になります。
別のtie policyが必要なLua skinは1977と`abs(1978)`を比較して独自に選択できます。JSON skinは
1976をimageset ref、1980を差分値、1981 / 1982を符号画像のopとして利用できます。
スコア無しでimagesetの先頭画像へフォールバックさせたくない場合は、destinationに
`op: [1985]`を指定します。

削除済みの表示設定で使っていた1971〜1973は再利用しません。旧BMZ skin向けに
`number(1971)=1`, `option(1972)=false`, `option(1973)=true`の固定NEXT値を返します。

### First Play Compatibility and BMZ Option

Play / Result skinの初回プレイ値はbeatoraja互換として扱います。保存済みスコアが無い場合、
`NUMBER_HIGHSCORE` (`ref=150`)と`NUMBER_HIGHSCORE2` (`ref=170`)は`0`を返します。
Resultの前回ミスカウント (`ref=176`)とミスカウント差分 (`ref=178`)は通常のnumber表示では
非表示のまま、Luaの`main_state.number()`では`Integer.MIN_VALUE` (`-2147483648`)を返します。
そのため既存Lua skinは`main_state.number(176) < 0`で初回を判定できます。Resultの前回ランクは
初回時にFとして扱い、option `327`が有効になります。

値が`0`の保存済みスコアと未プレイを明確に区別するため、BMZ拡張option `1986`を公開します。
Play開始時に現在のScoreKey（SHA256、LN方針、DP区分、rule mode）に対応する保存済みベストが
存在しなければtrueになります。Resultでも保存前の状態を維持します。JSON skinは`op: [1986]`、
Lua skinは`main_state.option(1986)`で参照でき、`op: [-1986]`は既プレイ時に有効になります。

### Result Autoplay Options

beatorajaの`OPTION_AUTOPLAYOFF` (`32`) / `OPTION_AUTOPLAYON` (`33`)はプレイ画面でのみ
評価されるが、BMZではAUTO PLAYでもリザルト画面を表示するため、Result skinにも拡張して
公開します。AUTO PLAYのResultでは33がtrue、32がfalseになり、通常プレイでは逆になります。
JSON skinは`op: [33]`、Lua skinは`main_state.option(33)`でAUTO PLAY専用表示を定義できます。

### Select Rival and Chart Replication

beatoraja互換の選曲イベントを次のように扱います。

| event id | action |
| ---: | --- |
| 10 | 難易度フィルターを巡回（309と同じ） |
| 14 | F1メニュー（BMZ メニュー）のスキンページを開く |
| 17 | 選択譜面フォルダの`.txt`を開く |
| 57 | HS倍率を設定刻みで増減 |
| 59 | ノーツ表示時間を1msずつ増減（`1..=10000`） |
| 74 | キーモード別の表示タイミングを1msずつ増減（`-500..=500`） |
| 79 | primary IRに属するライバルを`NONE → 登録順`で切替 |
| 210 | 選択譜面 / コースのprimary IRページを開く |
| 211 | 現在フォルダをF5相当で更新 |
| 212 | 選択譜面または通常曲フォルダをファイルブラウザで開く |
| 213 | 選択譜面の`url` / `appendurl`をブラウザで開く |
| 343 | GUIDE SEを切替 |
| 344 | `NONE → RIVALCHART → RIVALOPTION`で譜面再現モードを切替 |
| 350 | EXTRA NOTE DEPTHを4状態で巡回 |
| 351 | MINEモディファイアを5状態で巡回 |
| 352 | SCROLLモディファイアを3状態で巡回 |
| 353 | LONGNOTEモディファイアを6状態で巡回 |
| 400 | CONSTANTを切替 |

イベント引数が正なら増加 / 次、負なら減少 / 前として扱います。`event_index(10)`、
`event_index(343)`、`event_index(350..353)`、`event_index(400)`と同じ番号の
image / imageset refから現在値を取得でき、option 400はCONSTANT有効時にtrueとなります。
イベント360/361（7→9KEY変換）は未対応です。

イベント79で選んだライバルに選択譜面の
スコアがあれば、プレイ開始時のターゲットへ自動設定します。未プレイ譜面では通常の
TARGET設定を使います。名前の表示と比較スコアの有無は独立して扱います。

### ターゲット名のtext ref (BMZ仕様)

Select / Play / Resultで次の意味に統一します。beatorajaのPlay / Resultでref 1とref 3が
同じ名前を返す仕様とは異なります。

| ref | 内容 |
| --- | --- |
| 1 (`STRING_RIVAL`) | 選択中ライバルの名前を優先し、なければ確定したターゲットの名前です。どちらもなければ空文字 |
| 3 (`STRING_TARGET`) | 選択中のターゲット設定名 (`RANK AAA`, `IR TOP`, `IR NEXT`等)です。未選択は`NO TARGET` |

ref 1の選択ライバル名は、その譜面のスコアがなくても維持します。G-BATTLEではそのプレイで
選択した対戦相手を優先します。固定ランク目標は設定時に名前が確定し、IR系目標は取得した
ランキングの対象者名を使います。IR未取得・取得失敗・該当データなしの場合、選択ライバルが
なければref 1は空文字のままにします。名前とEXスコアは同じランキング行から解決し、
取得がプレイ開始に間に合わなかった場合も表示へ反映します。
Resultはプレイ時の名前とターゲット設定を保持し、リザルトのランキング取得や選曲側の
設定変更で置き換えません。JSON refとLuaの`main_state.text`は同じ規則で評価します。

譜面再現モード名はbeatoraja互換の`STRING_CHART_REPLICATION_MODE` (`ref=86`)と、
BMZの動的text id `bmz_select_chart_replication`から`NONE` / `RIVALCHART` /
`RIVALOPTION`として取得できます。`RIVALOPTION`は配置種別だけ、`RIVALCHART`は配置種別と
ライバルのside別24bit seedを適用します。ghostは使用しません。

### BMZ Dynamic Select Option Panels

選曲オプションの現在値は、状態ごとの画像セルを用意せず`text.id`から直接描画できます。
ラベルはselect snapshotの文字列を使うため、ARRANGEなどへ項目を追加しても
skin側のスプライト行追加は不要です。

| text id | value |
| --- | --- |
| `bmz_select_arrange` / `bmz_select_arrange_2p` | 1P / 2P ARRANGE |
| `bmz_select_target` | target |
| `bmz_select_gauge` | gauge |
| `bmz_select_gauge_auto_shift` | gauge auto shift |
| `bmz_select_bottom_shiftable_gauge` | bottom shiftable gauge |
| `bmz_select_double_option` | DP option |
| `bmz_select_hs_fix` | hi-speed fix |
| `bmz_select_assist` | assist |
| `bmz_select_mode` | key filter (`ALL KEYS` / `7KEYS` / `5KEYS` / `14KEYS` / `10KEYS` / `9KEYS` / `4KEYS` / `6KEYS` / `8KEYS`) |
| `bmz_select_sort` | sort |
| `bmz_select_ln_mode` | LN mode |
| `bmz_select_bga` | BGA |
| `bmz_select_chart_replication` | chart replication (`NONE` / `RIVALCHART` / `RIVALOPTION`) |
| `bmz_select_judge_timing_auto_adjust` | judge timing auto adjust (`ON` / `OFF`) |

ADFX02/ECFNの基本拡張は、上部のモードボタンに`bmz_select_mode`を使い、4K/6K/8Kも
実際のフィルター名で表示します。透明なクリック領域は既存event 11を維持します。
LNボタンのref/event 308はそのままに、option 19168でFORCE表示を重ねます。
基本拡張は`select/bmz_select_extensions.lua`の正常構築後だけ適用し、experimental詳細パネルの
設定・対応宣言から独立します。新しい公開IDは追加しません。

BMZ拡張の`panel`は画像を使わない単色矩形で、`color`, `borderColor`は
`RRGGBB` / `RRGGBBAA`、`borderWidth`はskin canvas pixelで指定します。
destinationに`act` / `click`を置くと、textやpanelもimage / imagesetと同じ
イベント対象になります。`clickable: false`はdestination自身と同名image / imagesetの
クリックを明示的に無効化します。

中央・右揃えtextのdestination `x`は描画上の基準点だが、イベント領域の`x`は
常に左端として扱います。操作可能な中央揃えtextは、描画用textと透明panelのdestinationを
分け、panel側へ元のセル矩形と`act` / `click`を指定します。

```json
{
  "panel": [{
    "id": "arrange-hit",
    "color": "00000000"
  }],
  "text": [{
    "id": "bmz_select_arrange",
    "font": "option-font",
    "size": 18,
    "align": 1,
    "overflow": 1
  }],
  "destination": [
    {
      "id": "bmz_select_arrange",
      "dst": [{ "x": 93, "y": 10, "w": 166, "h": 19 }]
    },
    {
      "id": "arrange-hit",
      "act": 42,
      "click": 2,
      "dst": [{ "x": 10, "y": 10, "w": 166, "h": 19 }]
    }
  ]
}
```

### BMZ Select DETAIL OPTIONS v1

新詳細パネル（互換panel 2）の操作状態を公開します。experimental ON時の物理入力はE1開閉 / E2切替です。設定そのものの既存ref/eventと独立した名前空間で、
既存event 301..307をカーソル操作に読み替えません。event 302はSCROLL REMOVEの
トグルであり、表示時間制御のCONSTANTはevent 400のままです。
項目・適用条件・操作設計は[DETAIL OPTIONS仕様](select-detail-options.md)を参照します。

「BMZ メニュー → 選曲 → 実験的な詳細オプション」（`[select].experimental_detail_options`、既定false）がONかつ、JSON documentのルート
（Luaでは返却するskin table）で`"bmzDetailOptions": 1`と`"bmzDetailOptionsNumbers": true`を
明示した選曲スキン（type 5）で、新詳細の表示と2パネル入力を有効にします。
Numbers宣言は数値行の現在値を表示でき、本体の数値編集操作に対応することを表します。
同梱3スキンは現在値のみを表示し、＋ / −ボタンは置きません。v1宣言だけの場合は3パネル方式の操作になります。

本体はselectの読込optionに予約キー`bmz_detail_options`を文字列`"1"`/`"0"`で渡します。
Luaは`bmz.get_option("bmz_detail_options", "0")`を確認し、OFFなら旧Assist表示を残します。
カスタマイズpropertyとして登録・保存しません。本体が値を上書きし、キャッシュの識別にも含めます。
OFF→ON / ON→OFFは選曲スキンを再ロードします。JSONは新option 19300とその否定で新旧を区別できます。
OFF・未対応時はpanel 1/2/3をholdで開きます。ON・対応時はpanel 1/2だけを使い、
固定表示中でもoption/timerは実際に表示するパネルを表します。物理E1/E2のhold参照は変更しません。
宣言なし、0、未知のversionではbeatoraja式のAssist表示・固定7鍵操作・クリック操作を使い、
本体の新UIを重ねません。一つのrefの使用だけでは対応を推定しません。
スキンなしでは本体の3パネル方式のASSIST OPTIONSを使います。互換option 22、panel 2、開閉timerは変更しません。
未宣言時は新snapshotを生成せず、各ref/text/optionは非表示値、新eventはno-opとします。

対応スキンはoption 19300でパネルを出し、7可視項目をvalidで条件付け、選択項目を
selectedで区別します。行/列の配置はスキン側で決められます。デフォルト選曲スキンは
項目を横並び、選択肢を縦並びにします。状態文字列と説明・理由を表示します。
タイトル・操作ガイド等の表示と背景の透過はスキンの方針に合わせられるが、背後への入力は遮断します。
E2表示中は新eventだけをクリック対象とし、背後の行・旧event・sliderへの透過を遮断します。
非表示または別モーダル中は新eventを無視します。未宣言時はKEY1〜7を既存event 301〜307へ対応させます。

退場を描画するスキンはルートに`"bmzDetailOptionsClose": true`も宣言できます（既定false）。
E2から離れて300ms未満だけ、最後の項目snapshotと列のスクロール位置を描画用に保持します。
通常パネルへの切り替えでも保持し、退出側timer 32と登場側timer 21を同時に進めます。
この間はoption 19300と各ref/text/optionが保持データを表します。設定のeditableは設定自体の状態であり、
退場中に操作できることを意味しません。退出側の新eventは即座に受け付けなくなります。
他のパネルが開いていない場合は退場中のクリック・slider・ホイールも遮断します。
切り替え先の通常パネルがある場合は、現在のパネルを操作できます。
開くdestinationには`op: [19300, 22]`とtimer 22、閉じるdestinationには
`op: [19300, -22]`とtimer 32を使い、退場用クリック領域は作りません。
別パネルの表示を禁止条件に含めません。通常→詳細ではtimer 31による退場とtimer 22を併用します。
300ms経過、E2再オープン、フォーカス喪失、別モーダル・画面への遷移で保持表示を終了します。
途中で詳細→通常→パネルなしへ移っても詳細の退場時間は再起動しません。
保持期間後は下記の非表示値に戻ります。Close宣言なしの対応スキンでは、新snapshotは解放時に終了します。
未対応スキンの3パネル方式の開閉演出には影響しません。
互換option 22は詳細を閉じる / 通常へ切り替える時にfalse、timer 32は0msから開始し、それらの意味は変更しません。新規IDの追加はありません。

同梱のBMZ拡張版mz-selectもこのAPIを利用します。`default_detailoptions/parts.lua`の正常ロード時に
対応宣言を伝播し、旧optionpanel3/4のE2表示（閉じるtimer 32を含む）だけを除去します。
選択列と現在値は枠で示します。構成・フォールバックは[実装仕様](select-detail-options.md#mz-select)を参照してください。

Luxez-FlatのBMZ拡張版も同じAPIを利用します。`select_skinparts/default_detailoptions/parts.lua`の
正常ロード時だけ対応宣言し、旧`default_optionpanel`内のE2開閉destinationとtimerなしのE2 hover領域を置換します。
E1/E1+E2は同じ画像IDを共用する場合もあるため、ID全体で除去しません。
部品の無効・欠落・失敗時とBMZ以外では3パネル方式のAssistを使います。新規APIの採番はありません。
[表示・互換仕様](select-detail-options.md#luxez-flat)を参照してください。

別途導入するADFX02/ECFNの拡張版も同じAPIを使います。`select/bmz_detail_options.lua`の正常構築後に
v1・Numbers・Closeを宣言します。旧timer 22表示のみを置換し、200msフェードと通常パネルの退出を追加します。
新しい公開IDは追加しません。既存timer 21/22/31/32とdraw callbackで暗幕を1枚に保ちます。
実験的な詳細オプションOFF・部品欠落・失敗時とBMZ以外では旧3パネルを保持します。
[表示・互換仕様](select-detail-options.md#adfx02--ecfn)を参照してください。

#### 選択項目のnumber ref

| ID | 意味 |
|---|---|
| 19300 | CURSORです。表示順の0-based位置 |
| 19301 | ITEM_COUNTです。全項目数 |
| 19302 | ITEM_IDです。表示順と独立した安定ID |
| 19303 | CATEGORY_IDです。1=LANE / HI-SPEED、2=GAUGE、4=LONG NOTE、5=ASSIST / MODIFIER、6=BGA、7=JUDGE。3（旧HI-SPEED）は欠番 |
| 19304 | SCOPEです。0=profile共通、4/5/6/7/8/9/10/14=編集対象キーモード、-1=未解決 |
| 19305 | VALUEです。保存設定の値です。boolは0/1、enumは下記の明示コード、numberは数値そのもの |
| 19306 | VALUE_INDEXです。bool/enumの0-based選択肢indexです。number、未取得、候補外値は-1 |
| 19307 | CHOICE_COUNTです。boolは2、enumは候補数、numberは0 |
| 19308 / 19309 / 19310 | numberのMIN / MAX / STEPです。他の型は-1 |
| 19311 | VIEWPORT_STARTです。環状配置の仮想slot 0に対応する0-based位置です。停止時は`(CURSOR + ITEM_COUNT - 3 % ITEM_COUNT) % ITEM_COUNT`（0項目時は0）。移動中は整数列分の残り変位を引いた表示窓に対応します |
| 19312 | ROW_SLOTSです。v1は7（項目数の上限ではない） |

enumのVALUEは、LANE COVER: `0=OFF,1=SUDDEN+,2=HIDDEN+,3=SUD+/HID+,4=LIFT,5=LIFT/SUD+,6=LIFT/HID+,7=ALL`（1=SUDDEN+、2=HIDDEN+、4=LIFTのビット和）、
GAS下限: `0=ASSIST EASY,1=EASY,2=NORMAL`、HS CONFIG:
`0=NORMAL,1=CLASSIC,2=FLOATING,3=NORMAL+FLOATING,4=CLASSIC+FLOATING,5=LR2`、LN MODE:
`0=AUTO LN,1=AUTO CN,2=AUTO HCN,3=FORCE LN,4=FORCE CN,5=FORCE HCN`です。
GAS MODEは`0=OFF,1=CONTINUE,2=HARD TO GROOVE,3=BEST CLEAR,4=SELECT TO UNDER`、
BGAは`0=ON,1=AUTO,2=OFF`、JUDGE ALGORITHMは`0=COMBO,1=DURATION,2=LOWEST`です。
緑数字は1..6000、表示オフセットは-500..500 ms、いずれも刻み1です。
Modifierは`0=OFF,1=REMOVE`を新E2の候補とします。既存経路で保存された候補外値は
SCROLL `2=ADD`、LN `2=ADD LN,3=ADD CN,4=ADD HCN,5=ADD ALL`、MINE
`2=ADD RANDOM,3=ADD NEAR,4=ADD BLANK`として読み取れます。変更するまで値を保持し、
VALUE_INDEXは-1、CHOICE_COUNTは2、textの値と理由で候補外設定を知らせます。

値未取得はVALUE=-1です。表示オフセットの有効な-1 msとは、editable/validで区別します。E2はキーモード未解決時に7Kを編集対象として解決し、SCOPE=7を返します。
低水準snapshotで対象を未取得として渡す場合はSCOPE/VALUE=-1の契約を維持します。
非表示時はITEM_COUNT / CHOICE_COUNT / ROW_SLOTS=0、他のnumber=-1、
textは空文字、正のoptionはfalseです。数値が0の設定と未取得を区別します。

#### 選択項目のtext ref

| ID | 意味 |
|---|---|
| 19300 / 19301 / 19302 | 項目名 / 値ラベル / カテゴリ名 |
| 19303 / 19304 / 19305 | 説明 / 編集不可・非適用・候補外値の理由 / 補助情報（保存量、GAS方式） |
| 19306 / 19307 | ローカライズされたタイトル / 編集対象スコープの表示 |
| 19308 / 19309 | 操作ガイド / 1-based位置と件数の表示 |

これらはJSONのtext.refとLuaの`main_state.text(id)`で同じ内容になります。
ラベルやenum値の翻訳対応表をスキンに持たせず、本体i18n文字列を使います。

#### option / event

| option ID | 意味 |
|---|---|
| 19300 | 新パネル表示中 |
| 19301 / 19302 | 選択項目のeditable / effective |
| 19303 / 19304 / 19305 | 値型bool / enum / number |

editableは変更可能、effectiveは現在のモード・方式で利用する設定かを表します。
OFF値、非適用、編集不可は独立します。effectiveは譜面に変換対象ノーツがあることや
最終Assist判定、スコア送信資格を保証しません。負のoptionは通常の否定規則に従います。

| event ID | 意味 |
|---|---|
| 19300 / 19301 | 項目の前 / 次です。端で循環 |
| 19302 / 19303 | 値の前 / 次です。bool/enumは循環、numberは範囲内に制限 |
| 19304 | 引数argの安定ITEM_IDを選択します。不明なIDは無視 |
| 19310..19316 | 可視行0..6を選択します。空き行は無視 |
| 19317 / 19318 | スクロール用の枠外slot 7 / 8を選択 |
| 19320..19337 | slot `s=0..8`の数値を1ステップ変更です。`19320+2*s`=減少、`19321+2*s`=増加 |

数値eventはその列を選択してから増減します。bool/enum・空き行・非表示・退場中ではno-opです。
同梱スキンでは使わないが、外部スキン用APIとして番号・動作を維持します。
最小 / 最大では設定・保存・重い再処理を行いません。argの符号は使いません。
19304は引数を渡せるイベント経路用です。通常のdestinationクリックは可視行eventを使います。
event引数の符号で19300..19303の方向は変わりません。
number / text / option / eventは別名前空間で、同じ番号でも意味が異なります。

#### 可視行スロット

slot `s=0..6`の基底は`19400 + 10*s`です。各名前空間で次を解決します。

| offset | number | text | option |
|---|---|---|---|
| 0 | item_id | label | valid |
| 1 | category_id | value_label | selected |
| 2 | value | 状態ラベル（編集不可 / 非適用 / 空文字） | editable |
| 3 | scope | 大分類の英語表記（`LANE / HI-SPEED`等。言語設定によらず英語） | effective |
| 4 | CHOICE_COUNT | 空文字 | enumの候補外値（VALUE_INDEX=-1）です。numberではfalse |
| 5 | VALUE_INDEX | 空文字 | 数値型 |
| 6 | 数値MIN（他の型は-1） | 空文字 | 数値の減少が可能（editableかつ現在値>MIN） |
| 7 | 数値MAX（他の型は-1） | 空文字 | 数値の増加が可能（editableかつ現在値<MAX） |
| 8 | 数値STEP（他の型は-1） | 空文字 | false（予約） |
| 9 | -1 | 空文字 | false（予約） |

空き行・非表示行はnumber=-1、text空、全option=falseです。
viewportは環状で、項目がある場合に中央slot 3だけselected=trueになります。
7項目以上ではslot 0..6が表示窓中央の前後3項目を含み、indexは`(VIEWPORT_START+s)%ITEM_COUNT`です。
停止時の中央はCURSORです。移動中は表示窓が選択先へ追従するため、slot 3とCURSORが異なる場合があります。
7項目未満では重複表示せず、`3-floor((ITEM_COUNT-1)/2)`からITEM_COUNT個のslotだけvalidにします。
例えば1項目ならslot 3、2項目ならslot 3と4、3項目ならslot 2..4が有効です。
配列の末尾で打ち切らず、validと各slotのrefを使います。
枠外の補助slot 7 / 8も同じ形式で公開します（19470..19489）。7項目を超える場合のみvalidで、
slot 7は表示窓中央の4つ前（描画位置はslot -1）、slot 8は4つ後（描画位置はslot 7）です。
常にselected=falseです。移動途中の端を途切れさせないため、各側1列を枠外に描画してマスクします。
ROW_SLOTSは可視枠の数なので7のままです。8項目だけの時は両補助slotが同じ項目を指すが、
見えるのは移動方向側だけです。7項目以下の補助slotは未取得値を返します。
19400..19489は行用の予約領域で、新timerは追加しません。

#### 項目切替アニメーション

destinationの`"bmzDetailScroll": [dx, dy]`に1項目分の移動量（skin canvasのpixel単位、
左下原点）を指定すると、本体のE2移動補間を描画とクリック判定の両方へ適用します。
デフォルト横並びは`[165,0]`、縦並びでは適切なyの間隔を指定できます。
未指定は`[0,0]`です。E2非表示時は変位0（上記の退場表示中だけ解放時の変位を保持）です。既存offset IDやtimerの意味は変更しません。
列の背景・文字・候補・クリック領域には同じ指定を付け、中央カーソルには付けません。
列が枠外へ出る部分はスキン側で不透明なマスクを置き、`act:0, clickable:true`等で
マスクの背後のクリックも消費します（E2のeventフィルターが0を除外する）。
選曲バーと同じ三次Hermite補間で、追加入力時の表示位置と速度を引き継いで終点速度を0にします。
ホイール・鍵の単発移動は選曲の低速設定、アナログは1項目120ms・複数項目30msを使います。
複数列の残り変位も保持し、整数列分をVIEWPORT_STARTへ、小数列分を`bmzDetailScroll`へ反映します。
選択先のref 19300帯と説明・値変更は入力直後のCURSORを参照し、行ref・行eventは表示窓の項目を参照します。
クリックは判定に使用したsnapshotの行へ適用するため、移動中も見えている項目を操作できます。

#### 全選択肢セル（横並び表示用の追加API）

可視項目slot `s=0..6`と枠外補助slot `s=7,8`、選択肢index `c=0..7`の基底IDは`19500 + 64*s + 4*c`です。
19500..20075を予約します（旧可視枠19500..19947に、補助枠19948..20075を追加）。
各項目ブロック後半32 IDは将来用で未取得値を返します。
bool/enumは2〜6選択肢のため、全候補を同時表示できます。数値行は候補セルがすべて無効です。

| offset | number | text | option | event |
|---|---|---|---|---|
| 0 | 選択肢のVALUE | ローカライズされた選択肢ラベル | valid | この項目のこの選択肢を直接設定 |
| 1 | -1 | 空文字 | 現在の設定値と一致 | なし |
| 2 | -1 | 空文字 | 編集可能 | なし |
| 3 | -1 | 空文字 | false（予約） | なし |

例: slot 0の2番目の選択肢は19504、現在値との一致はoption 19505です。
中央の選択項目はslot 3なので、同じ選択肢のIDは19696、一致はoption 19697です。
空きセル・非表示時のnumberは-1、text空、option falseです。候補外ADDや値未取得のsnapshotでは
どの選択肢も「現在値と一致」しません。number型は列挙せず、既存の現在値/range参照を使います。
編集不可項目は候補を表示できるが設定eventはno-opです。非適用でもeditableなら変更可能です。
eventはE2が有効な間だけ受け付け、argは無視します。同じ値のクリックでは副作用を実行しません。
選択肢のtextはLua `main_state.text`にも供給し、スキン側のenum表を不要にします。

### BMZ Select Settings Rows

設定入口、設定カテゴリ、`戻る`、`閉じる`は、通常の検索フォルダや曲フォルダとは
独立した`SelectRowKind`としてskinへ渡します。設定ルートの一覧先頭は`閉じる`、
設定カテゴリ内の一覧先頭は`戻る`になります。

beatoraja互換の選曲イベント`13` (`KEY CONFIG`)は、現在の選曲フォルダ履歴を維持したまま
`設定 > キー設定`を直接開きます。キー設定ルートの`戻る`を実行すると、イベントを実行する
直前のフォルダとカーソル位置へ復帰します。

通常設定行とキー設定行の`songlist`バーテキストは`設定項目名 [設定値]`の形式で渡します。
カーソルを合わせた行の詳細表示では、タイトル (`ref=10`)に設定項目名、サブタイトル
(`ref=11`)に設定項目自体の説明文、アーティスト (`ref=14`)に現在値を渡します。説明文には
設定項目名、現在値、設定操作の案内を含めません。編集中表示`[編集中]`はジャンル
(`ref=13`)に渡し、サブアーティスト (`ref=15`)は空にします。カテゴリ、`戻る`、`閉じる`
には設定値や説明文を付加しません。

BMZ対応select skinは`songlist`の配列末尾へ次の専用slotを追加できます。
indexは0始まりです。`image`はsonglist用imagesetの`images`、
`text`は`songlist.text`のdestination indexを表します。

| row | image index | text index | slotがない既存skinでのfallback |
| --- | ---: | ---: | --- |
| 設定入口 / 設定カテゴリ | 8 | 11 | 入口は検索フォルダ`6 / 10` → 曲フォルダ`1 / 4`、カテゴリは曲フォルダ`1 / 4` |
| `戻る` | 9 | 12 | 検索フォルダ`6 / 10` → 曲フォルダ`1 / 4` |
| `閉じる` | 10 | 13 | 検索フォルダ`6 / 10` → 曲フォルダ`1 / 4` |

専用slotが配列長の外なら表の順にfallbackし、曲フォルダslotもなければ先頭slotを使います。
このため、専用slotを持たないskinも、変更なしで既存の画像・文字destinationで表示されます。
image index `7`は既存skinがno-song barに使用するため、BMZ専用image slotは`8`から始めます。

行種別をdestination条件やimagesetの選択に使う場合は次のBMZ ref / optionを使います。

| ref / option | kind | meaning |
| ---: | --- | --- |
| 1960 | number / event_index | `0=設定以外`, `1=設定入口または設定カテゴリ`, `2=戻る`, `3=閉じる` |
| 1961 | option | 設定入口または設定カテゴリ |
| 1962 | option | `戻る` |
| 1963 | option | `閉じる` |

beatoraja互換の`OPTION_FOLDERBAR` (`op: 1`)は、設定入口・設定カテゴリ・`戻る`・
`閉じる`のいずれでも引き続き真になります。

### BMZ Best Score Option Refs

自己ベストEX SCOREを記録した履歴の配置を公開します。現在の設定用ref
`42/43/54/344/345`とは別に、次のIDを使用します。

| ref / option | kind | meaning |
| ---: | --- | --- |
| 19200 | option | 参照するベストに配置履歴がある |
| 19201 | number / event_index / image・imageset ref / text | ベストの1P配置 |
| 19202 | 同上 | ベストの2P配置 |
| 19203 | 同上 | ベストの適用済みDP OPTION |

配置indexは`0=NORMAL`, `1=MIRROR`, `2=RANDOM`, `3=R-RANDOM`, `4=S-RANDOM`,
`5=SPIRAL`, `6=H-RANDOM`, `7=ALL-SCR`, `8=RANDOM-EX`, `9=S-RANDOM-EX`,
`10=F-RANDOM`, `11=MF-RANDOM`です。DP OPTIONは`0=OFF`, `1=FLIP`, `2=BATTLE`,
`3=BATTLE AS`です。textはこれらの表示名を返します。1P/2Pの区別を保持し、SPで2P表示が
不要な場合はスキン側のキーモード条件で隠します。

Selectは選択曲の保存済みベスト、Decide/Playは開始時のベスト、Resultは今回の
保存・更新前のベストを参照します。Resultで更新しても表示対象は変わらず、Selectへ
戻ると更新後の値になります。初プレイのResult、非譜面行、コース総合結果、履歴未取得時は
`19200=false`、number/event_indexは`-1`、textは空文字です。image/imagesetは未取得時に
先頭画像へフォールバックするため、destinationには`op={19200}`を指定します。

参照元は現在のprofile・ScoreKey（譜面hash、正規化LN policy、DP集約区分、判定ルール）
の`best_score_history_id`です。同点の更新は既存のベスト更新規則に従い、ランプのみの更新では
配置を変更しません。DPのOFF/FLIPは同じスコア区分だが、表示は履歴の実際のDP OPTIONを返します。
beatoraja等から取り込んだオプションも、保存値をそのままベスト時の配置として扱います。
由来の確認済みフラグやtrophy集計は使用しません。

```lua
-- 自己ベストをRANDOMで記録した場合に表示する。
draw = function()
    return main_state.option(19200) and main_state.number(19201) == 2
end
```

配置値は画面のsnapshotから取得します。Luaでは選曲変更へ追随するよう、ロード時の
ローカル変数へ固定せず、上記のようにcallback内から参照します。

### BMZ RANDOM Lane Refs

beatoraja result skin互換の`450..469`は、BMZではplay/select skinにも拡張して公開します。
配列は`pattern[表示先レーン] = 元レーン`で保持し、skinへは各sideのレーン番号を
1始まりで返します。

| ref | meaning |
| ---: | --- |
| 450..458 | 1P key 1..9の元レーン番号 |
| 459 | 1P Scratchの元レーン番号 |
| 460..466 | 2P key 1..7の元レーン番号 |
| 469 | 2P Scratchの元レーン番号 |

`467` / `468`はbeatorajaに対応する定義がないため未使用です。resultでは
RANDOM / R-RANDOM / RANDOM-EX（ScratchはRANDOM-EXのみ）の固定配置を返します。
playでは現在プレイ中の同じ固定配置を返します。selectでは「このまま開始した場合に
適用する予定の配置」を返し、通常の未抽選RANDOMや予定配置がない場合は全て`0`です。
リプレイや将来のライバル配置コピーなど、選曲中に配置が確定している機能が
select snapshotへ予定配置を設定します。

### BMZ Hispeed Mode Refs

`1900..1996`はBMZ共通extension、`1997..1999`はLR2変換のJudge Detail用に予約し、
`19000`以降はBMZの高位extensionとして扱います。beatoraja互換ref / option / timerと
衝突させないため、BMZ独自状態はこれらの管理済み範囲へ追加します。

| ref | kind | meaning |
| ---: | --- | --- |
| 1900 | number / event_index / text | current HS modeです。number / event_indexは`0=base`, `1=Floating`、textは`CHS` / `NHS` / `FHS` |
| 1901 | number / option | Floating active flagです。`0=OFF`, `1=ON` |
| 1902 | number | target green numberです。Floating時は固定target、それ以外は現在green number |
| 1916 | number / event_index / text | base HSです。`0=Classic`, `1=Normal`、textは`CHS` / `NHS` |
| 1917 | number / event_index | Normal HS level (`1..=20`) |
| 1918 | number / event_index / text | HS設定です。`0=NORMAL`, `1=CLASSIC`, `2=FLOATING`, `3=NORMAL+FLOATING`, `4=CLASSIC+FLOATING`, `5=LR2` |

`op: [1901]`または`draw: "number(1901)==1"`でFloating時だけdestinationを表示できます。

### BMZ Key Mode Refs

選曲中は曲行の譜面、決定・プレイ・リザルトでは実際のプレイ譜面から値を得ます。
フォルダ行や設定行では、譜面モードのnumber / optionを無効として扱います。

| ref / option | kind | meaning |
| ---: | --- | --- |
| 1903 | number / event_index | key mode (`4`, `5`, `6`, `7`, `8`, `9`, `10`, `14`) |
| 1904 | number | Scratchを含む実レーン数 |
| 1905..1912 | option | 順に4K / 5K / 6K / 7K / 8K / 9K / 10K / 14K完全一致 |
| 1913 | option | Scratchなし (4K / 6K / 8K / 9K) |
| 1914 | option | single play (5K / 7K) |
| 1915 | option | double play (10K / 14K) |

`1903..1915`は、Selectでは現在の設定を適用した場合、Decide / Play / Resultでは実際に
開始した試行の**実効key mode**を返します。SelectのSessionMode `AUTO BATTLE` / `G-BATTLE`
は5K/7Kのまま維持し、7K→6K変換と通常のDP OPTION `BATTLE`だけを反映します。

### BMZ Source Chart Refs

変換前の譜面情報と実効譜面を区別するため、次の高位extensionを公開します。

| ref / option | kind | meaning |
| ---: | --- | --- |
| 19180 | number / event_index / imageset ref | 変換前key mode (`4`, `5`, `6`, `7`, `8`, `9`, `10`, `14`) |
| 19181..19188 | option | 変換前key modeです。順に4K / 5K / 6K / 7K / 8K / 9K / 10K / 14K |
| 19189 | option | 7K→6K変換をこの試行へ適用した |
| 19190 | number / event_index / imageset ref | 変換前LN profile bitmask (`0..15`) |
| 19191 | option | 未定義LNを含む (`bit 0`) |
| 19192 | option | 定義済みLNを含む (`bit 1`) |
| 19193 | option | 定義済みCNを含む (`bit 2`) |
| 19194 | option | 定義済みHCNを含む (`bit 3`) |
| 19195 | option | 上記LN種別を2種類以上含む |
| 19196 | option | 変換前LN profileを取得済み |

`19190`はLN policy、コース制約、7K→6K、譜面オプションを適用する前に固定します。
LNがない譜面も取得済みなら`19190=0`かつ`19196=true`になるため、未取得状態と区別できます。
Selectでは曲行だけ取得でき、フォルダ・設定・未解決コース行では値なしです。Decide / Play /
Resultでは同じ試行値を維持します。実効key modeは`1903`、実効LN種別はbeatoraja互換
`308`、実効LN有無はoption `172/173`を使います。

### Beatoraja Attempt Property Compatibility

次のbeatoraja互換index refはSelectの現在設定だけでなく、Decide / Play / Resultでは
実際に開始した試行の値を返します。JSONのimage / imageset `ref`、Luaの`event_index()`、
対応するindex propertyで共通の値を使います。NumberPropertyとIDが重なる場合はbeatorajaの
NumberProperty側の意味を優先します（例: Selectの`number(78)`はclear count）。

| ref | meaning |
| ---: | --- |
| 54 | 適用済みDP option (`0=OFF`, `1=FLIP`, `2=BATTLE`, `3=BATTLE AS`) |
| 55 | HSFIX (`0=OFF`, `1=START`, `2=MAX`, `3=MAIN`, `4=MIN`) |
| 78 | gauge auto shift (`0=OFF`, `1=CONTINUE`, `2=HARD TO GROOVE`, `3=BEST CLEAR`, `4=SELECT TO UNDER`) |
| 308 | 実効LN種別 (`0=LN`, `1=CN`, `2=HCN`) |
| 340 | judge algorithm (`0=COMBO`, `1=DURATION`, `2=LOWEST`) |
| 341 | bottom shiftable gauge (`0=OFF`, `1=EASY`, `2=NORMAL`) |

譜面プロパティoptionもSelect / Decide / Play / Resultで同じ実効譜面を参照します。

| option pair | meaning |
| ---: | --- |
| 170 / 171 | BGAなし / あり |
| 172 / 173 | LNなし / あり |
| 176 / 177 | BPM変化なし / あり |
| 178 / 179 | BMS `#RANDOM`系列なし / あり |

Selectで譜面行が選ばれていない場合は、これらの正負optionをどちらもfalseにします。

### BMZ Logical Input Refs

| option | timer | logical input |
| ---: | ---: | --- |
| 1920 | 19000 | E1 |
| 1921 | 19001 | E2 |
| 1922 | 19002 | E3 |
| 1923 | 19003 | E4 |
| 1924 | 19004 | UI Left |
| 1925 | 19005 | UI Right |
| 1926 | 19006 | UI Up |
| 1927 | 19007 | UI Down |

optionは論理入力の押下中、timerは直近の論理入力press edgeからの経過msを返します。
同じ論理入力に複数の物理キーを割り当てた場合はOR集約し、押下中に別キーを追加しても
timerを再起動しません。scene入場時から押されている入力はpress edgeとして扱いません。
E1は設定済みE1とlegacy Start、E2は設定済みE2とlegacy Selectを含みます。

E1/E2共通パネル向けには次の集約状態を使います。

| option | press / hold timer | release timer | aggregate input |
| ---: | ---: | ---: | --- |
| 1928 | 19008 | 19009 | E1またはE2 |

option 1928はE1/E2のどちらかを押している間trueです。timer 19008は最初の一方を押した
false-to-true edgeで0msから始まり、どちらかを押している間だけ有効になります。もう一方を追加で
押しても再起動しません。両方を離すと19008はoffになり、timer 19009が0msから始まります。
次の共通pressで19009はoffになります。scene入場時から押されている入力ではどちらも発火しません。

入場後の表示を押下中に維持し、release後に退場させる場合は、同じ見た目を別IDで定義して
次のように組み合わせます。

```json
{
  "destination": [
    {
      "id": "hs-panel-held",
      "timer": 19008,
      "loop": 120,
      "op": [1928],
      "dst": [
        { "time": 0, "x": 80, "y": 100, "w": 400, "h": 120, "a": 0 },
        { "time": 120, "x": 100, "a": 255 }
      ]
    },
    {
      "id": "hs-panel-release",
      "timer": 19009,
      "loop": -1,
      "op": [-1928],
      "dst": [
        { "time": 0, "x": 100, "y": 100, "w": 400, "h": 120, "a": 255 },
        { "time": 180, "x": 80, "a": 0 }
      ]
    }
  ]
}
```

`runtimeEvent`に`triggerAction`を指定すると、Luaで入力状態を取得せずにruntime flagを
切り替えられます。値は`e1_press`, `e2_press`, `e3_press`, `e4_press`,
`ui_left_press`, `ui_right_press`, `ui_up_press`, `ui_down_press`です。

```json
{
  "runtimeFlag": [{ "id": 1, "initial": false }],
  "runtimeEvent": [{
    "id": -20001,
    "toggleFlags": [1],
    "triggerAction": "e1_press"
  }]
}
```

### BMZ Scratch / Keys Judge Refs

プレイ中の最新判定を、判定領域ごとにScratchと鍵盤へ分けて公開するBMZ拡張です。
slotは`region * 2 + lane kind`の順で、Scratch判定はScratch側だけ、鍵盤判定は鍵盤側だけを
更新します。両方のtimerは同時に有効にでき、互いの判定では再起動しません。

| 判定領域 / lane kind | timer | PGREAT option | FAST / EARLY option | SLOW / LATE option | タイミング差ref |
| --- | ---: | ---: | ---: | ---: | ---: |
| region 0 / 1P Scratch | 19010 | 19020 | 19030 | 19040 | 19050 |
| region 0 / 1P Keys | 19011 | 19021 | 19031 | 19041 | 19051 |
| region 1 / 2P Scratch | 19012 | 19022 | 19032 | 19042 | 19052 |
| region 1 / 2P Keys | 19013 | 19023 | 19033 | 19043 | 19053 |
| region 2 / 3P Scratch | 19014 | 19024 | 19034 | 19044 | 19054 |
| region 2 / 3P Keys | 19015 | 19025 | 19035 | 19045 | 19055 |

判定領域は既存の`timer 46/47/247`と同じく、skinの`judge[].index`から得た領域数と
beatoraja互換のレーン分割で決まります。`1P` / `2P` / `3P`は物理プレイヤー数ではなく、
それぞれregion 0 / 1 / 2の別名です。Scratchは`Lane::Scratch` / `Lane::Scratch2`、それ以外の
レーンはKeysに分類するため、ScratchのないモードではScratch側timerは開始しません。

timerは該当チャンネルの最新判定からの経過msです。標準判定表示に使う800msの
`recent_judgements`保持期間とは別に、プレイ中の最後の値をrenderer runtimeへ保持します。
表示時間はdestinationの`dst`と`loop`で決めます。プレイ開始、リトライ、次曲開始、
skin runtimeのresetでは全チャンネルを未開始へ戻します。

FAST/SLOW optionとタイミング差refには、既存の判定表示設定と同じフィルタを適用します。

- `Auto`: PGREATのFAST/SLOW optionだけfalseです。タイミング差refは返します。
- `ThresholdMs`: PGREATは閾値未満でFAST/SLOW optionをfalseにし、タイミング差refも値なしにします。
  GREAT / GOOD / BAD / POOR / EMPTY POORは閾値内でも常にFAST/SLOW optionとタイミング差refを返します。
- タイミング差refの符号は既存ref `525..527`と同じで、正がFAST、負がSLOWです。

判定前はtimerがOFF、各optionがfalse、タイミング差refが値なしとなります。PGREAT optionは
最新判定がPGREATかどうかを表し、FAST/SLOWの表示フィルタとは独立します。

既存の`timer 46/47/247`、PGREAT option `241/261/361`、FAST/SLOW option
`1242/1243/1262/1263/1362/1363`、タイミング差ref `525..527`の挙動と800ms保持は変更しません。
このため拡張IDを参照しない既存skinの表示は変わりません。拡張IDはbeatorajaおよび古いBMZでは
利用できません。

### BMZ BPM Change Preview Refs

プレイ中、次のBPM変化を予告表示するためのBMZ拡張です。本体は変化の事実だけを公開し、
何ms前から予告するかはスキンが宣言します。表示のON/OFFや位置もスキン側のオプションで決めます。

```json
{ "bmzBpmChangePreview": { "leadMs": 2000 } }
```

Lua skinはロード結果のtableに同じキーを設定します。`leadMs`は省略時2000、0〜10000にclampします。
宣言がないskinでは予告option `19064`とtimer `19068`が常にOFFになります。その他のrefは宣言に関係なく返します。

| ID | kind | meaning |
| ---: | --- | --- |
| 19060 | number | 次のBPM変化までの残りms |
| 19061 | number | 変化後のBPM（`ref=160`と同じく四捨五入） |
| 19062 | number | 変化直前のBPM |
| 19063 | number | 変化率（変化後 ÷ 変化前 ×100、四捨五入） |
| 19064 | option | 残り時間が`leadMs`以下（予告表示中） |
| 19065 | option | 次の変化でBPMが上がる |
| 19066 | option | 次の変化でBPMが下がる |
| 19067 | option | 以降にBPM変化がある（距離は問わない） |
| 19068 | timer | 予告timerです。変化時刻の`leadMs`前に0msで始まり、変化時刻に達するとOFF |
| 19069 | timer | 直前のBPM変化からの経過msです。最初の変化まではOFF |

- BPM変化は、`ref=160`の現在BPMが切り替わる時刻とします。STOPや同じ値の再指定は含めず、
  同じ時刻に複数の指定がある場合は最後の値と比べます。BPMが変わらない譜面ではどの値も返しません。
- 時刻は`ref=160`と同じく表示オフセット適用後のレーン時刻で判定します。
- 19060、`leadMs`、timer 19068 / 19069は再生速度を反映した**実時間**とします。
  譜面時間で進む他のプレイtimerとは、1.0倍速以外で進み方が異なります。
- 予告は次の1回だけを対象とします。短い間隔で変化が続く譜面では、変化ごとに値が切り替わります。
- 次の変化がない場合、number画像は非表示、Luaの`main_state.number()`は`0`を返します。
  Luaでは`option(19067)`で判定します。

同梱antiqueはこの拡張で「BPM変化予告 (BMZ)」を実装しています。予告timerでフェードインし、
`19064`と`19065` / `19066`で、FAST / SLOW数と同じ青・赤の数字画像による「変化前 » 変化後」を切り替えます。
レーン外に置く場合は、レーン側の数字の桁数を`draw: "number(19062) >= 10 && number(19062) < 100"`
のような条件で判定し、「»」と反対側の数字の位置を変えてレーン側の数字をレーンへ寄せます。

### BMZ Exit Hold Refs

選曲画面のルートでEscapeを長押しして終了するときの進捗を、選曲skinが自前のデザインで
表示するためのBMZ拡張です。既定では本体が画面左上に「HOLD ESC TO EXIT」のパネルと
進捗バーを重ねます。skinが次の宣言をすると、本体の表示を出さずにskinの表示だけを使います。

```json
{ "bmzExitHold": { "builtin": false } }
```

Lua skinはロード結果のtableに同じキー（`bmzExitHold = { builtin = false }`）を設定します。
`builtin`の省略時と宣言自体がない場合は`true`で、本体の表示を残します。
次のref / option / timerは宣言に関係なく返すため、本体の表示に重ねる演出にも使えます。

| ID | kind | meaning |
| ---: | --- | --- |
| 19070 | number / graph | 進捗です。numberは`0..100`（切り捨て）、graphの`type`では`0.0..1.0` |
| 19071 | number | 終了までの残りms。長押ししていない間は`0` |
| 19072 | number | 終了に必要な長押し時間ms（現在は1200） |
| 19073 | option | Escapeを終了に向けて長押ししている |
| 19074 | timer | 長押し開始からの経過ms。長押ししていない間はOFF |

- 長押しの開始・取消の条件は操作仕様（[controls.md](controls.md)）と同じです。
  フォルダ内のEscapeやパネル表示中は進みません。
- 宣言はjson / Lua skinだけが対象です。LR2 CSVには宣言の手段がないため、本体の表示を残します
  （refは利用できます）。

```json
{
  "bmzExitHold": { "builtin": false },
  "graph": [{ "id": "exit_bar", "src": "parts", "x": 0, "y": 0, "w": 400, "h": 8, "type": 19070, "angle": 0 }],
  "destination": [
    { "id": "exit_bar", "op": [19073], "timer": 19074, "loop": 150,
      "dst": [{ "time": 0, "x": 40, "y": 40, "w": 400, "h": 8, "a": 0 },
              { "time": 150, "x": 40, "y": 40, "w": 400, "h": 8, "a": 255 }] }
  ]
}
```

### BMZ Chart Text Refs

BMSの`#TEXTxx`とチャンネル99（`#xxx99`）で指定した譜面テキストを、既存のtext部品で
表示するためのBMZ拡張です。beatoraja / LR2はこの命令を表示しないため、IDと動作はすべて
BMZ独自の契約です。新しい描画オブジェクトは追加せず、配置・色・透明度・改行・折り返し・
影・縁取り・アニメーションは通常のtext部品と同じです。`20100..20199`を譜面テキスト用に予約し、
beatorajaのcustom timer（`10000..19999`）やBMZの`19000`番台とは重なりません。

| ID | kind | 名前 | meaning |
| ---: | --- | --- | --- |
| 20100 | text | `bmz_chart_text` | 現在の譜面テキストです。最初のイベント前は空文字 |
| 20101 | option | `bmz_chart_text_visible` | 20100が空文字でなければtrue |
| 20102 | timer | — | 直近のチャンネル99イベントからの経過msです。最初のイベント前はOFF |
| 20103 | text | `bmz_chart_miss_text` | `#TEXT00`（ミス時のテキスト）の定義です。未定義なら空文字 |
| 20104 | option | `bmz_chart_miss_text_active` | `#TEXT00`を20100へ表示している間true |
| 20105 | timer | — | ミス表示の開始（連続ミスでは再開始）からの経過msです。表示中以外はOFF |

名前はBMZ拡張の表で解決し、beatoraja由来の生成表とは分けて管理する
（[名前によるプロパティ参照](#名前によるプロパティ参照)）です。

```json
{
  "text": [
    { "id": "chart_message", "font": "message_font", "size": 28, "value": "bmz_chart_text" }
  ],
  "destination": [
    { "id": "chart_message", "op": [20101], "dst": [{ "x": 40, "y": 600, "w": 800, "h": 32 }] }
  ]
}
```

```lua
local main_state = require("main_state")
local skin = { type = 0, w = 1280, h = 720 }
-- text部品の value に関数を渡すと、毎フレーム現在のテキストを返す。
-- main_state.text("bmz_chart_text") / main_state.option(20101) も使用できる。
skin.text = {
  { id = "chart_message", font = "message_font", size = 28,
    value = function() return main_state.text(20100) end },
}
```

timer 20102はイベントごとに0msから始まるため、切替直後のフェードインに使えます。
`loop`を最終時刻にすると、最後の状態のまま表示を保持します。

```json
{ "id": "chart_message", "timer": 20102, "loop": 300, "dst": [
  { "time": 0, "x": 40, "y": 600, "w": 800, "h": 32, "a": 0 },
  { "time": 300, "x": 40, "y": 600, "w": 800, "h": 32, "a": 255 }
] }
```

LR2 CSVでは既存の文字列refをそのまま使います。

```text
#SRC_TEXT,0,0,20100,0,0
#DST_TEXT,0,0,40,600,800,32,0,255,255,255,255,0,0,0,0,0,0,20101,0,0
```

- `value: 20100`、`value: "bmz_chart_text"`、`ref: 20100`のいずれでも同じ値を返します。
  Luaの関数はロード時の値に固定されず、プレイ中の切替に追従します。
- テキストは譜面時刻で決まります。READY中・最初のイベント前は空文字、以後は次の有効な
  イベントまで保持し、時間経過では消えません。PAUSE中は変化せず、シーク・練習モード・
  リプレイ・オートプレイでも同じ譜面時刻なら同じ値を返します。曲・コースステージが変わると
  新しい譜面のイベントから求め直します。
- timer 20102は文字列が変わったかではなく、イベントが発生したかで再始動します。
  同じ文字列の再指定や空文字への切替でも0msへ戻り、同じ時刻の複数イベントでは1回だけ戻ります。
  経過は譜面時間のmsで、PAUSE中は進みません。シークでは現在位置以前の最新イベントから
  求め直し、逆方向へのシークで後のイベントの状態を残しません。最初のイベント前・READY中・
  曲やコースステージの開始時はOFFです。表示文字列の保持期間とは独立で、timerが進んでも
  テキストは消えません。Luaの`main_state.timer(20102)` / `timer_elapsed_ms(20102)`も同じ値を使います。
- beatoraja / LR2はBMSのテキストを表示しないため、これらのID・名前・timerと上記のBMS解釈は
  すべてBMZ独自拡張です。既存のbeatoraja互換ref / option / timerの意味、既存スキンの
  描画順は変更しません。
- プレイスキンが譜面テキストを表示しない場合（内蔵スキンを含む）、20100の文字列を画面左上の
  汎用メッセージ欄へ表示します。設定は無く常に有効です。スキンの描画計画へtext部品を追加するわけでは
  なく、トースト・バックグラウンド処理の表示と同じ欄に`|`区切りで並べます。
  スキンが表示するとみなすのは次のどちらかに当たる場合です。
  - text部品の`ref` / `value`が20100 / 20103を指す（Luaの単純な関数がAutoモードで
    固定refへ置き換わった場合も含む）。
  - Luaの`main_state.text`で20100 / 20103が読まれる。値の連結・条件分岐・Compatモード等で
    関数のまま残るcallbackも、実行時の読み出しで判別します。描画計画ではスキンの評価後に判定するため、
    テキストが最初に現れたフレームから左上には出ません。一度読まれたらスキンを読み込み直すまで
    維持します。表示に使わないcustom timer等の関数から読んだ場合も表示するとみなします。

短いBMSとJSONスキンのサンプルを[docs/examples/chart-text](examples/chart-text/)に置いています。
フォントは省略しているため、実際のスキンでは`font`を指定します。

#### `#TEXT00` のミス表示（BMZ暫定ポリシー）

nanasiでは`#TEXT00`をミス時に表示します。対象判定や表示時間はnanasiの資料からは
確認できなかったため、BMZでは次の**暫定ポリシー**を採用します。nanasiの確認済み仕様ではありません。

- `#TEXT00`はチャンネル99のイベントとは別に保持し、`#xxx99`の`00`はミス表示になりません。
  `#SONG00`は互換範囲（`01`〜`ZZ`）外として無視します。
- 対象はミスレイヤー（ミスBGA）と同じく、スコアに数える判定のBADとPOOR（見逃し、
  CN/HCNの早離しを含む）です。空POORは含めません。
- 表示時間はプロフィール設定のミスレイヤー表示時間（`play.misslayer_duration_ms`、
  MISSLAYER）を使います。0msでは表示しません。MISSLAYER GOOD CLEAR（`play.hide_misslayer_on_good`）が
  ONのときは、ミスレイヤーと同じく、ミスより後のPGREAT・GREAT・GOODで表示を消します。
- 表示中は20100が`#TEXT00`を返し、20101もその文字列で判定します。チャンネル99のイベントは
  裏で進み、timer 20102もミスでは再始動しません。表示が終わると、その時点の通常テキストへ戻ります。
- 表示中の次のミスで表示時間を延長し、timer 20105を0msへ戻します。
- 時間はミスレイヤーと同じプレイ時計（譜面時刻）で測るため、PAUSE中は進みません。
  リトライ・練習モード・Viewerのシークはセッションを作り直すため表示状態も初期化されます。
  リプレイでは記録した入力から同じ判定を再現するため、同じ時刻に表示されます。
- `#TEXT00`が未定義の譜面では、ミスしても20100、20101、20103、20104、20105は変化しません。
  `#TEXT00 ""`のように空文字を定義した場合は、ミス表示中の20100が空文字になります。

BMS側の解釈:

- `#SONGxx`は`#TEXTxx`の旧式の同等命令として扱います（`01`〜`ZZ`）。同じIDを両方で定義した
  場合は、定義順に関係なく`#TEXTxx`を使います。`#TEXTxx`の値が空文字でも優先します。
- `#TEXTxx`の値を囲む1組の`"`は取り除きます。内側の`"`、日本語、長文はそのまま保持します。
  文字コードは譜面本体と同じ判定で読み込みます。
- `#xxx99`の`00`は空オブジェクトで、表示を変えません。空文字の定義（`#TEXT03 ""`）や
  未定義IDの参照は、空文字へ切り替えるイベントとして扱います。
- 同じIDを再指定してもイベントとして扱います。同じ時刻に複数のイベントがある場合は、
  原文で後のものを表示します。同じ種類（`#TEXT`同士、`#SONG`同士）の定義が重複した場合は
  後の行を使います（他のヘッダーと同じ）。
- `#RANDOM` / `#SWITCH`で選ばれた分岐の定義・メッセージだけを使い、`#BASE 62`では
  大文字小文字を区別したIDで定義と参照を対応させます。
- 解析は分岐解決後の原文から行います。譜面ハッシュ、原文、他の命令の解析は変わりません。
  不正な定義やIDは無視し、読み込みを中断しません。
- イベントは時刻順に保持し、描画側は二分探索で現在のイベントを求めます。同じ定義を参照する
  イベントは文字列を共有し、毎フレームの描画データでも複製しません。

### Modified LR2 FAST/SLOW Refs

LR2プレイスキンのdecode時に、改造LR2 / OpenLR2のFAST/SLOW拡張refを次のBMZ状態へ変換します。
JSON / Lua skinの同名refは変換せず、beatorajaの意味を維持します。

| LR2 ref | decode後 | meaning |
| ---: | ---: | --- |
| 210 | 19170 | 最新の1P判定です。`0=非表示`, `1=FAST`, `2=SLOW`です。PGREATは常に`0` |
| 212 | 423 | FAST合計です。GREAT / GOOD / BAD / POOR / EMPTY POORを集計 |
| 214 | 424 | SLOW合計です。GREAT / GOOD / BAD / POOR / EMPTY POORを集計 |

`19170`はLR2変換専用の内部refです。`212/214`は既存のBMZ `423/424`へaliasするため、
OpenLR2の原仕様と異なりPOOR / EMPTY POORも合計へ含みます。

### BMZ Result IR Scope

リザルトのIRパネル (`result_panel(1)`)は、BMZ対応skinに限り全体ランキングと
「自分 + IR ライバル」の一覧を切り替えられます。既存skinは全体ランキングを表示し続けます。

読み込み時に参照したIR数値が変わると、Result skinの構築部分を再評価します。
同じリザルト内では、`source.path`のワイルドカード解決と、ファイル選択`Random`による
`skin_config.get_path()`の結果を保持します。Luaが返されたパスを使って構築する表示も、
初回と同じファイルを参照します。同じパターンを複数回呼ぶ場合は呼び出し順に保持し、
更新で新しく現れた選択も、その更新が適用された時点から保持します。
次のリザルトへの遷移やスキンの通常読み込みでは改めて抽選します。

```json
{
  "resultIrScopeBinding": "active",
  "resultIrScopeToggle": "e1_press"
}
```

`resultIrScopeBinding`は省略時または`global`で全体ランキング固定、`active`
では既存のIR number / text / optionが選択中スコープを表示します。`resultIrScopeToggle`は
省略時または`none`でE1切替を無効にし、`e1_press`はIRパネル表示中のE1 press edge
でスコープを切り替えます。`resultIrScopeToggle`は`active` bindingと組み合わせた場合だけ
有効で、ほかの組み合わせでは無効になります。

| scope | label | IR API scope |
| ---: | --- | --- |
| `0` | `RANKING` | `global` |
| `1` | `RIVAL` | `self_and_rivals` |

`RIVAL`はライバルだけではなく、自分を含む`self_and_rivals`を指します。リザルト対象や
providerがRival scopeをサポートしない場合、E1とRival選択イベントはno-opになります。

以下のref / option / クリックeventはSelect IR Scopeと共通です。

| ref / option | kind | meaning |
| ---: | --- | --- |
| 1964 | number / event_index / text | 選択中scope (`0` / `1`、textは`RANKING` / `RIVAL`) |
| 1965 | option | `RANKING`選択中 |
| 1966 | option | `RIVAL`選択中 |
| 1967 | option | `global`を取得可能 |
| 1968 | option | `self_and_rivals`を取得可能 |
| 1969 | number | 選択中scope内の総人数 |

クリックイベントは`-10003=RANKING`、`-10004=RIVAL`、`-10005=toggle`です。いずれも当該sceneの
`*IrScopeBinding: "active"`のskinでだけIR scopeを変更します。E1は`-10005`と同じ動作で、
IRパネルを自動では開きません。E2 / SelectのIR・グラフ切替とは独立しています。

`runtimeEvent.triggerAction: "e1_press"`はruntime flagの更新専用であり、scope切替には
使いません。対応skinが同じE1 triggerを定義した場合、scope切替とruntime flag更新は
ともに実行されます。

### BMZ Select IR Scope

選曲中の曲行は、BMZ対応skinに限り全体ランキングと「自分 + IRライバル」の一覧を
切り替えられます。既存skinは全体ランキングを表示し続けます。コース行・フォルダ行・設定行は
`RANKING`のみで、Rival scopeは選択できません。

```json
{
  "selectIrScopeBinding": "active",
  "selectIrScopeToggle": "e3_press"
}
```

`selectIrScopeBinding`は省略時または`global`で全体ランキング固定、`active`では
既存のIR number / text / optionが選択中scopeを表示します。`selectIrScopeToggle`は省略時または
`none`でE3切替を無効にし、`e3_press`は通常の選曲表示中のE3 press edgeでscopeを切り替えます。
`active` binding以外との組み合わせ、検索・設定編集・選曲オプションパネル表示中、またはRival
scope未対応時はno-opになります。E3は設定済みのE3操作だけを対象とし、Start互換は持ちません。

Selectの`RIVAL`も`self_and_rivals`を指します。既存の`STRING_RIVAL`等の単一ライバル表示と
ターゲット用のライバル順位列は変更しません。`runtimeEvent.triggerAction: "e3_press"`を同時に
定義した場合も、scope切替とruntime flag更新はともに実行されます。

### BMZ Daily Statistics Refs

`score.db`のlocal / non-autoplay `score_history`をプロファイル単位で集計します。
`profile.toml`の`[statistics] day_start_hour = 0`で日付境界のローカル時刻を指定できます。

| ref | kind | meaning |
| ---: | --- | --- |
| 1930 / 1931 | number | play count / clear count |
| 1932..1937 | number | PGREAT / GREAT / GOOD / BAD / POOR / EMPTY POOR |
| 1938 / 1939 | number | 処理ノーツ / 完了ノーツ |
| 1940 / 1941 | number | EX score / max EX score |
| 1942 | number | rate (0..10000) |
| 1943 | number / text | rank index (`0=AAA` .. `7=F`) / rank label |
| 1944..1946 | number | score / clear / miss countの更新回数 |
| 1950..1959 | text | 当日の直近曲名 (新しい順、連続重複を除外) |

event `-10100`は表示上の日次集計を現在時刻でリセットします。score history自体は削除しません。
MILLIONDOLLAR / m-selectの既存オブジェクトID特例と仮想ファイル互換経路も継続します。

### BMZ Course Result Refs

| ref | meaning |
| ---: | --- |
| 19100 | result stage count |
| 19110..19119 | stage 1..10 EX score |
| 19120..19129 | stage 1..10 gauge (整数部) |
| 19130..19139 | stage 1..10 BP |
| 19140..19149 | stage 1..10 rate (0..10000) |

stage titleはbeatoraja互換text `150..159`を使います。WMII RESULT用
`skin/WMII_FHD/result/courseData.json`のread-only仮想ファイルも互換性のため併存します。

## LITONE9 の Lua 拡張API

選曲の`main_state.total_play_counts_in_session()` / `total_play_notes_in_session()`は、
今回のBMZ起動中にローカル保存したプレイ回数と処理済みノーツ数を返します。
コースは各ステージを集計し、Autoplay・Replay・Practice・保存無効のプレイは除外します。
日別・累計統計とは独立し、アプリ終了でリセットします。

`main_state.score_date_sec_time()`は選択中の譜面またはコースのベストスコアが持つ
`played_at`（Unix秒）を返し、未プレイ・フォルダでは0を返します。
IR順位の日時や最新履歴とは区別します。これら3 APIはfunction callback内で使用し、
フレームのsnapshotを参照します。初回Luaロード中はまだsnapshotがないため0となります。

## Profile Slots

profileの`[skin]`はkey modeごとにplay skin pathと設定を持ちます。
4Kは`play4`, `play4_options`, `play4_files`を使います。
6Kは`play6`, `play6_options`, `play6_files`を使います。
8Kは`play8`, `play8_options`, `play8_files`を使います。

2Kはskin typeのみ予約し、BMZ本体のkey modeとしてはまだ扱いません。

## Bundled Rmz-skin Extensions

`data/skins/Rmz-skin`のBMZ同梱版は、BMZ独自play skin typeとして
`play4main.luaskin` (`type=22`), `play6main.luaskin` (`type=23`),
`play8main.luaskin` (`type=24`)を提供します。

8K版はレーンごとのノーツ色をpropertyで選択できます。property名は
`8Key Lane 1 Color`から`8Key Lane 8 Color`までで、選択肢は
`White`, `Blue`, `Yellow`, `Scratch`です。既定値は`Yellow, White, Blue, White, White, Blue, White, Yellow`です。

5K版は`Notes 5Key Color` propertyでノーツ色の並びを選択できます。
`Default`は`Scratch, White, Blue, Yellow, Blue, White`
(scratch left時の画面左からの並び)です。`6Key-like`はscratch sideに関わらず、
画面左から`White, Blue, White, White, Blue, White`になります。

`F-RANDOM` / `MF-RANDOM`は既存の146×19 ARRANGE sprite領域へフォントで表示します。
Lua定義の`align=1`はdestinationの`x`を中央基準にするため、Rmz-skin側で
sprite左端から半幅だけ中央へ移し、フォントサイズもspriteの実字高へ合わせます。
