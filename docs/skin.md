# スキン仕様

BMZはbeatoraja JSON / Lua skinの互換を基本にします。既存beatoraja skin typeは
そのまま扱い、BMZ独自のkey modeだけ拡張skin typeを予約します。

JSON / Lua / LR2 / Qwilight skinの対応範囲と残課題は[skin-compatibility.md](skin-compatibility.md)を参照してください。
この文書ではIDや各機能の動作契約を管理します。

## Qwilightのプレイスキン

試験対応として、`.yaml` / `.yml`を入口に同じパッケージのLuaとZIPを読み込みます。
`@`で始まるテーマは対象外です。対象は4K / 5K / 6K / 7K / 8K / 9K / 10K / 14KのPlayで、
Select・Decide・Result・G-BATTLE専用表示には対応しません。スキン一覧には各Play枠へ登録します。

元のYAML・Lua・ZIPを揃えて`skins`配下へ置き、一覧を更新して選択してください。
外部のインストール先を直接指定する場合は、通常の[外部スキンの許可](#スキンのファイル参照と外部スキンの許可)を使います。
スキンのファイルは変更・展開しません。音声やテーマ素材の再配布も行いません。

- YAMLの`format`・`frame`・`lambda`・`point`を読み、`configure-0`〜`15`を既存のスキンオプションへ表示します。
  選択変更とキー数変更では配置を再計算します。カタログ走査ではLuaを実行しません。
  条件付き表示・言語別の表示名・位置の調整・レーンカバーは[Qwilightのカスタマイズ](#qwilightのカスタマイズ)を参照してください。
- Luaは配置・素材名の計算だけに使用します。`configures`、キー数、`P1Length` / `P1BuiltLength` / `P2BuiltLength` / `Wall` / `Contents`、
  位置・サイズの調整値等を供給します。ファイル・OS・module APIを公開せず、既存のLua命令数・メモリ制限を適用します。
- lane map、DPの展開、floating / slashの横配置、通常ノーツ・LN・地雷、レーン背景・判定線・キー押下素材、ボム、判定画像、基本装飾、BGA領域、タイトル・アーティスト・ジャンル、整数HUD・ゲージをBMZの描画へ変換します。
  LNは上下キャップの高さ・位置と胴体の重なりを個別に保持します。保持中のヘッド表示は`maintainLongNoteFrontEdge`に従います。
- スコア・コンボ・判定回数・ゲージはBMZの値です。スコア欄はEXスコアです。
  判定画像の文字や固定ラベルは元の素材のままであり、Qwilightの判定・採点ルールを再現しません。
- 判定画像と回数欄は、PGREAT → YELL!、GREAT → YELL、GOOD → COOL、BAD → GOOD、
  POOR → POOR、空POOR → failedに割り当てます。ゲージMAX時のPGREATもYELL!です。
- HS欄（`multiplier`）はBMZの表示HS値（310 / 311）、達成率欄（`point`）はプレイ中のEXスコアレート（102 / 103）を小数点以下2桁で表示します。
  EXスコアレートは`EXスコア / (処理済みノーツ数 × 2) × 100`で、譜面開始前は100.00%です。
  HSは0.01単位へ四捨五入し、EXスコアレートは既存参照と同じく小数点以下3桁目以降を切り捨てます。
  数字・小数点・単位は元の素材を使い、桁数変化、左・中央・右寄せ、DPの`alt-multiplier` / `alt-point`に追従します。

現段階ではその他の小数付きHUD（再生倍率等）、残り時間・進捗、FAST/SLOW・判定グラフ、
オンライン・音声可視化・ポーズUI・専用SE、文字色・font指定、特殊合成とイベント駆動paintPropertyは未対応です。
キー背景・ボタンは通常素材と押下終端素材を切り替え、押下・解放の途中アニメーションは省略します。
判定・コンボの拡大や反復、LNボムの途中ループ、Qwilight固有の失敗状態、素材prefixのフレーム依存分岐も完全互換ではありません。
ゲージ素材は`HP 2`（なければ`HP 0`）を使い、種類別の素材切り替えや上下左右すべての充填方向は未対応です。
オプション全組合せ・浮遊レーンの重なり順・原作との画素一致は保証しません。

内部ではZIPの仮想パスを`archive.zip!skin/entry`として扱い、曲アーカイブの`!/`名前空間と分けます。
アーカイブ実体を許可rootで確認し、entryの親移動・絶対パスを拒否します。
YAML / Luaは各2 MiB、ZIPは32,768 entry・各64 MiBまでです。
分割画像は`sourceFrames`へ変換し、最大256枚、各系列の入力RGBA合計とatlasをそれぞれ128 MiB以内に制限します。
`note.qwilightGeometry`は変換専用のLN配置情報で、既存JSON / Lua / LR2のLN配置には影響しません。

### Qwilightのカスタマイズ

Qwilight本体の設定画面にある項目のうち、プレイスキンの配置に関わるものを既存のスキン設定へ対応させます。
保存先・履歴・再読み込みは他のスキンと共通で、変更すると表示中のPlayスキンを読み込み直します。

**オプションの表示名と条件付き表示**

- 保存する項目名は`configure-comment-{i}-1041`、なければ`configure-comment-{i}`、なければ`Qwilight configure {i}`です。
  保存する選択肢は`configure-{i}`の名前で、BMZのUI言語を切り替えても変わりません。
- 表示名は`configure-comment-{i}-{LCID}`と`configure-{i}-{LCID}`から選びます。日本語は1041、韓国語は1042、
  英語と中国語は1033（なければ言語指定なし）です。選択肢の対応は順番で取るため、個数が違う言語別リストは使いません。
- 設定画面では、スロットのオプションでLuaを実行して`configure-available-{i}`を評価し、`true`以外の項目を隠します。
  隠れた項目の保存値は残ります。Qwilightと同じく、`lambda`・`format`の値と素材名（`_Get*`）は保存値で、
  `point`・`frame`・`alt-*`の値は隠れた項目を先頭の選択肢として計算します。
  式がLuaエラーになる場合は、保存値を戻せるよう項目を表示したままにします（Qwilightでは読み込み自体が失敗します）。

`SkinDocument.propertyLabels`はこの表示名のためのBMZ拡張です。項目名をキーに、`name`と`items`へUI言語コード（`ja` / `ko` / `en`等）ごとの文字列を持ちます。
設定画面は表示言語の[フォールバック](i18n.md#フォールバック)順に探し、なければ保存名を表示します。

**位置・サイズの調整**

スキンが`lambda`で`true`にした調整だけを「位置・サイズ」へ追加します。値はQwilightの設定と同じLuaのグローバル変数へ渡し、
宣言していない調整は0です。Qwilightのスライダーと同じく各値を±1000 pxへ制限します。

| スキンのフラグ | BMZの項目名（ID） | 値 | Luaの変数 |
|---|---|---|---|
| `setMainPosition` | Qwilight lane position（1001） | x | `mainPosition` |
| `setNoteLength` | Qwilight note width（1002） | w | `noteLength` |
| `setNoteHeight` | Qwilight note height（1003） | h | `noteHeight` |
| `setHitNotePaintArea` | Qwilight hit effect size（1004） | w | `hitNotePaintArea` |
| `setJudgmentMainPosition` | Qwilight judgment line position（1005） | y | `judgmentMainPosition` = 600 − y、`judgmentMainPositionV2` = 1 − y / 600、`judgmentMainPositionV3` = −y |
| `setBandPosition` | Qwilight combo position（1006） | y | `bandPosition` = −y |
| `setJudgmentPaintPosition` | Qwilight judgment position（1007） | y | `judgmentPaintPosition` = −y |
| `setJudgmentVisualizerPosition` | Qwilight judgment visualizer position（1008） | y | `judgmentVisualizerPosition` = −y |

- BMZのyは他のオフセットと同じく正の値で上へ動きます。Qwilight本体の値とは符号が逆です。
- 値はPlay枠ごとに保存します。Qwilightでキー数別に保存するレーン位置・ノーツ幅・高さ・ボム範囲に加え、
  Qwilightでは全キー数共通の判定線・コンボ・判定・判定ビジュアライザーの位置も、BMZではキー数ごとの値です。
- IDはbeatorajaの描画時offsetの範囲外で、描画中の要素へ直接は適用しません。

共通の位置・サイズの項目は、変換後の要素があるものだけを表示します。
「All offset(%)」は画面全体、「Notes offset」（30）はノーツ、「Judge offset」（32）は判定画像とコンボ数（`band`）へ適用します。
対応する要素を描画しない「Judge Detail offset」と「Bar Line offset」は表示しません。

**レーンカバー（Qwilightのveil）**

- BMZのSUDDEN+でレーンの上側を隠すときに描画します。カバーの下端がSUDDEN+の位置になり、1Pのレーン幅に合わせて縦横比を保ちます。
  DPでは2P側にも同じ画像を描きます。SUDDEN+を使わないときは描画しません。
  描画順は`pipeline`の50（VeilDrawing）で、ない場合はノーツ（7）の直後です。
- Qwilightのカバー高さ・終了時に下ろす設定・プレイ中の開閉は再現せず、BMZのSUDDEN+の量と操作を使います。
- 画像はBMZが追加するオプション「Lane cover image」で選びます。既定の「Skin」はスキンの`_GetVeil()`（既定は`Veil`）が返す
  ZIP内`Drawing/`の画像です。`[a, b]`の形で複数ある場合は読み込みごとに1枚を選びます。該当する画像がなければ描画しません。
- 設定画面の「レーンカバー画像を追加」で選んだ画像は、`<data>/skins/qwilight-veil/`へコピーし、すべてのQwilightスキンで共有します。
  png / jpg / jpeg / bmp / webpの32 MiBまでで、同名の別画像は`名前 (2).png`のように保存します。
  オプションには「Random」（追加画像から読み込みごとに1枚）と各画像のファイル名が加わります。
  選んだ画像を削除・移動した場合は、スキンの画像に戻ります。「選択中の追加画像を削除」はBMZのコピーだけを削除します。

## JSONのinclude展開

JSONスキンのincludeは、循環参照と深さ64以上の展開を読み込みエラーにします。
1回の展開全体で、元の文書を含むファイル読み込みは4,096回、展開後のJSONは1,000,000ノードまでです。
同じファイルの再includeも読み込み回数に数え、配列・オブジェクト・スカラーをそれぞれ1ノードと数えます。
上限を超えた場合は読み込みを中断し、超えた上限をエラーに記録します。

## 画像の拡大縮小

画像destinationの`filter=0`（既定）は最近傍、非0は線形補間を使います。
最近傍では切り出し範囲のpixel境界を保持し、整数倍表示でも各画素を同じ幅で描きます。
線形補間では、隣の素材が混ざらないよう切り出し範囲の内側に半画素の余白を設けます。
通常画像・回転画像に共通です。

JSON / Luaの`image`の切り出し範囲が元画像からはみ出す場合は、beatorajaの`TextureRegion`と同じく
範囲を詰めず、はみ出した部分には元画像の端の画素を引き延ばします（libGDXの既定のClampToEdge）。
ModernChicのオプション画面は、3000 pxの画像に対して240 pxの窓を下端から204 pxはみ出して切り出します。
LR2の`SRC`はLR2の仕様どおり、はみ出した分を元画像の内側に詰め、
残った範囲をDSTの大きさへ引き伸ばします（従来どおり）。`w` / `h`が負の場合は、どちらも始点から
元画像の端までです。

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

## LR2の位置調整

`#CUSTOMOFFSET`から生成する位置調整の定義は、件数にかかわらず配列として保持します。
Lua / LR2の数値変換で、1件だけのoffset定義や数値画像のoffset定義を単一のオブジェクトへ
畳み込みません。数値フィールドの`x = {10}`等は従来どおり単一の数値として扱います。

## LR2の条件分岐

`#IF` / `#ELSEIF` / `#ELSE` / `#ENDIF`はLR2と同じく、
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
CSVファイル名の単一wildcardが対象で、未選択のディレクトリ名のwildcardとDXA内CSVは対象外です。
一致するCSVが無い場合は、親ディレクトリの許可境界を検査してから、呼出元とコマンド番号付きで
警告し読み込みを続けます。root外参照や壊れた親リンクは、欠損wildcardでもエラーです。

`#CUSTOMFILE`の`*|1P|` / `*|2P|`等のタグは、設定項目の照合時には区別し、実際の
ファイルパスからは取り除きます。タグの後の拡張子やinclude先の相対パスは保持します。
タグ付きのフォルダー選択は、既定値・保存値・`Random`を受け付けます。
`Random`は項目ごとに1回のロードで1度だけ選び、同じ項目を使うinclude間と解像度推定で
共有します。この場合も文書のdecode cacheは利用しません。

## LR2のnumber / option

LR2とbeatorajaで同じ番号の意味が違うnumber / optionは、変換時にLR2専用の参照へ移し、
LR2のnumber / optionの定義で評価する（number `93_000+ID`、
option `94_000+ID`、否定は符号を維持）です。意味が一致する番号はbeatorajaの参照のまま使います。
変換の表は[refs.rs](../crates/bmz-skin/src/lr2/refs.rs)に集約します。

| 種類 | ID | 画面 | LR2の定義 |
|---|---|---|---|
| number | 100 / 107 / 108 / 109 / 114 | Play、Result（1P / 反転後の自分側） | LR2式スコア、2%刻みのゲージ、2P（SPではターゲットの現在進捗）とのEX差、次ランクまでの差（負値、MAXは0）、POOR＋空POOR |
| number | 154 | Result / Course Result | 自分のEX − 次ランクの整数境界です。AAAではMAXとの差を負値で表示し、MAXは0です。JSON/Luaの同番号の正値とは別 |
| number | 163 / 164 | Result / Course Result | 終了した譜面・コースの長さの分 / 秒です。Playの残り時間とは別 |
| number | 173 / 175 / 176 / 178 | Result / Course Result | 更新前の自己ベストの最大コンボ / 今回との差、最小BP（MISS COUNT）/ 今回との差です。自己ベストが無い初回は0として表示し、差は今回の値になります（beatoraja / JSON / Luaの同番号は非表示） |
| option | 100..105 | Result / Course Result | 更新後の自己ベストのクリア種別（未取得なら今回のクリア）です。ASSIST系をEASY、EX-HARDをHARD、PERFECT / MAXをFULLCOMBOへ畳む |
| button | 40 | Result | 今回のゲージ種別です。コース曲間リザルトでランプがNoPlayに丸められた場合（FULLCOMBO未満で通過）は非表示です。option 90 / 91は実際のクリア / 失敗のまま |
| number | 92 / 93 / 94 / 95 | Select / Decide | 選択中の曲のIR順位、参加人数、EASY以上のクリア率、ライバルとのEX差です。フォルダでは非表示 |
| number | 12 | Select / Decide | 判定タイミング（ソート変更中も同じ） |
| number | 45..49 | Select / Decide | 選択中の譜面と同じフォルダにある各難易度（BEGINNER..INSANE）の最高レベルです。難易度が無い欄は非表示 |
| option | 500..519 | Select / Decide | 同じフォルダに各難易度が無い / ある / 1譜面 / 複数譜面 |
| option | 520..565 | Select / Decide | 各難易度のレベルバーのランプ（`520 + 10 × 難易度 + LR2ランプ`）です。同じ難易度が複数あれば最良のクリア |
| option | 70..79 | Select / Decide | 各難易度のレベルが点滅しきい値以下（70..74）/ 超過（75..79）です。しきい値はLR2の既定値（5 / 10鍵9、7 / 14鍵12、9鍵42） |
| bargraph | 105..109 | Select | 各難易度のレベル ÷ 点滅しきい値（LR2のアニメーションは省略） |
| option | 118..149 | Select / Decide | 選択中の譜面を過去にクリアしたオプション（LR2の`op_history`）です。スコア履歴のオート以外のクリアから、NORMAL以上はゲージ（118..123）とRANDOM（126..131）、EASY系はEASYゲージ（121）、アシスト付きは142だけを立てます。HIDDEN / SUDDEN（134..141）は記録が無いため立ちません。BMZではアシスト付き（AUTO SCRATCHを含む）とH-RANDOM / ALL-SCRのクリアはランプだけを更新し、スコア履歴に残りません（[スコア保存](score-persistence.md)）。そのため142とH-RANDOM / ALL-SCRのbit（130 / 131）も実際には立たない |
| number | 250..259 | Select / Decide / Course Result | 選択中のコース（Course Resultでは今回のコース）の各ステージのレベルです。Course ResultではFAILEDで未プレイのステージも、コース開始時のライブラリのレベルを表示します。未解決のステージは非表示 |
| option | 580..589 | Course Result | コース定義のステージ数がN+1以上（`580 + N`）です。FAILEDで未プレイのステージも数えます。`#IF`はロード時、DSTの条件は描画時に評価する |
| option | 700..745 | Course Result | 各ステージの難易度（`700 + 10 × ステージ + 難易度`、0は未定義、1..5はBEGINNER..INSANE）です。未プレイのステージはライブラリの難易度を使います。`#IF`はロード時に評価する |
| number | 70 / 73 | Select / Decide | 選択中の曲の自己ベストのLR2式スコア / レート（%） |
| number | 210..219 | Select / Decide | IRのFAILED / EASY / CLEAR / HARD / FULLCOMBOの人数と率です。ASSIST系はEASY、EX-HARDはHARD、PERFECT / MAXはFULLCOMBOへ畳む |
| number | 212 / 214 / 216 | Result / Course Result | 拡張LR2のFAST / SLOW / コンボブレイク数（Playの210 / 212 / 214と同じ扱い） |
| number | 50..56 / 60..66 | 全画面 | EQ / FX / PITCHです。BMZに設定が無いため非表示（beatorajaの54 / 55はDPオプション / HS-FIX） |
| number | 290 / 291 | 全画面 | 最高 / 最低BPM（90 / 91と同じ） |
| text | 190..199 | Select / Decide | ランダムコースの設定です。BMZに該当が無いため空文字 |
| text | 60..65 / 70 / 74 / 76 / 80 / 84..85 | Select / Decide | BMZで変更できる設定の現在値（キーフィルター、ソート、難易度、RANDOM 1P / 2P、ゲージ、FLIP、HS-FIX、BGA、自動調整、レーン効果）です。text 84 / 85は同じ共通設定を表示します。文言はBMZの表記（例: NORMAL / HARD / HAZARD、H-RANDOM / ALL-SCR、SCORE、AUTO）を使います。キーフィルターはBMZ全体でLR2と同じ`ALL KEYS` / `7KEYS`等の表記にそろえています。変更できない設定（GHOST、SCORE GRAPH、BGA SIZE、画面設定、REPLAY等）は空文字 |
| number | 150 / 151、155..158 | Play | 自己ベスト / ターゲットのゴーストの現在進捗でのEXと率です。自己ベストはゴーストがあれば使い、無ければ線形換算です。ターゲットは線形換算 |
| number | 120..136（シングルプレイ） | Play | 2Pはターゲットのゴーストです。BMZはEXの進捗換算だけを持つため、EX・率・差・次ランク差・ノーツ数以外は0 |
| number | 120..136（シングルプレイ） | Result / Course Result | 自分と反対側は、ノーツ数だけを持つ空の2Pプレイヤーです。EX・率・判定数・コンボ・ゲージは0、ノーツ数は譜面（コース）のノーツ数、EX差と次ランク差はEX 0としてLR2の式で求めます。LR2のCourse Resultと同じで、単曲Result（LR2はターゲットの値）もBMZではこれに合わせます。AUTO BATTLE / G-BATTLEでは、LR2のBATTLE / G-BATTLEと同じく相手の最終成績（判定数・EX・率・コンボ・ゲージ・ノーツ数）を表示します。左右反転に追従する |
| number | 272 / 273 / 274 | Select / Decide | ライバルの理論値EX・レート・ノーツ数（同じ譜面のノーツ数から計算） |
| option | 52、620 / 622 | 全画面 | EXTRA OFF、ランキング表示OFF、ゴーストバトルOFFとして常にtrue（53、621 / 623はfalse） |
| option | 54..57 | 全画面 | 1P / 2PのAUTO SCRATCH OFF / ONです。プレイ画面は実際に自動演奏しているスクラッチレーン、他の画面は設定値 |
| option | 100..105 / 110..117 | Select / Decide | 選択中の曲のクリアランプ（beatorajaのクリア種別をバーのランプと同じ5段階へ畳む）とランク（プレイ済みのみ）です。描画時に評価する |
| option | 340..347 | Result / Course Result | 今回のスコアのランク |
| option | 20 | Select / Decide | オプションパネルを開いていません。描画時に評価する |
| option | 10 / 11 / 12 | Select / Decide | STYLEフィルターのDP / BATTLE / DPまたはBATTLEです。14 / 10鍵フィルター、またはBATTLEがD-BATTLE / SP-TO-DPのとき10と12がtrueです。選択譜面の鍵盤数からは判定しません。2人対戦BATTLEはBMZ未対応のため11はfalseです。`#IF`内も描画時に評価する |
| option | 21..29 | Select / Decide | 対応するオプションパネルを開いています。`#IF`内も描画時に評価する |
| option | 600..608 | Select / Decide | LR2の単一の`IRstatus`です。常にどれか1つだけが成立します。IR未設定・非対象の行は600、取得待ちは606、取得中は607（LR2の接続中）、取得済みで参加者ありは602、参加者0人（未登録の譜面を含む）は603、取得失敗は604です。BAN曲（605）は、BMZ IRが非公開・ブロック中の譜面への送信を拒否したリザルトでだけ成立します（BMZ IRの[アカウント・譜面の制限](ir.md#アカウント譜面の制限)）。BMZに無い一時読み込み（601）とIRビジー（608）はfalseです。beatoraja / JSON / Luaの600..608は従来どおりです（0人で602と603、失敗で604と608が同時に成立する。譜面の制限はbeatorajaに対応するpropertyが無いため604と608で表す）。`#IF`内も描画時に評価する |
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
`BAR_LABEL`は0=LN、1=ランダム分岐、2=地雷、3=CN、4=HCNです。
譜面の宣言と未定義LNのユーザー設定からHCN / CN / LNの順で選び、専用画像がなければ
LN画像を使います。これは譜面の特性表示であり、BMZのFORCE設定による判定変更とは区別します。
コースは所持している全構成譜面の特性を集約し、未所持の曲・コースにはラベルを出しません。
ラベルも最初のDST位置に従って描画し、スクロールとバーの座標へ追従します。
`BAR_TROPHY`は0=銅、1=銀、2=金です。コース定義の末尾から達成済みのトロフィーを選び、
対応する画像をコースバーへ描画します。未所持コースや曲バーには出しません。
ラベル・トロフィーの負のDST幅・高さはbeatorajaと同じく位置を移して正の大きさにします。
`BAR_GRAPH`のtype 0はフォルダー内の11種類のクリアランプ分布です。
元画像をSRCの格子順に11セルずつアニメーションの1フレームとして使い、画像側timerとcycleで
切り替えます。正の画像側timerがOFF・開始前なら先頭フレームを使い、
0以下のtimerは未指定として画面の経過時間を使います。
ランプ10から0の順に曲数の割合で横へ並べ、空フォルダーや通常の曲バーには描画しません。
位置・条件・timer・合成・補間はDSTに従い、宣言順に配置します。
内部の`lr2Distribution`はLR2変換用のフラグです。既存JSON / Luaの分布画像やダウンロード表示は維持します。
type 1のスコアランク分布（28種類）は未対応で、読み込み時に警告します。
Selectの`SRC_NOTECHART` / `DST_NOTECHART`は既存のノーツ分布グラフへ変換します。
幅・高さはSRCの指定を使い、DSTのY座標はbeatorajaと同じくグラフの下端として変換します。
表示遅延・背景・積み上げ順・隙間、条件・timer・offsetも保持します。
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
静止状態からの移動は初期接線を距離の1.5倍とし、smoothstepより速く動き始めます。
同じ時刻にまとめて入力された移動はこの初期接線を合計距離で再計算し、同じ行数の一括移動と一致させます。
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
別オブジェクトのDSTをkeyframeの間へ挟み、アニメーションに応じて前後関係を変えるLR2の描画順の補間は対象外とします。
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
option 54..57（1P / 2PのAUTO SCRATCH OFF / ON）は、プレイ画面では実際に自動演奏しているスクラッチレーン、選曲・リザルト等の他の画面ではLR2と同じく設定値で判定します。
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
対象のbuttonはLR2本体と同じく172を14KEYS、173を10KEYSとして扱う（LR2同梱のskinselect画像と、
buttonの番号から170を引くLR2の種別番号で確認）です。ECBEは14K表示のタブに173、10K表示のタブに172を割り当てているため、
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

LR2のDecideはLR2と同じく、スキップでtimer 3を開始し、`#CLOSE`の時間
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
Course Resultはこの補完を含むステージResultを引き継ぎます。

LR2の画像スロットは全シーンで共有され、`CONTINUE`は以前の場面が読み込んだ素材を使います。
BMZは直前の場面以外を保持しないため、BMZ独自の補完として、Decide / Play / Result / Course Resultの
LR2スキンでまだ空の`CONTINUE`番号を、同じテーマのLR2スキンの素材で埋めます。
参照する種別はLR2で直前に通る場面の順です。

| 読み込む場面 | 補完元（先に見つかった素材を使う） |
|---|---|
| Decide | Select、Result、Play |
| Play | Decide、Select、Result、Course Result |
| Result | Decide、Select（Playは上の補完で扱う） |
| Course Result | Result、Play、Decide、Select |

各種別はテーマ内のパス順で最初のLR2スキンを、既定のスキン設定で解決します。直前の場面から引き継いだ素材を優先し、
補完元でも`CONTINUE`の番号や、存在しないファイル（`dummy`等）は使いません。
補完元が削除・移動された場合や許可root外になった場合は、警告してその補完を省き、本体スキンの読み込みを続けます。
W-MIXのPlay開始とCourse Resultのシャッター（W-MIX Resultの画像28）、ECSEのPlayのフェード（ECSE Resultの画像27）、
3R / ECSEのDecideのレベル数字（Selectの画像0）、RED_BELT ResultのクリアマークがDecideから補われます。
別テーマを前提とするOA_DX+（RED_BELTの選曲・決定）と、テーマ内に素材の無いPLATINUMの画像27は補完しません。

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
LR2と同じ規則で時刻を評価します。JSON/Luaの既定評価は変更しません。
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
`DST_NOTE2`は横スクロール時には落下先Xとして扱います（[Playの見逃しノーツと判定領域](#playの見逃しノーツと判定領域)）。
LR2で縦方向の計算が残るLN/小節線も一貫して横へ流す点はBMZの補完動作です。
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

## Playの見逃しノーツと判定領域

JSON / Luaの`note.dst2`とLR2の`DST_NOTE2`は、beatoraja LaneRendererのPMS見逃しPOOR表示に使います。
`dst2`があるスキンでは、キーモードによらず、判定時刻を過ぎた未判定・BAD・POORのノーツを次の順で描きます。
PeacefulPlayの7 to 9版のように、7KEYS用のスキンでも適用します。

1. ノーツのSTOP終了から、遅い側の空POOR判定幅（PMSは175ms）まで判定ライン上に留めます。
2. その後はハイスピードによらず、1小節で判定ラインからレーン上端までの長さぶん落とします。
   落下中のSTOPでは止めます。
3. ノーツの下端が`dst2`を越えたら消します。`dst2`は判定ラインからレーン高さぶん画面下までの範囲に収めます。

PeacefulPlayのように`dst2`を判定ラインに置くスキンでは、ノーツは判定ライン上で止まり、落下を始めた時点で消えます。
落下中のノーツは、処理済みノーツの画像設定によらず通常のノーツ画像で描きます。
`dst2`があるスキンでは、PGREAT・GREAT・GOODで処理したノーツを判定時刻前でもその場で消します。
`dst2`が無いスキンでは見逃しノーツを描かず、処理済みノーツを判定時刻まで描きます。

内蔵9KEYスキンでは、同じ待機時間と落下量を使い、ノーツの下端がレーン下端を越えたら消します。
LIFTで判定ラインを上げると落下速度も短くなったレーン奥行きに従います。
奥行きが0になるLIFTでは待機終了後に消します。落下中は通常のノーツ画像で描きます。

横スクロール（`#HORIZONTAL`）では`dst2`を落下先Xとし、1小節で判定位置から右端までの長さぶん左へ進めます。
この横方向の規則はBMZの補完動作です。

judgeの`index`は判定領域です。beatorajaの`JudgeManager`と同じく、キーモード内のレーン番号を
「レーン数 / 領域数」で割って領域を選びます。
9KEYSで3領域のスキンでは、KEY1-3・KEY4-6・KEY7-9がそれぞれ領域0〜2（timer 46 / 47 / 247）になります。
割り切れない場合の余りのレーンは、最後の領域に含めます。

## Playのヒットエラービジュアライザー

JSON / Luaの`hiterrorvisualizer`は、直近100件のPGREAT / GREAT / GOOD / BADのタイミングを
使います。FASTは左、SLOWは右です。`width`は内部画像の横幅、`lineWidth`はその画像内の
線幅（1〜4）で、座標・線幅・高さをdestination全体へ拡縮します。画像の端を越えた線は
内部画像の範囲で切り、通常画像や他のグラフの座標計算は変えません。

- `hiterrorMode=1`で個別の判定線を描きます。0でも中央線とEMAは残ります。
- `windowLength`（1〜100）は表示する履歴数、`drawDecay=1`は古い線を縦の中央へ短くします。
- `colorMode=0`は`lineColor`を使い、古い線を薄くします。1は実際の判定幅で
  `PGColor` / `GRColor` / `GDColor` / `BDColor` / `PRColor`を選び、`transparent=1`でPOOR領域を透過します。
- `emaMode`は0=非表示、1=線、2=三角、3=線と三角です。`emaColor`を使います。
  EMAは初期値0から`alpha`で平滑化し、直近判定のindexが更新されたフレームに最新値を1回だけ
  取り込みます。BAD領域の境界・範囲外と未使用の値は取り込みません。
  スキン再install・Play再入場・時刻巻き戻しで保持値をリセットします。

`simple-play-simple`、`simple-play`、Rmz-skin、ModernChic、LITONE9等が同じ描画経路を使います。
実スキンの配置とGPU上の影響範囲の回帰テストは
[`hit_error.rs`](../crates/bmz-player/src/skin_loader/tests/lua/hit_error.rs)を参照してください。

## Playのノート分布グラフ

Playの`judgegraph`は、beatorajaの`SkinNoteDistributionGraph`と同じく1秒ごとのbucketを積み上げます。
`type`は0がノート種別（LN終端・LN本体・スクラッチ・通常ノート・地雷）、1が判定、2がFAST/SLOWです。
色・`backTexOff`・`orderReverse`・`noGap`・`noGapX`はResultと同じ描画を使います。

- ノート種別のbucketは譜面から1回だけ作ります。Resultと同じ集計です。
- 判定とFAST/SLOWのbucketは、全ノートを未判定の灰色で始めます。gameplay threadが判定イベントから
  更新し、beatorajaと同じく変化があるときに最短750 ms間隔で公開します。
- TIMER_PLAYがONの間は、現在位置に白線を描きます。幅は1秒を5 pxとしたときの3 pxです。
  Practiceの開始・終了カーソルはBMZの練習パネル側で表示するため、スキンのグラフには描きません。
- `delay`の既定値はbeatorajaと同じ500 msで、この時間をかけて左から表示します。
  bucketがない文書は従来どおり密度の単色グラフで描きます。

## PeacefulPlay互換のキーロガー

PeacefulPlayのKEY LOGGERは、Luaの関数timerとtrackerで押下を表示列ごとに記録します。
BMZはこの関数を毎フレーム呼ばず、`keylogger-*`のIDから同じ表示をBMZの入力履歴で描きます。
押下からの経過時間は、判定と同じ譜面の時刻で数えます。

表示列と入力レーンの対応は、ロード時に`keylogger-button-on-<列>`のdraw callbackが読む
key-on timer（`100`〜`117`）から求め、`keyloggerLane`（列と`keyonTimer`の組）として保持します。
7 to 9版では、レーン割り当ての設定に応じて皿と鍵盤の列が変わります。
対応が無いスキンでは、キーモードのレーン順（7KEYSは皿が先頭）で列を数えます。

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
- LR2フォントの`#T`ページは許可範囲内で欠損している場合だけ省き、残りのページを利用します。
  ページが復元されるとキャッシュを更新します。root外参照、壊れたリンク、不正なDXAはエラーです。
  `.fnt`のページは従来どおり必須です。
- Lua / JSON / LR2 CSV共通の`SkinPathContext`制約を適用し、アーカイブ実体の正規化パスが許可root外なら拒否します。
  フォント定義から参照するページ画像にも同じroot制約を適用します。
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

## Resultのお気に入り

単曲・コース曲間Resultでは、`act = 89`でプレイした曲、`act = 90`でプレイした譜面の
お気に入りを登録 / 解除します。image/indexの`ref = 89 / 90`も現在の状態を返します
（0=未登録、1=登録済み）。BMZはinvisible状態を使いません。number refの同番号とは別です。

選曲のFAVORITE SONG / FAVORITE CHART（既定F8 / F9）も同じ処理を使い、スキンに
ボタンがなくても操作できます。対象は結果のSHA-256と実際に開始した譜面IDから解決し、
選曲カーソルを参照しません。曲の登録単位・同じフォルダの別譜面からの解除は選曲と共通です。
曲のフォルダを特定できない場合、曲のお気に入りは変更しません。

キー・組み込みactともに入力待機中・退出演出中は変更しません。コース最終Resultでは
お気に入りを変更せず、image/indexの状態は0です。変更はプロフィールのcollection DBへ保存し、
成功後に通知・操作音・Resultの状態へ反映します。選曲へ戻る際に一覧を読み直します。

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

BMZ拡張名は`bmz_`で始まり、beatorajaの名前を上書きしません。IntegerPropertyでは
`bmz_select_mode_index`（[`19250`](#bmz-select-key-filter-index)）を追加しています。
beatorajaのIndexType名`mode`は従来どおり`ref=11`です。
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

## Decideのロード時の選択譜面

Decideへ入るときのskin再読込は、選択譜面の難易度option 150..155、play level（number 96）、
難易度表のtext 1001..1003をロード時の状態として渡します。難易度とplay levelはDecide描画と
同じ変換で評価し、数値1..5とBEGINNER / NORMAL / HYPER / ANOTHER / INSANEを151..155へ、
未定義を150へ対応付けます。beatorajaは選択曲のある状態でDecide skinを読むため、ModernChicや
mz-selectのようにロード時に難易度の色やラベルを決めるLua skin、`#IF`で難易度を分けるLR2 skinが
これに依存します。これらの値はdecode cacheの依存関係に含めます。

Decide進入以外の読込（skin設定パネル、profile切替等）は、表示中のDecide、なければSelectの
カーソル位置の譜面の値を渡します。起動時や、カーソルがフォルダ等で譜面がない場合は、
難易度未定義（option 150）、play level 0、空の難易度表textとして読みます。
beatorajaには選択曲のないDecideの読込がないため、これはBMZの決めごとです。
ModernChicのDecideはロード時の難易度色をnil確認なしで参照するため、難易度optionがすべてfalseだと
読込に失敗し、skin設定パネルにエラーが残っていました。実際のDecide表示は、従来どおり進入時に
選択譜面の値で読み直します。

## Playのロード時の譜面情報

beatorajaは解析済みの譜面でPlayスキンを読みます。ModernChicはロード時に曲名を連続プレーの判定キーへ
保存し、難易度で色を決め、LNの有無でエフェクターの部品を選びます。

Playスキンのdecodeは譜面ワーカーの結果を待ち、変換後の譜面の値をロード時の状態として渡します。

| 種類 | 値 |
|---|---|
| text | 10〜16（曲名・ジャンル・アーティスト）、1001〜1003（難易度表） |
| option | 150〜155（難易度）、172 / 173（LN）、176 / 177（BPM変化）、180〜184（判定ランク） |
| number | 90〜92（最大・最小・主BPM）、96（play level）、1163 / 1164（譜面長） |

難易度の変換はDecide、曲名と判定ランクはResultと同じ処理です。難易度表のtextは空でも値を渡すため、
`main_state.text(1003) == ""`で判定できます。これらの値はdecode cacheの依存関係に含めます。

## Select / Decideのロード時のプレイヤー統計

Select / Decideのskin読込は、profile全体の累計プレイ時間（number 17..19）、プレイ数・クリア数・
失敗数（30..32）、判定累計（33..37）、累計ノーツ（333）をロード時の状態として渡します。
値は描画時の同じrefと同じ変換です。beatorajaは全sceneでプレイヤー情報を保持しているため、
mz-selectのプレイヤーランク表示のように、ロード時に経験値やランクを計算するLua skinが
これに依存します。これらの値はdecode cacheの依存関係に含め、プレイ後にSelectへ戻ると
変化した値で再decodeします。profile切替では切替先profileの値を使います。
mz-selectの「今日の成果」とプレイ曲ログは、beatorajaではスキンが`io`で書いたファイルを
次回ロード時に読みます。BMZは`io`の書き込みをアプリ終了後まで残さないため、BMZ対応版のmz-selectは
`bmz`がある場合に[BMZ Daily Statistics Refs](#bmz-daily-statistics-refs)を参照し、
リセットボタンをevent `-10100`にします。

現在日付（number 21..23）はロード時もローカル時刻です。

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
対応済みfieldだけ永続Lua VMのruntime callbackへ残します。推論用VMの`main_state.option`は
falseを返すため、数字の`value`がoptionの真偽で戻り値を変える場合は、OFF側の経路だけを
式にしません。1つのoptionだけがONになる重み付き和（全option OFFで0）として表せる場合を除き、
runtime callbackで評価します。スキン開発・beatoraja比較では
`--lua-skin-runtime compat`を指定すると、次のfunction fieldを推論せず実行時に評価します。

- destination `draw`
- `value[]`, `text[]`, `graph[]`, `slider[]`の`value`
- `customTimers[].timer`（ID `10000..19999`）

beatorajaの`LuaSkinLoader`は関数・数値・文字列以外の`draw`をnullとして読むため、
`draw = false`のようなbooleanは条件になりません。BMZも条件を付けず、常に描画します。
ModernChicの`draw = not CUSTOM.OP.isMainBpm`はこれに依存し、現在のBPMを常に表示します。
関数が返す`false`は従来どおり非表示です。

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

ロード時の推論はtableのfieldを固定順（整数keyは値順、文字列keyはバイト順）で変換します。
推論で呼ぶfunctionがglobalを書き換えても、Luaの`pairs`順（VMごとのhash seed）によって
推論結果やcallback IDが変わりません。Luxe Flat Selectのランク画像とスコアレート帯
（`scorerate`の10分の1刻み）の`draw`は、`scorerate`を書き込む数字の`value`より先に推論しても、
選曲中の自スコアから評価する組み込み式へ変換します。数字の`value`も組み込み式になるため、
これらの`draw`をruntime callbackに残すと`scorerate`が更新されず表示されないからです。

毎フレームの入力・custom timer・custom event更新は、同じ`main_state`の設定を共有します。
入力、timer、eventの順に実行し、callbackごとのLua関数生成を避けます。callbackの実行回数、
永続VMの状態、同一フレーム内のtimer書き込み、命令数制限は維持します。

custom timerは描画前に宣言順で1フレーム1回更新します。`auto`では固定遅延・別名の
推論を維持し、推論できないtimerを永続VMへ残します。VMを使うスキンでは推論済みの
custom timerも同じVMで更新し、先行するtimerの変更を同じフレームで参照できます。
`main_state.set_timer(id, start_us)`は受動custom timerをON/OFFにします。組み込みtimerの
書き換えは拒否し、functionを持つ能動timerへの書き込みは無視します。
Luaの`main_state.timer(id)`はシーン基準の開始時刻（マイクロ秒）、`main_state.time()`は
現在時刻（マイクロ秒）を返します。描画側では経過ミリ秒へ変換します。OFF判定には
`main_state.timer_off_value`を使います。timer callbackのnilはbeatorajaと同じく開始時刻0、
例外・非有限値・命令数上限超過はOFFとして扱い、失敗の診断はcallbackごとに1回出します。

ターンテーブルの回転offset（1P: 1、2P: 2）はPlay画面の経過時間で更新します。
READY中も回転を続け、譜面時刻0をまたいでも回転角をリセットしません。
スクラッチ入力による累積回転量はgameplay側の確定値を加え、通常の回転速度は再生速度に依存しません。
開始前のキャンセルによる終了演出でも、画面時計による回転と押下 / 解放の経過時間を維持します。

KEYON / KEYOFF・HOLD・HCN・ゲージ増加 / 最大・FULLCOMBOと拍同期のtimerは、
描画時点の譜面時刻で更新します。新しいsnapshotを受け取れないフレームでも、保持した
イベントの開始時刻から経過時間を求め直します。PeacefulPlay互換キービームは押下 / 解放の
開始時刻で同じ入力を識別し、LN解放後の残光抑制をsnapshotの再利用や再生速度で解除しません。

Play中のボム・KEYON・判定・PLAY等のtimerは譜面時刻、`main_state.time()`は画面の経過時間で
数えるため、両者から逆算した開始時刻はフレームごとに揺れます。beatorajaは1回の起動につき
同じ開始時刻を返し、ModernChicのプレイ状況詳細モードや攻撃演出は開始時刻の変化で打鍵・判定を
数えます。runtime VMはtimerごとに前フレームの値を覚え、同じ起動の間は前回の開始時刻を返します。

- 判定・ボム・HOLD・KEYON・KEYOFF・HCNのtimerは、経過時間で起動を見分けます。経過時間は同じ起動の間
  増えるだけなので、前フレームより2 msを超えて減ったときだけ再起動とします。入力オフセットが正だと
  経過時間が0のまま続き、snapshotの古さで開始時刻が数ms動くため、開始時刻の比較では打鍵を数えすぎます。
  前回の起動から1フレーム以内に再起動した場合は、区別できないことがあります。
- それ以外のtimerは、前フレームの開始時刻から2 msを超えて変わったら再起動とします。
  TIMER_PLAYやRHYTHMは再生速度やBPMで開始時刻が動くためです。

`set_timer`等のLua custom timerはこの対象外です（[回帰テスト](../crates/bmz-skin/src/tests/read_helpers.rs)）。

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
使う場合も実行時の時計と状態を参照します。`customEvents`の`condition` / `action`も同じです。
destinationへ直接指定した任意timer functionはこの対応には含まれません。

`timer_observe_boolean`はスキンの本体（`main`）の実行中に呼ばれ、その場で観測関数を推論します。
推論はprobe用の値で`main_state`を差し替えるため、終わったら読み込み時の値と依存関係の記録を
元に戻します。戻さないと、それ以降の本体が読む`main_state.option` / `number` / `text`が既定値になり、
ModernChicのResultは`option(51)`（オンライン）がfalseになってIRメニューを組み立てませんでした
（[回帰テスト](../crates/bmz-skin/src/tests/cases_03.rs)）。

destinationの`timer`へ`timer_util.timer_observe_boolean`の結果を指定し、観測関数がLuaの変数
（`act`で切り替えるメニューの開閉状態など）を読む場合は、式へ変換できません。この場合は
runtime VMのdestination timer callbackとして観測関数を毎フレーム評価し、falseからtrueに
変わった時刻をtimerの開始時刻とします（beatorajaの`TimerUtility`と同じ）。`act`と同じVMで
評価するため、クリックで変えた状態を参照できます。`main_state`だけを読む観測関数は従来どおり
`dynamicTimer`の式へ変換します。destinationの`dst`内やimageのアニメーション用`timer`など、
runtime callbackのない位置に指定した場合は常にOFFのままで、読込警告を出します。
ModernChicのSelectのサイドメニュー・ヘルプ、Result / Courseのメニューがこれに依存します
（[回帰テスト](../crates/bmz-skin/src/tests/cases_03.rs)）。

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
リセットしません。これは既存パネル切替の互換処理です。それ以外の`act` / `customEvents`は
[Lua actとcustomEventsの実行](#lua-actとcustomeventsの実行)で扱います。

Lua sandboxはロード時の互換APIとして`os.clock` / `os.date` / `os.time` / `os.difftime`を提供します。
`os.time`の日付tableは標準Luaと同様にローカル時刻として正規化し、正規化後の値を
tableへ書き戻します。引数なしでは現在時刻を返し、`os.date`は`!` prefixでUTCを選べます。
`os.date`の書式は[Lua APIの互換範囲](#lua-apiの互換範囲)を参照します。libGDXの
`Controllers.getControllers()`は入力のない空配列として`size` / `get()` / `first()`を
提供します。`io`の書き込みは[メモリ上のスキンファイル](#メモリ上のスキンファイル)に残ります。初回document構築は、蓄積データを
Luaで解析するResult skinも通せる専用の命令数上限を持ち、runtime callback単位の上限は
それより小さい値とします。

命令数の予算はコルーチン間で共有し、`resume` / `wrap` / `close`にも適用します。
再開・closeは最低1フック間隔分の予算を消費します。フックが無効になる`__gc`の登録は
禁止します（通常のGCは利用できます）。
上限超過は`pcall` / `xpcall`では回復できません。通常の例外処理とコルーチンのyieldは
利用できます。`xpcall`のエラーハンドラーは、対象関数の巻き戻し後に同じ予算で実行します。
`string.find` / `match` / `gmatch` / `gsub`は、Lua 5.4由来のnative照合内部で、
探索・バックトラック・比較・コピーの作業量を数えます。Lua命令数上限は変更せず、
native作業量はロード・推論それぞれ合計6400万、callbackあたり800万、フレーム合計3200万を
上限とします。呼び出し・iterator再開は最低1024単位を消費し、`pcall`やコルーチンで予算を
リセットできません。フレームが変わればiteratorを引き続き利用できます。
byte単位のパターン、キャプチャ、balanced / frontier、関数・table置換を維持し、
上限を超える照合は途中でエラーになります。置換関数からのyieldの可否も標準Luaに従います。
Lua VMの256MiB上限とは別に、`io.open` / `io.lines`が保持する読み込みバッファは
VMごとに合計32MiBまでとし、closeまたはGCで解放します。Lua tableから文書への変換は、
キー・値の文字列のコピーを合計64MiBまでに制限します。同じ文字列を複数の要素から
参照する場合も、変換で生じるコピーごとに数えます。

判定の存在を表すoption `2241..2246` (PGREAT / GREAT / GOOD / BAD / POOR / EMPTY POOR)
はリザルト画面だけでなくプレイ中も現在の判定数を参照します。これにより、LITONE11などが
`-2244` / `-2245`で制御するフルコンボ演出はBAD/POOR発生後に表示されません。

skin catalogのJSON候補はinclude・条件・数値正規化後に数値の`type`を明示した文書だけを受理します。
これによりskin配下のplayer dataやparts JSONが既定値`type=0`のPlay 7KEYS skinとして
候補へ混入することを防ぎます。Selectの曲行テキストIDは区切り文字を無視して`bartext`を
照合し、`bar_text` / `folderbar_text`形式もbar textへ割り当てます。Lua skinのslider / graph
で使う`isRefNum`はJSONのbooleanに加えて互換表現の整数`0` / `1`も受理します。

### Lua actとcustomEventsの実行

beatorajaの`SkinLuaAccessor.loadEvent`・`CustomEvent`・`MainState.executeEvent`と同じく、
functionの`act`と`customEvents`を永続VMで実行します。`auto` / `compat`で共通です。

- 定数の`act`と、既存の推論で組み込みイベントへ置き換えられる`act`（固定IDを返すfunction、
  Resultパネル切替等）は組み込みイベントのままです。それ以外のfunctionはクリック時に
  `act(arg1, arg2)`として呼びます。`arg1`は`click`に従う方向で組み込みイベントと同じ値、
  `arg2`は0です。closureとmodule stateは描画callbackと共有します。
- クリックはそのsceneの次フレームで、custom timer・`customEvents`の更新と描画より先に
  実行します（beatorajaの入力処理から`Skin.updateCustomObjects`への順）。1フレームに
  受け付けるクリックは64件までです。Select / Decide / Play / Resultで受け付けます。
  Decide / Playには組み込みのクリックイベントがないため、Luaの`act`だけを実行します。
  ResultのLua `act`は左右・中央ボタンを受け付けます。組み込みイベントとsliderは従来どおり
  左ボタンだけで操作し、退出演出中は新しい`act`を受け付けません。
- クリック判定は、最後に描画したフレームのLua `draw`の結果とcustom timerを使い、判定の
  ためにLuaを再実行しません（beatorajaの`mousePressed`が直前の`prepare`の状態を使うのと同じ）。
  Lua `draw`で表示を切り替えるボタンも、表示中だけクリックできます。Selectの曲バーは
  行ごとに条件を評価するため、このLua `draw`の結果を使いません。
- `customEvents`は1フレーム1回、custom timerの更新後にID順で評価します。`condition`
  （function、またはoption ID。負のIDは否定）は`minInterval`の待機中も毎フレーム評価します。
  条件が真で、評価後に確認した最終実行時刻から`minInterval`ミリ秒以上経っていれば
  `action`を実行します。`condition`の無い定義は`event_exec`や数値`action`による呼出しで実行します。
  `action`が数値なら、custom event `1000..1999`は同じVM内で同期実行し、それ以外は
  そのsceneの組み込みイベントとして実行します（Select / Result）。数値による連鎖にも
  functionと共通の入れ子上限を適用します。
- `main_state.event_exec(id, arg1, arg2)`はcustom event `1000..1999`だけを同期実行し、
  他のIDは無視します。入れ子は16段までです（beatorajaは上限が無くstack overflowになります）。
- Result / Course Resultのtimer 2（fadeout）条件で、ロード時に`audioActions`へ変換できた定義は、
  従来の宣言的な経路で再生し、毎フレームは評価しません。`event_exec`からは実行します。
  他のscene・timerの音声イベントは永続VMで評価します。
- `act` / `action` / `condition`の例外は定義ごとに1回だけログに出して無視します
  （beatorajaの`loadEvent`と同じ）。命令数・メモリの上限は描画callbackと同じです。
- function の`act`や毎フレーム評価する`customEvents`を持つskinは永続VMを使うため、
  document cacheの対象外です。Lua-to-JSON変換は`act` functionを警告付きで除き、
  functionの`action` / `condition`は従来どおり出力しません。
- `act` / `action`内の`audio_play` / `audio_loop` / `audio_stop`は、そのsceneのskin音声として
  鳴らします。pathはskinの許可root内の既存fileに限ります。初回はbackgroundでdecodeし、
  decode中の命令は順に保留します。skin破棄後に完了したdecodeは登録せず、次のskinの
  音声を上書きしません。音量はbeatorajaと同じく0〜2に制限したうえで、
  `audio_play`はシステムSE、`audio_loop`はシステムBGMの音量を掛けます（最大1.0。
  beatorajaはどちらもシステム音量です）。sceneを離れるとloopを止め、単発の音は鳴らし切ります。
  1 sceneで実行時に読み込むfileは64個までです。描画callbackからの`audio_*`は無視します。
- `set_volume_*`はプロファイルのマスター（beatorajaのシステム音量）・キー・BGM音量を
  0〜1で変えます。Selectの音量スライダー（17〜19）と同じ扱いで、他のSelectの設定と同様に
  プロファイルへ保存します。同じフレーム内の`volume_*`は設定した値を返します。

### カタログとシステム音セットの走査

スキンの自動探索はアプリのresource/dataの`skins`を許可rootとします。
ファイルとディレクトリの実体を先に検査し、root外のリンク先は列挙・ヘッダ評価しません。
明示指定されたroot自体がsymlink / Windows junctionの場合は、その実体を許可します。
ディレクトリから読み込むPMchara等でも指定時のroot名を保持し、macOSの`/var`と`/private/var`、
Windowsの短縮名などが異なっても、同じ許可rootとして解決します。
配下の子リンクを理由に許可rootを追加することはありません。

走査rootごとにcanonical pathで訪問済みディレクトリとファイルを管理し、自己・親・相互リンクと
別名による重複を省きます。各ディレクトリでは通常の実体名、別名の順で名前をソートし、
保存する論理パスを安定させます。解決不能・壊れたリンクは省きます。
1回の探索は深さ64、列挙entry数100,000までです。上限到達時は残りを省いて警告します。
LR2 CONTINUEの関連スキン探索も同じ検査と上限を使います。
拒否理由の代表例と件数、既訪問による省略はまとめてdebugログへ記録します。

スキンの`resource:skins/` / `data:skins/`の相対パスとorigin、名前順の候補ソートは維持します。
resourceとdataのrootが異なる場合はそれぞれ独立して走査します。
サウンドセットは対応するBGM / SE音源が最初に見つかったディレクトリをルートとし、
その直下の音源付きフォルダをバリエーションとして1つ選びます。補完用SEセットは`clear`マーカーで検出します。
対応する固定名と`.wav` / `.ogg` / `.flac` / `.mp3`はASCIIの大文字小文字を区別せず検出します。
親子の補完、RESULT BGMと全SEの優先選択、`.loop`指定は[サウンドセット仕様](system-sound.md)を参照します。
`bgm_dir` / `se_dir`設定キーは維持します。親子両方に音源がある構成は、独立した複数セットではなく1セットとして扱います。
システム音の許可rootには設定されたBGM / SE / fallbackのディレクトリを使います。
スキン用rootは流用せず、探索後の音源参照にもサウンドセット側の許可範囲を引き継ぎます。

### スキンのファイル参照と外部スキンの許可

`bmz-skin-assets`の`SkinPathPolicy` / `SkinPathContext`を、Lua / JSON / LR2 CSVで共有します。
本体・ヘッダ・JSON/LR2 include・Lua module・画像/アニメーション/動画・音声・フォントとページ画像・
PMchara・DXA実体・wildcard・保存されたファイル選択・runtimeでの追加参照が対象です。
許可root内の共有資源、`skin/`エイリアス、大小文字互換、Unicodeパスは利用できます。
OSフォントfallbackや譜面側のBGA/音声はアプリが別途管理し、スキンが任意のパスを指定して
その権限を得ることはできません。

Lua / JSONの`source.path`と`font.path`では、`filepath`で選択した既存フォルダを
`parts/*/image.png`のようなパスに当てはめます。そのフォルダに素材がなければ、
beatorajaと同様に欠損として扱います。フォルダ選択を解決できない場合はwildcard探索に進みます。

アプリの通常の読み込みでは、外部entryの親を自動承認しません。設定のスキン欄へ外部ファイルを
指定して「この外部スキンを許可」を押すと、そのentryと実体の親フォルダだけを追加承認します。
許可前のプレビューや設定項目取得では外部スキンを評価しません。
ホーム、アプリ全体、それらの祖先、ファイルシステムrootを親とするファイルは専用フォルダへ移してください。

承認はプロファイルの`skin.external_access`へ、指定した絶対パスをキーとして保存します。
`entry`は実体の絶対パス、`roots`はこのentryだけに追加する許可rootです。
共有フォルダが必要なら、アプリを終了して同じentryの`roots`へ必要な範囲を追加します。
例えばUIで許可したentryへ共有フォルダを追加する設定は次の形です。パスは実際の絶対パスへ置き換えます。

```toml
[skin.external_access."/opt/my-skins/theme/select.luaskin"]
entry = "/opt/my-skins/theme/select.luaskin"
roots = ["/opt/my-skins/theme", "/opt/my-skins/shared"]
```

別のスキンにはこの許可を引き継ぎません。リンク先entryが変わった場合は再許可が必要です。
「外部スキンの許可を解除」で承認を削除でき、許可変更時には読み込み世代、document/資源キャッシュ、
Luaメモリoverlay、描画用Lua VMを無効化します。
進行中のworkerの結果は古い世代として破棄し、新しいcacheへ戻しません。
既に開始したworkerやOSの読み込みを、その瞬間に強制中断する保証ではありません。
古いプロファイルのパス文字列だけでは承認を判別できないため、自動移行しません。
ライブラリ外の既存指定は再許可するか、スキンをライブラリ内へ移してください。
CLIでプロファイルのスキン指定を上書きする場合も、利用するentryの承認情報が必要です。

低レベルの`for_entry`/単一ファイルload APIは、呼び出し側が明示選択したentryの親だけを許可する入口です。
自動探索・保存済みプロファイルの処理には使わず、アプリが作る明示的なcontextを渡します。
contextが無い場合に無制限なパス解決へ戻ることはありません。
JSON includeの循環検出と深さ64の上限を維持し、LR2 includeは現在の読み込み経路で循環を検出し、
深さ64・処理ファイル数4,096までとします。別の正常な分岐から同じincludeを使うことは可能です。

未作成パスも最も近い既存の親の実体を検査します。Luaの書き込みはメモリoverlayだけで、
許可root内でもディスクへ保存しません。許可されない書き込み先は既存の互換動作どおり破棄します。
この境界は静的なリンク構成を対象とします。検査後のディレクトリ差し替えなどのTOCTOUを
完全には防ぎません。OSのhandleを使った隔離やプロセスsandboxを提供する保証ではありません。

### Lua Module Path

skin library root配下のLua skinへ公開する初期`package.path`は、過去のbeatorajaと同じ
`?.lua;skin/<skin entry directory>/?.lua`形式にします。sandbox内の実ファイル解決はこの論理
パスをlibrary rootに対応付け、`require` / `dofile` / `loadfile`がroot外へ出ないことを
別途検証します。明示選択用の低レベルAPIはentryの親だけを許可します。
共有モジュールを使う呼び出し元は、許可するlibrary rootsをcontextへ明示します。

標準ライブラリの迂回経路も閉じています。`require("os")` / `require("io")` と
`package.loaded.os` / `io` はglobalと同じ限定版を返し、`package.loaded.debug` はありません。
`package.searchers` は `package.preload` を引く1件だけで、`package.searchpath` はありません。
`require` は `package.loaded`、組み込みモジュール（`main_state` / `timer_util` / `event_util` / `luajava`）、
`package.preload`、sandbox内の `package.path` の順に探します。
`string.dump` はなく、`load` は常にテキストモード（`mode="t"`）で読み込みます。
`dofile` / `loadfile` / `require`、文字列の`act` / `event`もバイナリチャンクを受け付けません。
`act`の定数推論では、callbackの複製を`os` / `io` / `package` / chunk loaderを持たない
別VMで、命令数・メモリの上限付きで実行します。

### Lua APIの互換範囲

譜面長のnumber `1163` / `1164`（`chartlength_minute` / `chartlength_second`）は、
Playでは、実際の`#RANDOM`分岐・先頭ノーツ余白・譜面変換を反映した全体長を返します。
beatorajaの`BMSModel.getLastTime()`と同じく、ノーツ・不可視ノーツ・BGM・通常/Layer BGAの
最終配置を含み、PoorのみのBGA、PCMの余韻、リザルト待機時間は含みません。
BMZ追加のLayer2（0A）も曲長には含めず、終了待ちでの従来の扱いは維持します。
Practiceは区間・速度の適用前の全体長です。経過時刻、残り時間、ノーツ表示時間、終了判定とは独立しています。
`1163 = (length_ms / 60000) % 60`、`1164 = (length_ms / 1000) % 60`で、整数除算します。
Playの譜面情報がまだなければLuaは`INT_MIN`、数値画像と`numberRef`テキストは非表示です。

Playスキンのデコード依頼は先行できますが、Luaロードは変換済み譜面の通知を待ちます。
同じ譜面からロード時定数、runtime callback、数値画像、`numberRef`へ値を渡します。
通知はWAVロードより前に行い、再解析を追加しません。Retry・Replay・コース次曲も
要求世代ごとの通知を使い、キャンセルと失敗時には待機を解除します。Offlineも同じ長さを使います。
Select・Decide・Result・Course Resultの長さの取得元は従来どおりです。
beatorajaとの残る画面別の差異と検証範囲は[作業記録](../notes/2026/2026-11-02-skin-runtime-properties.md#2026-11-02-pr-60追補譜面長の互換修正)に記載します。

text `1030` / `1031`（`songhashmd5` / `songhashsha256`）は元の譜面ファイルの
MD5 / SHA256を小文字の16進数で返します。Selectの選択譜面、Decide / Playの対象譜面、
Resultの終了した譜面を参照します。コースResultでは最後にプレイした譜面を参照します。
難易度表の未所持譜面も既知のハッシュを返し、譜面またはハッシュがなければ空文字です。
ロード時に読む場合も同じ譜面情報を渡します。選曲変更に追従する処理はcallback内で取得してください。
同じ番号のoption `1030` / `1031`（ランダム選曲 / コースバー）とは独立しています。

beatoraja（`.local/beatoraja`の最新版）とLR2oraja Endless Dream（beatoraja 0.8系の
`MainStateAccessor`を継承）に共通するLua APIを中心に提供します。両実装の比較と
インストール済みスキンでの検証は[作業記録](../notes/2026/2026-10-21-lua-api-compat.md)を参照します。
既存APIの名前・引数・戻り値は変えず、追加方式で拡張します。

`main_state`（`require("main_state")`）:

| API | BMZの戻り値 |
|---|---|
| `option` / `number` / `float_number` / `text` | 数値IDと名前を受け付けます。[名前によるプロパティ参照](#名前によるプロパティ参照)を参照します |
| `offset` / `event_index` / `judge` | 数値IDだけを受け付けます |
| `timer` / `time` / `timer_off_value` / `set_timer` | マイクロ秒です。OFF値はBMZでは`-2147483648`です（beatorajaは`Long.MIN_VALUE`） |
| `exscore()` | number 71と同じ整数です |
| `rate()` / `rate_best()` / `rate_rival()` | float_number 111 / 112 / 115と同じ値です |
| `exscore_best()` / `exscore_rival()` | number 150 / 121と同じ整数です。ロード時の依存値と推論も同じです |
| `gauge()` | Play中の現在ゲージです。他のsceneでは0です。ロード時は0で、推論せずruntime callbackで評価します |
| `gauge_type()` | 現在のゲージ種別です |
| `volume_sys` / `volume_key` / `volume_bg`、`set_volume_*` | 音量の読み書きです（0.0〜1.0）。runtime callbackでは現在の設定を読み、`set_volume_*`で設定を変えます。ロード時の`set_volume_*`は設定を変えずtrueを返します |
| `audio_play` / `audio_loop` / `audio_stop` | ロード時の推論では宣言的な音声命令へ変換し、`act` / `customEvents`内では実行時に鳴らします。音量の省略は1.0です |
| `event_exec(id, arg1, arg2)` | custom event `1000..1999`を実行してtrueを返します。ロード時は実行しません。[Lua actとcustomEventsの実行](#lua-actとcustomeventsの実行)を参照します |
| `audio_preload` / `audio_dispose` | trueを返します。音声は宣言から読み込むため処理はありません |
| `numbers` / `timer_is_on` / `timer_is_off` / `timer_elapsed*` / `screen_width` / `screen_height` | beatoraja最新版のAPIです |
| `key_pressed(key)` | キーボードのキーを押しているかです。`key`はlibGDXのキーコード（数値・数値の文字列）か`Input.Keys`の名前（`"A"`、`"Space"`、`"L-Shift"`、`"Escape"`等。大文字小文字を区別）です。未知の名前・負の値はfalseです。ロード時はfalseで、推論せずruntime callbackで評価します |

整数のプロパティ（`number`、`judge`、`exscore*`）はinteger、小数のプロパティ
（`float_number`、`rate*`、`gauge`）はfloatで返します。`number` / `option` / `text` /
`float_number`の引数に小数を渡すと0方向へ切り捨てます。nilやbooleanはLuaエラーです
（beatoraja最新版は0 / false / 空文字、EDはID 0として扱います）。未知のIDや名前は
0 / false / 空文字 / 0.0です。

`skin_config.enabled_options`はヘッダの`property`順に選択中のopを並べた1始まりの
配列です。`#`、添字、`ipairs`、`pairs`、`table.concat`、`table.unpack`で読めます。
読んだopだけを文書の依存値に記録するread-only proxyのため、`rawget` / `next`では空です。

Luaのヘッダはbeatorajaの`LuaSkinLoader`と同じ型変換を使います。`property[].item[].op`は
数値文字列を含め0方向へ切り捨て、数値に変換できない値は0です。`property[].def`と
`filepath[].def`の数値・booleanは文字列へ変換します。数値の`def`をop指定とは扱わず、
項目名に一致しない既定値は先頭項目へ戻します。通常のJSONスキンの型検査は維持します。

Luaのdestination `op`に入れ子のtableやbooleanが混ざる場合、beatorajaの
`DestinationOption`と同じくその要素は条件にしません。入れ子をOR条件とは解釈せず、
残りの数値・名前付き条件を維持します。通常のJSONでの不正な入れ子は引き続きエラーです。

Luaの`act`は関数に加え`"switchMenu()"`のようなscript文字列を受け付け、クリック時に
永続VMで実行します。`customTimers[].timer`とdestination `timer`の文字列は
`return <文字列>`として評価し、関数が返ればその関数、それ以外は式を毎回評価します。
destinationの任意timer関数も描画時に開始時刻（microseconds）を取得し、OFF値・nil・
64bit時計はcustom timerと同じ規約です。クリック判定は最後の描画結果を使い、再実行しません。

Select / Resultの`slider[].event`のLua関数はFloatWriterとして扱い、トラック上の押下・
ドラッグ位置（0.0〜1.0のfloat）を次のフレームのcustom timer更新前に渡します。
関数はactと同じ永続VMで動き、入力順序を維持します。表示条件とtimerは最後の描画結果で
判定し、同じ`type`を持つ複数のLua sliderもドラッグ開始時の関数を区別します。

`timer_util` / `event_util`はbeatorajaの`TimerUtility` / `EventUtility`相当です。
`new_passive_timer`の`turn_on` / `turn_on_reset` / `turn_off`はtrueを返します。
`event_observe_timer_off`は、最初の評価でtimerがOFFでも発火しません（beatorajaは発火します）。
ロード時の推論が一度きりの音声イベントを誤って作らないようにするためです。

LuaJ（Lua 5.2系）との差:

- `bit32`はLuaJと同じLua 5.2の関数群を提供します。2^32を法として扱い、非整数は0方向へ
  切り捨てます。globalと`require("bit32")`は同じテーブルです。同名のglobalがある場合は
  置き換えません。`extract` / `replace`の範囲外のfield / widthはLuaエラーにします。
- `math.pow` / `ldexp` / `frexp` / `cosh` / `sinh` / `tanh` / `atan2` / `log10`はLua 5.4の
  互換関数として使えます。`unpack` / `loadstring`はLuaJ 3.0にもないため提供しません。
- `math.random`の引数はLuaJと同じく32bit整数へ0方向に切り捨てます。数値文字列も受け付け、
  第3引数以降は無視します。`random(0)`や32bitの区間幅が正でない範囲はエラーです。
  乱数生成器自体はLua 5.4のものなので、同じseedでLuaJと同じ列になる保証はありません。
- `os.date`はLuaJの`%a %A %b %B %c %d %H %I %j %m %M %p %S %U %w %W %x %X %y %Y %z %%`に
  対応します。正午の`%I`は`00`、`%U` / `%W`はLuaJの週番号です。引数なしの既定書式
  `%Y-%m-%d %H:%M:%S`と、未知の指定子を文字のまま返す動作は既存スキンの表示を
  変えないため維持します（LuaJは`%c`とエラー）。`os.difftime(t2, t1)`はfloat秒です。
- BMZはLua 5.4です。整数値のfloatは`tostring`で`5.0`になります（LuaJは`5`）。
  `string.format("%d", 2.5)`はエラーです（LuaJは切り捨て）。標準ライブラリの動作は変更しません。
- 数値callbackが`nil`を返すと、BMZは値を未取得として描画しません。beatoraja / EDは
  `toint()` / `tofloat()`で0として扱います。timer callbackの`nil`はBMZも開始時刻0です。

`key_pressed`のキーコードはbeatoraja同梱のlibGDX（`lib/gdx.jar`）の`Input.Keys`です（`Escape`は131、
`F1`〜`F12`は244〜255）。BMZはキーボードの物理キーを同じ番号へ対応付けます。テンキーの演算子とEnterは
LWJGL版と同じく`Plus` / `-` / `*` / `/` / `.` / `Enter`の番号になります。コントローラーのボタンは対象外です。
キー割り当てとは無関係に、ウィンドウがフォーカスを持つ間のキーボード入力だけを反映します。

BMZ独自のAPIは`bmz.option` / `bmz.get_option`、runtime VMだけの`main_state.float`
（`float_number`の別名）、`total_play_counts_in_session` / `total_play_notes_in_session` /
`score_date_sec_time`、名前付きプロパティです。

beatoraja最新版の`file_*`は[メモリ上のスキンファイル](#メモリ上のスキンファイル)で提供し、
HTTP通信と`open_url`は未対応です。
`http_get_async`は`nil, "HTTP disabled"`、`http_poll`は`status="failed"`・`status_code=0`・
空の`lines`・`error="HTTP disabled"`、`http_cancel`はfalseを返します。
beatorajaの通信無効時と同じ応答により、SocialSkin等がJava互換stubを実ワーカーと
誤認して待ち続けることを防ぎます。実際の通信は開始しません。
`luajava`の`bindClass` / `newInstance` / `new` / `createProxy`は入力のない無害なstubを返し、
Javaのオブジェクトを作りません。例外は`java.io.File`で、メモリ上のスキンファイルの一覧・作成に
対応する`listFiles` / `mkdir` / `mkdirs` / `exists` / `isDirectory` / `isFile` / `getName` /
`getPath`を返します（ModernChic Ver 4.2系のフォルダ一覧）。
パスは親子を結合したUTF-8表現で4 KiBまでです。`listFiles`の子を含む`File`がRust側に保持する
パスと管理領域には、Lua VMごとに合計32 MiBの予算があります。上限を超える生成はLuaエラーを返し、
テーブルとそのメソッドが回収されると予算が戻ります。

### メモリ上のスキンファイル

beatorajaのLua skinは`io`や`main_state.file_*`でスキンフォルダへファイルを書き、後で読み戻します
（ModernChicのレーンカバー・背景・キャラクターのローテーション、プレイ状況詳細モード、
プレイ履歴、Starseekerの打鍵数など）。BMZはディスクへ書かず、書いた内容をメモリに残します。
内容はLuaJと同じくバイト列のまま保持します。ModernChicは`string.sub(title, 1, 32)`のように
UTF-8の文字の途中で切った文字列を書き、読み戻して比較するためです。

- 読み込みは、appが渡す読取専用の仮想ファイル、スキンが書いたファイル、ディスク上のファイルの順に探します。
  `skin_config.get_path`が返す絶対パスも、スキンのroot内なら読めます。相対パスは従来どおり`..`を拒否します。
- `io.open(path, "w" | "a")`の書き込みは、閉じる前でも読み戻せます。`file:write`はファイルを返すため、
  `io.open(path, "a"):write(text):close()`の連結も使えます。`file:read`は`*l` / `*a` / `*n`と、
  `*`なしの形式、読み取りバイト数に対応します。
- `main_state.file_exists` / `file_mkdir` / `file_list` / `file_read_lines` / `file_write` /
  `file_append` / `file_clear` / `file_count_lines` / `file_delete`は、beatorajaの
  `SkinFileLuaApiExporter`と同じ戻り値です。`file_list`はディスクの一覧と書いたファイルを合わせ、
  パターンを正規化したパスにLuaの`string.find`で照合して一致部分を返します（beatorajaはJava正規表現に
  変換しますが、スキンが使う`Play.+%.png`等は同じ結果です）。
- 1回のロードではスキンを複数回実行します。後のロードへ残すのは、document VMの本体実行と、
  `act`・描画callback等の実行時の書き込みだけです。callback推論とruntime VMの本体の再実行の
  書き込みは、その実行の中だけで読み戻し、破棄します。ロードが失敗した場合も残しません。
- appは1つの保存領域をアプリ終了まで共有します。そのため、ローテーションの一巡や粘着情報、
  プレイ履歴は同じ起動中は引き継がれ、再起動すると消えます。保存領域を渡さないロード
  （動画出力、テストハーネス等）は、そのロードの中だけで読み戻します。
- 保存領域へ書くロード、または保存領域から読んだロードは、document cacheを使いません。
  ロード時の書き込みを省略しないためです。ディレクトリの一覧を読んだロードも同様です。
- 1ファイル8MiB、保存領域およびロード・推論の各一時領域は合計32MiBを上限とします。
  合計には内容のbuffer容量に加え、空ファイル・ディレクトリ・削除記録のパスと管理領域を含めます。
  各領域は合計4096件、要求パスと解決後のパスはそれぞれ4096bytesまでです。
  ロード結果を保存領域へ反映するときも、上限を超える変更はまとめて破棄します。
- 書き込み引数の合計と追記後のサイズを、連結用bufferの確保前に検証します。
  超える書き込みは`file:write`がnilとエラー文を、`main_state.file_*`とmkdirはfalseを返します。

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

5K/7Kの対戦表示では、判定領域をスキンの表示部品数にかかわらず1P・2Pへ分離します。
通常スキンへフォールバックして`judge.index=0`だけを描く場合も、相手の判定で自分の
判定文字・コンボ・FAST/SLOWやtimer 46/446を更新しません。専用スキンの
`judge.index=1`は相手側を表示します。対戦相手のいない通常DPの共通1領域・左右2領域や、
9KEYSの3領域はスキン定義に従います。

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

## Image Sources

beatoraja skinの`source.path`はPNG / BMP / JPEG / GIF / TGA / WebPに加え、libGDX
`PixmapIO`のCIMを読み込めます。CIMはzlib stream内のwidth / height /
Gdx2DPixmap formatとpixel bufferをRGBA8へ展開します。MILLIONDOLLAR RESULTの
主要atlasは配布時点から`.cim`のため、PNG fallbackへ書き換えずそのまま扱います。
JSON / Lua / LR2 CSV / PMcharaのsourceはすべて同じ規則で読み込みます。

2フレーム以上のanimated WebP / APNG / GIFは、beatoraja `PixmapResourcePool`の
`toSpriteSheetPixmap`と同じスプライトシート1枚へ展開します。

- 各フレームはcanvasと同じ大きさで、左上から行優先に並べます。余ったセルは透明です。
- 列数は、フレーム数を割り切る配置のうち、シートの幅と高さの差が最小のものです。
  同点なら列数の少ない方を選びます。割り切れる配置が上限に収まらない場合は、
  beatorajaの`getSpriteSheetGrid`と同じ代替計算を使います。
- フレームの表示時間は使いません。スキンは通常の`divx` / `divy` / `cycle` / `timer`で
  アニメーションさせます。
- 1フレームの画像と、既定画像以外に1フレームしか持たないAPNGは従来どおり静止画です。
  16bit APNGはアニメーションを展開できないため、既定画像を使います。

beatorajaは格子をGPUの`GL_MAX_TEXTURE_SIZE`で決めます。BMZも、シートを受け取るwgpuデバイスの
`max_texture_dimension_2d`で格子を決めます。decode時はフレームだけを保持し、シートはGPUへ
uploadする直前に組み立てます。起動直後などGPUの生成前にinstallしたスキンは、デバイスが
接続された時点でシートを組み立て、スキンのsourceサイズを更新します。
どの配置でもデバイスの上限に収まらないシートと、展開後256MiBを超えるシートは、警告して
そのsourceを読み込みません。展開の上限（ファイル64MiB、canvas 8192px、4096フレーム、
展開後256MiB、decode 10秒）を超えた場合も読み込みエラーです。

譜面側の`#STAGEFILE` / `#BANNER` / `#BACKBMP`とBGAのアニメーションは、スキン画像とは別に
フレーム時間どおりに再生します。PlayのアニメーションSTAGEFILEはruntime source `100`を専用texture
へ差し替えるため、スキンの要素追加は不要です。規則は[譜面の画像](chart-images.md)を参照してください。

APNG / GIFのシート化はBMZ独自の挙動です。beatoraja / LR2は先頭フレーム
（APNGは既定画像）だけを表示するため、それらを`divx` / `divy`なしで使う既存スキンでは
BMZだけシート全体が表示されます。読み込んだアニメーションはフレーム数と格子を
info logに出力します。

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
直接指定、保存済み選択、wildcardの親ディレクトリと後続の階層では、完全一致を優先して
ASCIIの大小文字差を補正します。探索は許可rootの内側で行い、途中のリンクの行き先も検査します。

現状はPMcharaの`--`座標補間と、画像右下色を透過色にするchroma-key処理には未対応です。
PNG等が持つalphaは通常どおり反映します。

## Text opacity

BMZでは、解決済みdestinationの`a`を文字本体・縁取り・影に共通して適用します。
`outlineColor` / `shadowColor`自身のαには、この共通透明度を乗算します。
`a: 0`ではすべて透明になり、アニメーションやoffset、後段の画面フェードにも追従します。
これはBMZの統一方針であり、beatorajaの距離フィールド描画と厳密に同一の挙動ではありません。

`shadowOffsetX` / `shadowOffsetY`を指定した文字は、`shadowColor`が未指定や透明でも、文字色の半分の
明るさで影を描きます。beatorajaの`SkinTextFont`と通常の`SkinTextBitmap`は`shadowColor`を読まないためです。
読むのは距離フィールドのフォントだけで、BMZでも`font.type`が1 / 2の文字にはこの影を描きません。
不透明な`shadowColor`はBMZでは指定色のまま使います。

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

## Text Gradient

BMZ拡張の`colorTop` / `colorBottom` / `gradientAngle`で、文字本体に線形グラデーションを掛けられます。
JSON skinとLua skinで同じフィールドを使用できます。

| field | value |
| --- | --- |
| `colorTop` | 始点の色（`RRGGBB` / `RRGGBBAA`）。省略時は白 |
| `colorBottom` | 終点の色（`RRGGBB` / `RRGGBBAA`）。省略時は白 |
| `gradientAngle` | 向き（度）。省略時は`0` |

- `colorTop`と`colorBottom`のどちらかを指定すると有効になります。どちらも省略した場合や、読めない色だけの場合は、従来どおり1色で描きます。
- 色はdestinationの`r` / `g` / `b` / `a`に乗算します。destinationのアニメーションやフェードにも追従します。
- `gradientAngle: 0`で上から下へ変化します。正の値はdestinationの`angle`と同じく反時計回りで、`90`で左から右、`180`で下から上になります。角度は画面の画素で測ります。
- 横の範囲は、実際に描画される文字（グリフ）の左端から右端までです。空白は範囲に含めないので、先頭や末尾に空白を足しても色の付き方は変わりません。単語の間の空白は前後の文字に挟まれるため範囲の内側になり、文字列全体で1つのグラデーションになります。`overflow`で縮小・切り詰めた場合も、描画された文字の範囲を使います。
- 縦の範囲は、文字の上端からdestinationの高さまでです。`wrapping: true`では、行ごとに繰り返さず、最後の行の下端までを含む1つのグラデーションになります。範囲の外側は端の色になります。
- 縁取り・影は範囲の計算に含めません。空白だけの文字列など、描画する文字が無い場合は何も描きません。
- グラデーションは文字本体だけに掛かります。`outlineColor`の縁取り、`shadowColor`の影は指定色のままです。カラー絵文字は絵文字自身の色を保ちます。
- TTF/OTFと画像フォント（`.fnt`）の両方で使えます。画像フォントでは、グリフ画像の色にグラデーションを乗算します。

```json
{
  "text": [
    { "id": "title", "size": 30, "colorTop": "ffe680", "colorBottom": "ff6600" },
    { "id": "artist", "size": 20, "colorTop": "66ddff", "colorBottom": "cc66ff", "gradientAngle": 90 }
  ]
}
```

## フォントのfallbackと欠字表示

スキンのtextで、指定フォントに無い文字を描く方法を示します。
beatorajaの`font.fallback`と欠字の代替候補は互換仕様、OSフォントでの補完とカラー絵文字はBMZ拡張です。
BMZ メニュー等のegui描画は対象外です。実装の経緯は[作業記録](../notes/2026/2026-10-20-unicode-emoji-text.md)を参照してください。

### 探索順

ベクターフォント（TTF / OTF / TTC）を指定したtextは、次の順に文字を探します。

1. スキンの主フォント
2. `font.fallback`のベクターフォント（記述順）
3. BMZの既定フォント（同梱のNoto Sans CJK等。言語設定の地域を優先）
4. OSの記号・絵文字フォント
5. 欠字表示

ビットマップフォント（`.fnt` / `.lr2font`）を指定したtextは、次の順です。

1. 主フォントと`font.fallback`の`.fnt`（記述順）
2. 〜（U+301C）と～（U+FF5E）の相互代用（beatoraja互換）
3. `font.fallback`のベクターフォント、既定フォント、OSの記号・絵文字フォント（BMZ拡張）
4. 欠字表示

`font`を指定しないtextとLR2のsystem fontは、既定フォントから探します。
別フォントの文字も、主フォントの基準線とemの大きさに揃えます。
ビットマップフォントでは`info size`（LR2は`#S`）を1emとして扱います。

### ビットマップフォント文中のベクター文字の見た目

ビットマップフォントのtextで、探索順3のベクターフォントから描く文字は、
主フォントのページ画像から推定した塗りと縁取りで描きます（BMZ拡張）。
`.fnt`は色の情報を持たず、Hiero等で付けたグラデーション・縁取りはページ画像にだけあるためです。

- フォントの読み込み時に、代表的なグリフ（英大文字・数字・かな・漢字の一部）の不透明な画素を調べます。
- 輪郭に接する画素の色を縁の色とし、内側へ色が変わる距離を縁取りの幅にします。
- 縁取りより内側の色を、行上端（`yoffset`の基準）からの高さごとに平均し、塗り色の表にします。
  Hieroのグラデーションは行の位置に固定されるため、文字の高さが違っても同じ位置は同じ色になります。
- 推定した範囲の外（行の上下の端）は、端の行の色で塗ります。
- ベクターの文字を塗り色の表で塗り、外側を縁取りの幅だけ縁取り色で描きます。
  ビットマップのグリフと同じく、スキンの文字色・影・縁取りの色を掛けます。
- 次のフォントは推定せず、従来どおりスキンの文字色の一色で描きます。
  - 縁取りが無く、塗りがほぼ白のフォント（スキン側で色を付ける前提）
  - 推定に使える画素が足りないフォント（4×4px未満のグリフだけ等）
  - 文字ごとに色が違うフォント（数字だけ赤い等）。同じ行の高さでも塗りの色が大きく違う場合です。
  - 推定対象の矩形がページ画像の外にある、行座標が範囲外になる、総画素数が1,048,576を超える、
    または行座標の幅が16,384pxを超えるフォント。フォント本体は維持し、推定だけを見送ります。
- カラー絵文字はフォント自身の色で描きます。どのフォントにも無い文字の生成した枠は対象外です。
- 文字の太さ・書体（`info face`）は合わせません。影・テクスチャは推定しません。
  ぼかした縁取り（グロー）は、中間調が塗りに混ざり、塗りが少し暗くなります。

### `font.fallback`

```json
{
  "font": [
    {
      "id": "main",
      "path": "font/main.ttf",
      "fallback": [{ "path": "font/symbols.ttf", "type": 0 }, "font/emoji.ttf"]
    }
  ]
}
```

- 各要素は文字列か`{ "path", "type" }`です。beatorajaと同じく`{ "value" }`と単一の値も受け付けます。
- パスは`font.path`と同じ規則（スキン基準、ワイルドカード、大文字小文字の違い）で解決します。
- Luaスキンも同じtableを書けます（`fallback = { "font/a.ttf", { path = "font/b.ttf", type = 0 } }`）。
- 読み込めないfallbackは警告を出して使いません。
- `type`は読み込みますが、描画方式には使いません（主フォントの`type`と同じ扱い）。
- ベクターの主フォントは`.fnt`のfallbackを使いません（beatoraja互換）。

### 欠字表示

どのフォントにも無い文字は、beatorajaの`MISSING_GLYPH_CANDIDATES`と同じ候補を順に探して代わりに描きます。

□（U+25A1）→ ▢（U+25A2）→ ☐（U+2610）→ ■（U+25A0）→ ?

候補も無い場合は、文字色の枠を生成して描きます。
欠字も通常の文字と同じく幅を持ち、Truncate・縮小・折り返し・揃え・影・縁取りの対象になります。

フォントが文字を持っていても、そのグリフに輪郭もカラー画像も無ければ、描けないものとして次のフォントを探します。
COLRフォントの空の基底グリフや、画像フォントの空のglyfが該当します。
描画時にグリフを画像にできなかった場合も、もう一方の形式（輪郭 / カラー）、最後に生成した枠で描きます。

- 空白・制御文字・Default_Ignorable（ZWJ、異体字セレクタ、ZWSP等）は欠字表示しません。
- 空白と点字の空白（U+2800）は画素を持たず、送り幅だけを持ちます。
- U+FFFD（置換文字）は通常の文字として探します。
- ビットマップフォントに無い空白は幅0にします。

### Unicodeの扱い

- 拡張書記素クラスタを1文字として扱い、Truncate・折り返し・caretはクラスタを分割しません。
- 複数コードポイントのクラスタ（ZWJ絵文字、国旗、肌色修飾、キーキャップ、結合文字）は、
  全文字を持つ1つのフォントでシェーピングします（rustybuzz、GSUB / GPOS / AAT morx）。
- 構成文字のグリフがあっても、シーケンスとして描けないフォントは使いません。条件は次のとおりです。

| クラスタ | 描けると判断する条件 |
| --- | --- |
| ZWJシーケンス、国旗、肌色修飾、タグシーケンス（🏴＋タグ文字の地域旗） | 構成文字が1つの合字へ置換された |
| キーキャップ（数字＋U+20E3） | 合字、または送り幅0の囲み記号を数字に重ねた |
| 結合文字など | 各文字のグリフを並べた結果でよい |

- タグシーケンスは見える文字が🏴の1つでも、クラスタ全体でシェーピングします。
- 描けるフォントが無い絵文字シーケンスは、部品に分解せず、クラスタ全体で1つの欠字表示にします。
  タグシーケンスも黒い旗だけを描くことはしません。結合文字の列は基底文字だけを描きます。
- 異体字セレクタは、cmap format 14の異体字グリフを優先します。
- シェーピングはクラスタ内だけです。クラスタをまたぐ合字・カーニング、
  アラビア文字等の連結、双方向テキストの並べ替えは行いません。

### カラー絵文字

文字の表示形式で、探す順を変えます。

| 文字 | 探す順 |
| --- | --- |
| VS16（U+FE0F）付き、絵文字シーケンス、Emoji_Presentation=Yesの文字（😀等） | カラー → 輪郭 → 欠字表示 |
| それ以外（✡ ❤ ♠等の記号、VS15（U+FE0E）付き） | 輪郭 → カラー → 欠字表示 |

Solomon難易度表の✡はテキスト表示のため、記号フォントの輪郭で描きます。

| 形式 | 主なフォント | 対応 |
| --- | --- | --- |
| COLR v0 / CPAL | Segoe UI Emoji（Windows） | 対応 |
| COLR v1 | Segoe UI Emoji（Windows 11）、Noto Color Emoji（COLRv1版） | 対応。可変フォントは既定の軸値で描きます |
| CBDT / CBLC | Noto Color Emoji（Linux） | 対応（PNG / BGRA） |
| sbix | Apple Color Emoji（macOS） | 対応（PNGのみ） |
| OpenType SVG | 一部の絵文字フォント | 未対応。輪郭か欠字表示で描きます |
| EBDT（単色ビットマップ） | — | 未対応 |

- カラーグリフは文字色を掛けず、destinationの透明度だけを掛けます。
- 影・縁取りでは、カラーグリフの形を影色・縁取り色で塗ります。
- CPALはパレット0を使い、前景色の指定（0xFFFF）は白で描きます。

### OSの記号・絵文字フォント

`bmz-font`の`SYMBOL_FONT_FAMILIES`の順に、ファミリ名が実際に一致したフォントだけを使います。
どれもインストールを前提にせず、文字ごとにグリフの有無を確認します。

| OS | 主な候補 |
| --- | --- |
| Windows | Segoe UI Symbol、Segoe UI Emoji |
| macOS | Apple Symbols、Zapf Dingbats、Apple Color Emoji |
| Linux | Noto Sans Symbols 2、Noto Sans Symbols、DejaVu Sans、FreeSerif、Symbola、Noto Color Emoji |

フォントファイルは読み取り専用でmemory mapし、プロセス内で一度だけ解決します。
OSに追加したフォントは、次回の起動から使います。

### キャッシュ

- 文字列の解決結果（フォント・グリフ・幅）は、フォントセットと文字列ごとに保持します。
  解像度やフォントサイズが変わっても再利用します。
- グリフ画像は文字atlasに一度だけ置き、新しいグリフの領域だけをGPUへ転送します。
- 文字atlasは幅1024px、高さ8192pxまでです。GPUの`max_texture_dimension_2d`がこれより小さい場合は、その値までにします。
- 1辺がatlasに収まらない大きなグリフは、表示上の大きさを保ったまま、画像の解像度だけを下げて置きます。
- atlasが高さの上限まで埋まっても、登録済みのグリフはそのまま使い続けます。
- 新しいグリフが入らないときは、そのグリフをそのフレームでは描かず、次のフレームでatlasを作り直します。
  欠けた文字列のレイアウトはキャッシュせず、作り直した後に組み直します。
  作り直したときは、GPUテクスチャを部分転送ではなく全体で書き直します。
- 作り直した直後のフレームでも収まらない場合は、1フレームの文字がatlasに入りきりません。
  毎フレーム作り直さないよう、60フレームごとに作り直します。待つ間も、格納済みのグリフは描きます。
- グリフごとの輪郭・カラー画像の有無はフォントごとに一度だけ調べて保持します。
- スキンフォントの登録、`font.fallback`の変更、既定フォントの地域変更では、atlasと解決結果を作り直します。

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
Lua skinの`text`で`value = function() return main_state.number(id) end`のように
数値refをそのまま返す関数は、text refではなく`numberRef`へ推論します。
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
`number(1970)` / `event_index(1970)` / `image.ref=1970` / `imageset.ref=1970`は
次の同じインデックスを返します。BMZデフォルトスキンのplay mode panelもこのrefを使用します。
選曲画面ではこれから開始するモードを返します。
決定画面 / プレイ画面 / リザルト画面では試行開始時に固定したモードを返します。

| 値 | SESSION MODE |
| ---: | --- |
| 0 | NORMAL |
| 1 | PRACTICE |
| 2 | AUTOPLAY |
| 3 | AUTO BATTLE（画面上の表記はAUTOPLAY BATTLE） |
| 4 | G-BATTLE |

縦に5分割したスプライトシートは、`image`に`divy=5, len=5, ref=1970`を指定すると
現在のモードに対応する行を表示します。`imageset`では`ref=1970`と、上表の順序の5つの
画像IDを`images`へ指定します。JSON / Luaで同じ値を使えます。

選曲画面の`act=-10006`は、引数が正なら次、負なら前のSESSION MODEへ切り替えます。
引数0では変更しません。NORMAL → PRACTICE → AUTOPLAY → AUTO BATTLE → G-BATTLE → NORMAL
の順に循環し、逆方向も循環します。画像を切り替える`ref`と操作する`act`は別に指定します。
このイベントはモードだけを変更し、曲の決定や再生開始は行いません。
キーボードと同じ処理で選曲状態とプロファイルの`session_mode` / `auto_play`を更新します。
モードの変更は次の描画に反映します。
設定の保存タイミングと、モード設定後に再生を開始する既存の`act=15/16/315`は変わりません。

`image` / `imageset`の`click`は既存のbeatoraja互換ルールに従います。

| click | 次へ（引数+1） | 前へ（引数-1） |
| ---: | --- | --- |
| 0 | 左クリック | 右クリック |
| 1 | 右クリック | 左クリック |
| 2 | 画像の右半分 | 画像の左半分 |
| 3 | 画像の上半分 | 画像の下半分 |
| 4 | クリック無効 | クリック無効 |

`click=2/3`では、左右どちらのマウスボタンでもクリックした領域で方向を決めます。
`click=3`はbeatorajaおよび既存BMZと同じく、上半分が正方向です。

Luaの`image`定義例です。描画用の`source` / `destination`は通常どおり定義します。

```lua
{
    id = "session_mode", src = "src_session_mode",
    x = 0, y = 0, w = 210, h = 250,
    divy = 5, len = 5,
    ref = 1970, act = -10006, click = 0
}
```

この例は左クリックで次、右クリックで前へ進みます。`click=2`に変えると画像の左半分で前、
右半分で次へ進み、どちらの方法でも現在のモードの画像を表示します。

`act=1970`はBMZ組み込みイベントとして採用しません。beatorajaのLua `customEvents`が使う
`1000..1999`と衝突するためです。`ref=1970`は状態参照用であり、同じ番号のイベントを
BMZが横取りすることはありません。Luaカスタムイベントとfunctionの`act`の実行順も維持します。

### BMZの負数イベント

以下の公開イベントはJSON / Lua skinの`act`へ直接指定できます。

| ID | 区分 | 用途 |
| ---: | --- | --- |
| -10003 | 公開 | 対応する選曲 / リザルトスキンのIR scopeをRANKINGへ変更 |
| -10004 | 公開 | 同IR scopeをRIVALへ変更 |
| -10005 | 公開 | 同IR scopeを切り替え |
| -10006 | 公開 | 選曲画面のSESSION MODEを引数の符号で前後へ切り替え |
| -10100 | 公開 | 表示上の日次集計をリセット（score historyは削除しない） |
| -10001 / -10002 | 内部専用 | Lua Result skinの`Expand_op`操作をIR / graph panel切り替えへ変換 |
| -20000から減少 | 内部専用 | Lua変換時に生成するruntime flag操作 |
| -1000000..-1999999 | 内部専用 | Luaのfunction / string `act`を永続VMのcallbackへ接続 |

内部専用IDはBMZが生成するため、skinへ直接ハードコードしないでください。
公開イベントの`-10006`はLua callback用IDと重ならず、既存の負数イベントも維持します。

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
| 344 | `NONE → RIVALCHART → RIVALCHART MIRROR → RIVALOPTION`で譜面再現モードを切替 |
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
`RIVALCHART MIRROR` / `RIVALOPTION`として取得できます。`RIVALOPTION`は配置種別だけ、
`RIVALCHART`は配置種別とライバルのside別24bit seedを適用します。
BMZ拡張の`RIVALCHART MIRROR`は、その譜面を左右反転した配置種別とseedを使います。ghostは使用しません。

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
| `bmz_select_chart_replication` | chart replication (`NONE` / `RIVALCHART` / `RIVALCHART MIRROR` / `RIVALOPTION`) |
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
他のパネルが開いていない場合、退場中のクリック・slider・ホイールとキー入力は、保持表示が無い状態と同じく選曲画面の操作として扱います。
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

enumのVALUEは、LANE COVER: `0=OFF,1=SUDDEN+,2=HIDDEN+,3=SUD+ & HID+,4=LIFT,5=LIFT & SUD+,6=LIFT & HID+,7=ALL`（1=SUDDEN+、2=HIDDEN+、4=LIFTのビット和）、
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

`"bmzDetailRaise": [column, dy]`は、そのdestinationの描画列`column`（中央が3、枠外の左は-1、右は7）が
中央にある間だけ`dy`（skin canvasのpixel、左下原点なので正で上）持ち上げます。
見かけの列位置`column + scroll`（scrollは上記の小数列分）と中央との距離をdとして、変位は`dy * max(0, 1 - d)`です。
中央へ入る列は徐々に上がり、出ていく列は徐々に下がります。描画とクリック判定の両方へ適用し、
退場表示中は解放時のscrollで固定します。未指定・`dy = 0`は無効で、E2非表示時も変位しません。

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

### BMZ Select Key Filter Index

選曲画面のキーフィルターを画像で出し分けるための拡張参照です。beatoraja互換の`ref=11`は
4K / 6K / 8KをALL KEYS（`0`）として返すため、これらのフィルター用の画像を選べません。
既存スキンの表示を変えないよう`ref=11`はそのままにし、BMZ拡張の`ref=19250`を追加しています。

| ref / name | kind | meaning |
| --- | --- | --- |
| 11 / `mode` | image・imageset ref / number | beatoraja互換のキーフィルターindex。4K / 6K / 8Kは`0` |
| 19250 / `bmz_select_mode_index` | image・imageset ref / number / event_index / IntegerProperty | BMZ拡張のキーフィルターindex。下表の`0..12` |

どちらも**現在選択しているキーフィルター**を返します。曲行の譜面のkey modeは
[`1903`](#bmz-key-mode-refs)で参照し、キーフィルターとは連動しません。

| index | filter | `ref=11` | 区分 |
| ---: | --- | ---: | --- |
| 0 | ALL KEYS | 0 | beatoraja |
| 1 | 5KEYS | 1 | beatoraja |
| 2 | 7KEYS | 2 | beatoraja |
| 3 | 10KEYS | 3 | beatoraja |
| 4 | 14KEYS | 4 | beatoraja |
| 5 | 9KEYS | 5 | beatoraja |
| 6 | 24KEYS | 6 | beatoraja |
| 7 | 48KEYS | 7 | beatoraja |
| 8 | SINGLE (5K+7K) | — | beatoraja予約 |
| 9 | DOUBLE (10K+14K) | — | beatoraja予約 |
| 10 | 4KEYS | 0 | BMZ |
| 11 | 6KEYS | 0 | BMZ |
| 12 | 8KEYS | 0 | BMZ |

- `0..9`はbeatorajaの`ModeFilter.getSkinNumber()`と同じ値です。8 / 9はBMZにSINGLE / DOUBLE
  フィルターが無い現在も予約とし、別の用途に使いません。
- 番号は公開仕様として固定します。モードを追加する場合は新しい番号を割り当て、既存の番号は変更しません。
- 新しく作るimagesetでは`images`を13個（index `0..12`）用意します。使わない枠には
  ALL KEYS等の画像IDを重複して指定できます。
- 画像が足りない場合は既存の範囲外処理に従います。`imageset.ref`は最後の画像、
  `imageset.value`は先頭の画像、`image.ref`は`divy`（`len`指定時はその値）の最後の行を使います。
- Select以外ではimage / imagesetのindexとnumberは未取得（numberは非表示、Luaの
  `main_state.number`は`0`）、event_indexは`-1`です。

既存の`ref=11`を使うスキンは修正不要で、4K / 6K / 8KではこれまでどおりALL KEYSの画像を表示します。
4K / 6K / 8K用の画像を出す場合は、imagesetを`ref=19250`へ変更し、13個の画像を指定します。

```json
{
  "imageset": [
    {
      "id": "key_mode_filter",
      "ref": 19250,
      "images": [
        "all", "5k", "7k", "10k", "14k", "9k", "24k", "48k",
        "single", "double", "4k", "6k", "8k"
      ]
    }
  ]
}
```

```lua
skin.imageset = {
    {
        id = "key_mode_filter",
        ref = 19250,
        images = {
            "all", "5k", "7k", "10k", "14k", "9k", "24k", "48k",
            "single", "double", "4k", "6k", "8k"
        }
    }
}
-- main_state.number(19250) / main_state.event_index(19250) でも同じindexを取得できる。
```

画像ID（`all`、`4k`等）は例です。対応する`image`の定義は別途必要です。
IntegerPropertyの経路を使う場合は`"value": "bmz_select_mode_index"`を指定します。
destinationの条件式では`draw: "number(19250)==10"`のように参照できます。
キーフィルターの切り替え操作（button / event 11）は変更しません。

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

プレイ中、次の`BPM × SCROLL`の変化をBPM換算値として予告表示するBMZ拡張です。
STOP中の値は0とし、SPEEDは含めません。本体は変化の事実だけを公開し、
何ms前から予告するかはスキンが宣言します。表示のON/OFFや位置もスキン側のオプションで決めます。

```json
{ "bmzBpmChangePreview": { "leadMs": 2000 } }
```

Lua skinはロード結果のtableに同じキーを設定します。`leadMs`は省略時2000、0〜10000にclampします。
宣言がないskinでは予告option `19064`とtimer `19068`が常にOFFになります。その他のrefは宣言に関係なく返します。

| ID | kind | meaning |
| ---: | --- | --- |
| 19060 | number | 次の実効BPM変化までの残りms |
| 19061 | number | 変化後の実効BPM（四捨五入、0・負値を含む） |
| 19062 | number | 変化直前の実効BPM（四捨五入、0・負値を含む） |
| 19063 | number | 変化率（変化後 ÷ 変化前 ×100、四捨五入） |
| 19064 | option | 残り時間が`leadMs`以下（予告表示中） |
| 19065 | option | 次の変化で実効BPMの数値が増える |
| 19066 | option | 次の変化で実効BPMの数値が減る |
| 19067 | option | 以降に実効BPM変化がある（距離は問わない） |
| 19068 | timer | 予告timerです。変化時刻の`leadMs`前に0msで始まり、変化時刻に達するとOFF |
| 19069 | timer | 直前のBPM変化からの経過msです。最初の変化まではOFF |

- BPMとSCROLLの変更、およびSTOPの開始・終了をまとめて判定します。STOP開始時は0へ、
  終了時はその位置の`BPM × SCROLL`へ変化します。連続するSTOPの途中では復帰を予告しません。
- 同じ時刻の更新をすべて適用した前後の値を比較し、四捨五入前の差の絶対値が1未満なら予告しません。
  BPMとSCROLLが同時に変わっても積が変わらなければ予告しません。非有限値は公開しません。
- 負のSCROLLを絶対値には変換しません。加速・減速のoptionは符号付き数値の大小を使います。
  例えば`-150 → 0`は増加、`0 → -150`は減少です。符号なしの数字画像を使うskinは負号を別途描画してください。
- 変化率19063は変化前が0の場合は未定義（number画像は非表示、Luaでは0）です。負値からの比率も符号付きで計算します。
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
  前後は判定の処理順で決めます。multi-BADのように一つの入力から同時刻のBAD→GOODが出た場合は消し、
  同時刻のGOOD→BADでは表示します。
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

LR2プレイスキンのdecode時に、改造LR2のFAST/SLOW拡張refを次のBMZ状態へ変換します。
JSON / Lua skinの同名refは変換せず、beatorajaの意味を維持します。

| LR2 ref | decode後 | meaning |
| ---: | ---: | --- |
| 210 | 19170 | 最新の1P判定です。`0=非表示`, `1=FAST`, `2=SLOW`です。PGREATは常に`0` |
| 212 | 423 | FAST合計です。GREAT / GOOD / BAD / POOR / EMPTY POORを集計 |
| 214 | 424 | SLOW合計です。GREAT / GOOD / BAD / POOR / EMPTY POORを集計 |

`19170`はLR2変換専用の内部refです。`212/214`は既存のBMZ `423/424`へaliasするため、
改造LR2の原仕様と異なりPOOR / EMPTY POORも合計へ含みます。

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
m-selectのID特例は、今日の成果を`constantText`で埋め込む旧版向けです。現行版はLua callbackで
表示するため、BMZ対応版のmz-selectはこれらのrefを直接参照します。

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

## G-BATTLE一覧の相手情報

BMZ拡張の`20200..20229`は選曲中のG-BATTLE一覧で選択している相手専用です。
自分のオプションや配置（`42` / `43` / `450..469`）とは独立しています。

| ID | 種類 | 内容 |
| --- | --- | --- |
| 20200 | option | 相手一覧が開いている。BATTLE OFF行でもtrue。SESSION MODEの1970とは独立 |
| 20201 | option | 1Pオプションを取得済み |
| 20202 | option | 固定配置を取得済み |
| 20203 | option | ローカルリプレイの情報を読み込み中 |
| 20204 | text | 選択中の相手名 / MYBEST / REPLAY名 |
| 20205 / 20206 | text | 相手の1P / 2P配置オプション。未取得は空文字 |
| 20207 | text | 相手のDPオプション。未取得は空文字 |
| 20208 | number | 元譜面のキー数（4 / 5 / 6 / 7 / 8 / 9 / 10 / 14） |
| 20210..20229 | number | 表示先鍵盤に来る元鍵盤。450..469と同じスロット順、0は未取得 |

一覧を閉じた後とSelect以外ではoptionはfalse、textは空、配置numberは0です。
配置の1Pは20210..20218、2Pは20220..20228、各皿は20219 / 20229です。
DP FLIPではDPラベルにFLIPを示し、数字は移動元サイド内の鍵盤番号で表します。

IRは取得済みのランキング / ライバルスコアのメタデータを参照し、表示目的で
リプレイをダウンロードしません。MYBEST / REPLAYはカーソル停止120ms後に
ローカルファイルを別処理で読み、古い譜面・カーソルへの結果は反映しません。
情報欠損をNORMALや新しい乱数seedで補完しません。固定配置のある
NORMAL / MIRROR / RANDOM / R-RANDOM / RANDOM-EX / F-RANDOM / MF-RANDOMは
実際の配置処理と同じ乱数規則を使い、S-RANDOM等のノートごとの変換は固定配置を出しません。

mz-selectは左下の独立パネルで配置オプション、鍵盤順、既存の
CHART REPLICATION設定（text 86）の値を表示します。
既存パネル素材の約50%黒地と左の白線を流用し、タイトルは`g-battle data`です。
英字と鍵盤番号はNETWORK / score dataと同じm_select_system.fntを使います。
相手名とRIVAL OPTION / LANE ORDER / REPLICATIONのラベルは表示しません。
配置オプションは既存オプション画像、鍵盤はresult/keys1p.pngを使用し、
元の鍵盤番号が偶数（8を含む）なら黒鍵、奇数と皿は白鍵の枠にします。
HARD等のゲージ名は表示しません。未取得はNO DATA、読み込み中はLOADING、
S-RANDOM等の可変配置はNOT FIXEDと区別し、NO PLAYと同じ灰色にします。
DPは1P/2Pの行を明示し、SP/DPの内容に合わせてパネルの高さを変えます。
20200がtrueの間はランキング / rival data / ir dataの本文を隠し、
閉じたら元の条件で復帰します。上部のライバル選択欄は維持します。

Luxez-FlatはSPでは既存のRIVAL DATA欄（413×150px）、10K/14KのDPでは
左下のIR DATA / SCORE DETAIL欄（412×284px）へ切り替えます。
DPの1P/2Pは配置オプションと鍵盤を上下に並べ、DP OPTIONとtext 86の値を下段に表示します。
右側のRIVAL DATA / SCORE DATAと上部のライバル選択欄は維持します。
通常パネルの配置オプション画像、既存の角丸枠・発光・透過・フォントと、
result/png/random_pattern.pngの鍵盤を流用します。鍵盤はResultと同じ25×35px、27px間隔です。
画像のない8/9/Sは、元の鍵盤番号が奇数なら明るい1番、偶数なら暗い2番の鍵盤の枠と無地の面に、
同梱のNoto Sans CJK Mediumで数字と同じRGB(255,168,0)・1pxの黒縁の文字を重ねます。
ゲージ名・相手名・重複する項目ラベルは追加せず、NO DATA / LOADING / NOT FIXEDは
既存NO PLAYのRGB(127,128,128)で表示します。
`default_battleinfo/parts.lua`の正常ロード時だけ元のパネルを条件付きで隠し、
一覧を閉じると復帰します。元のLua callbackも維持し、無効・欠落・ロード失敗時とBMZ以外は
従来の表示を残します。[回帰テスト](../crates/bmz-player/src/skin_loader/tests/luxe_battle_info.rs)と
[実装記録](../notes/2026/2026-11-01-luxez-flat-gbattle-info.md)を参照してください。

別途導入するADFX02のECFNは、グラフ下の空き（x=441、top=696、715×108px）に
`G-BATTLE DATA`窓を追加します。右端はTYPE-AのIR DATA窓に揃え、既存の窓はそのまま残します。
SCORE DATA窓の見出し帯・枠・縁の陰影を等倍で組み直し、滑らかな本文のグラデーションだけを縮めます。
見出しと値は窓に焼き込まれた文字と同じ源暎エムゴHeavy＋黒縁、列の区切りはPLAY DATAと同じ1px線です。
配置オプション、鍵盤順、text 86の値を3列に並べ、10K/14Kでは1P/2Pの行とDP OPTIONを分けます。
鍵盤は同梱StarseekerのResult鍵盤を31×29px・28px間隔で流用します。画像のない8/9/Sは
同じ鍵盤の無地部分に、ECFNの数字画像と源暎エムゴを重ねます。数字画像は17pxで縁込みの上端・下端を
鍵盤の数字に揃え、暗い縁がにじまないよう補間せずに描きます。
相手名・項目ラベル・ゲージ名は表示せず、NO DATA / LOADING / NOT FIXEDは
窓の数字の淡い0と同じ50%の不透明度で表示します。
`ECFN/select/bmz_battle_info.lua`は他のBMZ拡張と独立して読み込み、欠落・失敗時とBMZ以外は
従来の画面のままです。[明示実行テスト](../crates/bmz-player/src/skin_loader/tests/ecfn_battle_info.rs)と
[実装記録](../notes/2026/2026-11-01-ecfn-gbattle-info.md)を参照してください。
