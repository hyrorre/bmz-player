# サウンドセットとシステム音

サウンドセットは、Select / Decide / RESULT BGMと任意のシステムSEをまとめた音源セットです。
既存のBGMセットへファイルを追加して利用でき、profileの設定キーはそのまま使います。
Discussion [#29](https://github.com/hyrorre/bmz-player/discussions/29) の連動方式を扱います。

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

補完用SEセットは従来どおり、`se_dir`配下の`clear`音源があるディレクトリを再帰走査します。
候補の走査は起動時とprofile切替時に行い、起動時とSelect復帰時にサウンドセットと補完用SEセットを
それぞれ1セットずつ抽選します。Selectを離れた後は適用済みセットを保持し、
Decide / Play / Resultで別セットへ切り替えません。Selectを経由しないリトライも同じセットです。

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
同名の音源は選んだ子、親、音種ごとの従来の補完先の順で読み込みます。
他の子や、セットのルートより上の整理用フォルダからは補完しません。
子の`clear.ogg`は親の`clear.loop.ogg`より優先します。`.loop`と拡張子の優先順位は各フォルダ内で適用します。

バリエーションは子1階層までで、孫以降は候補にしません。ルート確定後の子を独立したセットとして
再登録することもありません。`readme.ogg`等の非対応名や、音源名と同名のディレクトリは音源として扱いません。

以前は親子それぞれに`select`がある場合に別セットとして検出していましたが、現在は親子で1セットです。
独立したセットとして配置する場合は、対応音源を持たない整理用フォルダの下で兄弟として並べてください。

## RESULT BGM

今回のプレイ結果に対応する、正常に読み込めた音源を1つ再生します。

| 今回の結果 | サウンドセット内の優先順位 | BGMがない場合のSE優先順位 |
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
補完用SEの`aaa`、補完用SEの`clear`の順です。各SEはSEセット、既定音源の順で補完します。
バリエーション利用時もランクを先に判定し、AAAなら子の`aaa`、親の`aaa`、子の`clear`、親の`clear`、
同ランクSE、`clear` SEの順です。親に`aaa`がある場合は子の汎用`clear`より優先します。

BGMが選ばれた場合、clear / fail SEは同時に鳴らしません。BGMの音量が0でもSEへ戻しません。
単発BGMの終了後にclear / fail SEを追加再生することもありません。
RESULT BGMは選択中のサウンドセット内だけを探索します。SEセットや既定音源内の
`clear` / `fail` / `a` / `aa` / `aaa`は単発SEとして使い、RESULT BGMがない場合に再生します。
COURSE RESULT専用BGMは追加せず、既存の`course_clear` / `course_fail`を使います。
コース曲間の通常Resultは通常RESULT BGMの規則を使います。

### 単発とループ

新しいRESULT BGMは通常名なら単発、音源拡張子の直前に`.loop`を付けるとResult退出までループします。

| ファイル名の例 | 再生 |
|---|---|
| `clear.ogg` | 1回のみ |
| `clear.loop.ogg` | ループ |
| `aaa.wav` | 1回のみ |
| `aaa.loop.wav` | ループ |

対象は`clear` / `fail` / `a` / `aa` / `aaa`です。同じフォルダに通常版と`.loop`版が両方ある場合は
`.loop`版を先に試します。途中のループ区間指定はなく、音源の末尾から先頭へ戻ります。
既存Select BGMは引き続きループ、Decide BGMとSEは引き続き単発です。
SEに`.loop`を付けてもループSEとしては認識しません。

Result退出時は単発・ループのどちらも既存の音声フェードに合わせて停止します。
Select復帰・リトライ・コース遷移・profile切替でRESULT BGMを持ち越しません。
通常終了後の譜面音の余韻と、スキンが宣言する音声処理は既存の契約を維持します。

## SEの上書きと補完

RESULT入口音（`clear` / `fail` / `a` / `aa` / `aaa`）以外のシステムSEは次の順で探索します。

1. 選択中の子フォルダ内の同名音源（子がある場合）
2. 選択中サウンドセットのルートにある同名音源（共通音源）
3. 選択中の補完用SEセット内の同名音源
4. `default_sound_dir`内の同名音源

例えばサウンドセット内の`scratch.ogg`を、従来SEセットのスクラッチ音より優先して再生します。
これらの音源は単発再生とシステムSE音量を維持します。
RESULT入口音もサウンドセットから置き換えられますが、RESULT BGMとしてシステムBGM音量を使います。
同じファイル名でも、補完用SEセットや既定音源に置いたRESULT入口音はSE音量で単発再生します。
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

Select / Decide / 新しいRESULT BGMはシステムBGM音量とシステムBGM正規化の対象です。
上書きしたSEとRESULT入口SEはシステムSE音量を使い、BGM正規化を適用しません。
いずれもマスター音量を乗算します。音量変更と、解析済みBGMの正規化ON/OFFは再生中にも反映します。
正規化OFFで読み込んだセットに対して後からONにした場合は、現在の音源を非同期に解析し、
完了後に再生中の音量へ反映します。Play / ResultでもSelectへの復帰は不要です。
この解析でセットを再抽選したり、BGMを停止・再開したりはしません。

関連: [操作・設定](controls.md)、[スキン仕様](skin.md)、
[実装・検証記録](../notes/2026/2026-10-08-result-soundset.md)。
