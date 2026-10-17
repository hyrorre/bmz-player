# サウンドセットとシステム音

サウンドセットは、選曲画面・決定画面・リザルト画面のBGMと、任意のシステムSEをまとめた音源セットです。
既存のBGMセットへファイルを追加して利用でき、profileの設定キーはそのまま使います。
Discussion [#29](https://github.com/hyrorre/bmz-player/discussions/29)の連動方式を扱います。

## 配置と設定

profile.tomlの既存設定を使います。設定UIでは`bgm_dir`を「サウンドセット ルート」、
`se_dir`を「補完用SE ルート」、`default_sound_dir`を「既定音源」と表示します。

```toml
[system_sound]
bgm_dir = "data/bgm"
se_dir = "data/se"
default_sound_dir = "data/defaultsound"
```

`bgm_dir`から再帰走査し、対応するBGM / SE音源が最初に見つかったディレクトリを
サウンドセットのルートとして確定します。`select`は必須ではなく、SEだけでも認識します。
音源がない階層は複数セットをまとめる整理用フォルダとして扱います。
設定した`bgm_dir`自体に対応音源がある場合は、そこが1セットのルートになります。
探索時は対応する固定名・拡張子のファイルがあるかで判断し、デコード可否は読み込み時に確認します。

補完用SEセットは、`se_dir`配下の`clear`または対応する`.enter`音源があるディレクトリを再帰走査します。
候補の走査は起動時とprofile切替時に行い、起動時とSelect復帰時にサウンドセットと補完用SEセットを
それぞれ1セットずつ抽選します。Selectを離れた後は適用済みセットを保持し、
決定画面・プレイ画面・リザルト画面では別セットへ切り替えません。選曲画面を経由しないリトライも同じセットを使います。

```text
data/bgm/BGM01/
  select.ogg
  decide.ogg
  clear.loop.ogg
  fail.ogg
  a.ogg
  aa.ogg
  aaa.loop.ogg
  scratch.ogg
  f-open.ogg
```

セット名は任意です。追加したSEだけを上書きでき、セット内で不足する音源を他のサウンドセットから
補完したり、RESULT専用のセットを別抽選したりはしません。

### 共通音源とバリエーション

セットのルートに共通音源を置き、その直下の任意名フォルダへ差分の音源を置けます。
`variants`等の専用フォルダ名や設定ファイルは不要です。

```text
data/bgm/MySoundSet/
  scratch.ogg             # 共通SE
  f-open.ogg
  decide.ogg              # 共通BGM
  style-a/
    select.ogg
    clear.loop.ogg
  style-b/
    select.ogg
    clear.ogg
```

セットを1つ抽選してから、対応音源を持つ直下の子フォルダを均等に1つ抽選します。
子の数はセット自体の当選確率に影響しません。候補の子がなければ親の音源だけを使います。
親子の組み合わせはSelectからResultまで保持し、Selectを経由しないリトライでも維持します。

子には変更する音源だけを置けます。`select`は必須ではなく、Result音源だけ、SEだけでも候補になります。
同名の音源は選んだ子、親の順で読み込み、どちらにもなければ音種ごとの補完先を使います。
他の子や、セットのルートより上の整理用フォルダからは補完しません。
子の`clear.ogg`は親の`clear.loop.ogg`より優先します。`.loop`と拡張子の優先順位は各フォルダ内で適用します。

バリエーションは子1階層までで、孫以降は候補にしません。ルート確定後の子を独立したセットとして
再登録することもありません。`readme.ogg`等の非対応名や、音源名と同名のディレクトリは音源として扱いません。

以前は親子それぞれに`select`がある場合に別セットとして検出していましたが、現在は親子で1セットです。
独立したセットとして配置する場合は、対応音源を持たない整理用フォルダの下で兄弟として並べてください。

## 通常RESULTの音源選択

今回のプレイ結果に対応する、正常に読み込めた音源を1つ再生します。

| 今回の結果 | サウンドセット内の優先順位 | 優先音源がない場合のSE優先順位 |
|---|---|---|
| FAILED | `fail` | `fail` |
| CLEAR・AAA | `aaa` → `clear` | `aaa` → `clear` |
| CLEAR・AA | `aa` → `clear` | `aa` → `clear` |
| CLEAR・A | `a` → `clear` | `a` → `clear` |
| その他のCLEAR | `clear` | `clear` |

FAILEDをランクより優先します。ランク判定はResult表示と同じ今回のEX SCORE / 全ノーツ数を使い、
自己ベストやIR順位は使いません。AAAは8/9以上、AAは7/9以上、Aは6/9以上です。
ランク音源はBGM・SEとも完全一致で選び、例えばAAAで`aaa`がなければ`clear`へ戻し、`aa`や`a`には落としません。
ノーツ数0ではランク音源を使わず、`clear`へ戻します。
ランク別SEより汎用の`clear` BGMを優先します。AAAならサウンドセットの`aaa`、同セットの`clear`、
同ランクの互換SE、`clear`互換SEの順です。各SEは子、親、SEセット、既定音源の順で補完します。
バリエーション利用時もランクを先に判定し、AAAなら子の`aaa`、親の`aaa`、子の`clear`、親の`clear`、
同ランクSE、`clear` SEの順です。親に`aaa`がある場合は子の汎用`clear`より優先します。
互換SEの`clear` / `fail` / `a` / `aa` / `aaa`も子、親、補完用SEセット、既定音源の順で
通常名を探索します。`.loop`は互換SEの候補に含めません。
同じ通常名ファイルがBGMとSEの両方へ登録されても、通常RESULTではBGM優先で1つだけ再生します。
この場合はデコードを1回にまとめ、同じ単発音源を共有します。`.loop`版は共有しません。

サウンドセットのRESULT音源が選ばれた場合、補完用のclear / fail SEは同時に鳴らしません。
選ばれた音源の音量が0でも別のSEへ戻しません。
単発RESULT音源の終了後にclear / fail SEを追加再生することもありません。
RESULT BGMは選択中のサウンドセット内だけを探索します。SEセットや既定音源内の
`clear` / `fail` / `a` / `aa` / `aaa`は単発SEとして使い、RESULT BGMがない場合に再生します。
COURSE RESULTは後述の専用BGMと入場SEを使います。
コース曲間の通常Resultは通常RESULT BGMの規則を使います。

### 単発とループ

RESULT音源は通常名なら単発SE、音源拡張子の直前に`.loop`を付けるとResult退出までループBGMとして再生します。

| ファイル名の例 | 再生 |
|---|---|
| `clear.ogg` | 1回のみ |
| `clear.loop.ogg` | ループ |
| `aaa.wav` | 1回のみ |
| `aaa.loop.wav` | ループ |

対象は`clear` / `fail` / `a` / `aa` / `aaa`です。同じフォルダに通常版と`.loop`版が両方ある場合は
`.loop`版を先に試します。途中のループ区間指定はなく、音源の末尾から先頭へ戻ります。
選曲BGMはループ、決定BGMとSEは単発で再生します。
対象外のSEに`.loop`を付けてもループSEとしては認識しません。

### 独立した入場SEとBGM

`<name>.enter.<ext>`は画面入場時に1回鳴るSE、`<name>.loop.<ext>`は画面表示中の
ループBGMです。SEの終了を待たずにBGMを開始し、それぞれシステムSE音量、システムBGM音量を使います。
BGMだけシステムBGM正規化の対象です。従来名の再生方式は変わりません。

| 画面 | 対応するname | 入場SE | 優先音源・BGM |
|---|---|---|---|
| Select | `select` | `select.enter` | `select.loop` → `select`（どちらもループ） |
| 通常RESULT | `clear` / `fail` / `a` / `aa` / `aaa` | `<name>.enter` | `<name>.loop` → `<name>`（通常名は単発） |
| COURSE RESULT | `course_clear` / `course_fail` | `<name>.enter` → 従来SE | `<name>.loop`のみ |

`.enter`は子、親、補完用SEセット、既定音源の順に探索します。
通常RESULT BGMとCOURSE RESULT BGMは子・親だけを探索し、補完用SEセットや既定音源の
`.loop`を流用しません。Select BGMは子、親、既定音源の順で、各層で`.loop`を通常名より優先します。
`.enter`や`.loop`だけのサウンドセット・子バリエーションも認識します。
拡張子・大小文字・デコード失敗時の次候補への補完は共通規則に従います。

通常RESULTでは、SEとBGMを独立してランク選択します。AAAは`aaa.enter` → `clear.enter`、
AAは`aa.enter` → `clear.enter`、Aは`a.enter` → `clear.enter`です。
BGMは同ランクの`.loop` → 通常名、続いて`clear.loop` → `clear`を使います。
各音種について子、親を探索してから汎用音種へ戻り、下位ランクへ段階的には戻りません。
FAILEDは`fail.enter`と`fail.loop` → `fail`を使います。
明示的な入場SEが読み込めた場合はループBGMと同時に鳴らし、旧形式の単発音源は重ねません。
入場SEがない場合は従来のBGM優先・SEフォールバックを維持します。

COURSE RESULTのSEは`course_clear.enter` / `course_fail.enter`を優先し、なければ
従来の`course_clear` / `course_fail`、さらに従来のRESULT `clear` / `fail` SEへ戻ります。
各音種は子、親、補完用SEセット、既定音源の順で、欠落・デコード失敗時は次候補を使います。
RESULT互換SEへの補完ではサウンドセットの通常名`clear` / `fail`を補完用SEセットより優先し、
単発・システムSE音量で再生します。`clear.loop` / `fail.loop`はこの補完に使いません。
SEは1つだけ鳴らし、専用`.loop` BGMがあれば同時に開始します。
通常RESULTの`clear.loop`等をCOURSE RESULTへ流用しません。

Select入場SEは初回起動、RESULT復帰、Decideキャンセル復帰など画面入場ごとに最大1回再生します。
再抽選したセットの非同期ロード中は待機し、旧セットの入場SEを鳴らしません。
同じセットの再利用でもロード完了時に1回だけ鳴らします。フォルダー移動やプレビュー切替では鳴らしません。
入場SEはプレビューの有無に依存せず、Viewer待機中は既存の音声抑制に従います。
Select退出時は入場SEも停止します。RESULT退出時は追加した入場SE・専用BGMも既存のフェード対象です。
`resultclose` / `course_close`の再生タイミングは変更しません。

Result退出時は単発・ループのどちらも既存の音声フェードに合わせて停止します。
Select復帰・リトライ・コース遷移・profile切替でRESULT BGMを持ち越しません。
通常終了後の譜面音の余韻と、スキンが宣言する音声処理は既存の契約を維持します。

## SEの上書きと補完

互換RESULT SE（`clear` / `fail` / `a` / `aa` / `aaa`）を含むシステムSEは次の順で探索します。

1. 選択中の子フォルダ内の同名音源（子がある場合）
2. 選択中サウンドセットのルートにある同名音源（共通音源）
3. 選択中の補完用SEセット内の同名音源
4. `default_sound_dir`内の同名音源

例えばサウンドセット内の`scratch.ogg`を、補完用SEセットのスクラッチ音より優先して再生します。
これらの音源は単発再生とシステムSE音量を維持します。
RESULT入口音の通常名は通常RESULTの従来音源として選択できますが、単発・システムSE音量です。
COURSE RESULTの互換SE補完として使う場合は、同じ通常名でも単発・システムSE音量です。
補完用SEセットや既定音源に置いた通常名のRESULT入口音もSE音量で単発再生します。
ランク別SEの`a` / `aa` / `aaa`にも`.loop`は適用しません。

| ファイル名（拡張子を除く） | 用途 |
|---|---|
| `scratch` | スクラッチ操作音 |
| `f-open` / `f-close` | フォルダーを開く / 閉じる音 |
| `o-change` | オプション変更音 |
| `o-open` / `o-close` | オプションパネルを開く / 閉じる音 |
| `playready` | プレイ開始前の音 |
| `playstop` | 途中FAILED時の閉店音 |
| `clear` / `fail` | 通常RESULTのクリア / 失敗SE |
| `a` / `aa` / `aaa` | 通常RESULTのランク別SE（今回追加） |
| `resultclose` | 通常RESULTを閉じる音 |
| `course_clear` / `course_fail` | COURSE RESULTのクリア / 失敗SE |
| `course_close` | COURSE RESULTを閉じる音 |
| `guide-pg` / `guide-gr` / `guide-gd` / `guide-bd` | PGREAT / GREAT / GOOD / BADのGUIDE SE |
| `guide-pr` / `guide-ms` | 空打ちPOOR / 見逃しMISSのGUIDE SE |
| `landmine` | 譜面に地雷音源がない場合のシステム地雷音 |

既存BGMの`select` / `decide`はサウンドセット、既定音源の順で探索し、SEセットからは補完しません。
動画出力のPlay音声でも子・親のSE上書きと補完を使います。
動画出力では候補順の先頭のセットと子を使い、実行ごとのランダム抽選は行いません。

## ファイル形式と音量

音源拡張子は`.wav` → `.ogg` → `.flac` → `.mp3`の順で探索します。
OSやファイルシステムに関係なく、固定の音源名・`.loop`・拡張子はASCIIの大文字・小文字を区別しません。
セットや子の検出にも同じ規則を使い、`CLEAR.LOOP.OGG`は`clear.loop.ogg`と同じ扱いです。
同じ拡張子で大小文字だけ異なるファイルが共存する場合は、ファイル名全体が正式な小文字表記の候補を優先し、
残りはファイル名順に探索します。`.loop`版優先と拡張子の優先順位は維持します。
各優先段階の候補を順にデコードし、欠落・デコード失敗時は次の候補へ進みます。
全候補が利用できなければ、その音種を無音で継続します。

音量区分は、デコードに成功した音源の再生方式で決定します。探索元のディレクトリや音種名には依存しません。

| 再生方式・ファイル名 | 音量 | システムBGM正規化 |
|---|---|---|
| ループする全システム音源（`select`、対応する`.loop`） | System BGM | 対象 |
| 単発の`decide` | System BGM | 対象 |
| その他の単発音源（通常名のRESULT、`.enter`、従来SE、退出SE） | System SE | 対象外 |

旧仕様ではサウンドセットの通常名`clear` / `fail` / `a` / `aa` / `aaa`もBGM音量・正規化を使いました。
現在は単発再生を維持し、音量区分をSEへ変更しています。通常RESULTの音源選択では引き続き
サウンドセットの従来音源を優先し、補完用SEを重ねません。`.enter`がある場合は旧形式の単発を重ねず、
ループBGMのみ独立して同時再生します。COURSE RESULTの単発SEと専用ループBGMの併用は維持します。
単発RESULTもRESULT退出・リトライ時の停止・フェード対象です。音量区分とシーンの停止対象は別に管理します。
譜面キー音・譜面BGM、プレビュー、スキン独自音声にはこの変更を適用しません。
いずれもマスター音量を乗算します。音量変更と、解析済みBGMの正規化ON/OFFは再生中にも反映します。
正規化OFFで読み込んだセットに対して後からONにした場合は、現在の音源を非同期に解析し、
完了後に再生中の音量へ反映します。プレイ画面・リザルト画面でも、選曲画面へ戻る必要はありません。
この解析でセットを再抽選したり、BGMを停止・再開したりはしません。

関連する文書は[設定画面](settings.md#サウンドセット)、[スキン仕様](skin.md)、
[実装・検証記録](../notes/2026/2026-10-08-result-soundset.md)です。
