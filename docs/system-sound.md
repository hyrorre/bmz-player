# サウンドセットとシステム音

サウンドセットは、Select / Decide / RESULT BGMと任意のシステムSEをまとめた音源セットです。
既存のBGMセットへファイルを追加して利用でき、フォルダーやprofileの移行は必要ありません。
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

`bgm_dir`配下の`select`音源があるディレクトリをサウンドセット、`se_dir`配下の`clear`音源が
あるディレクトリを補完用SEセットとして再帰走査します。候補の走査は起動時とprofile切替時に行い、
起動時とSelect復帰時にそれぞれ1セットを抽選します。Selectを離れた後は適用済みセットを保持し、
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

対象は`clear` / `fail` / `a` / `aa` / `aaa`です。通常版と`.loop`版が両方ある場合は
`.loop`版を先に試します。途中のループ区間指定はなく、音源の末尾から先頭へ戻ります。
既存Select BGMは引き続きループ、Decide BGMとSEは引き続き単発です。
SEに`.loop`を付けてもループSEとしては認識しません。

Result退出時は単発・ループのどちらも既存の音声フェードに合わせて停止します。
Select復帰・リトライ・コース遷移・profile切替でRESULT BGMを持ち越しません。
通常終了後の譜面音の余韻と、スキンが宣言する音声処理は既存の契約を維持します。

## SEの上書きと補完

RESULT入口音（`clear` / `fail` / `a` / `aa` / `aaa`）以外のシステムSEは次の順で探索します。

1. 選択中サウンドセット内の同名音源
2. 選択中の補完用SEセット内の同名音源
3. `default_sound_dir`内の同名音源

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
動画出力のPlay音声でもSEの上書きと補完を使います。

## ファイル形式と音量

音源拡張子は`.wav` → `.ogg` → `.flac` → `.mp3`の順で探索し、拡張子の大文字・小文字を区別しません。
各優先段階の候補を順にデコードし、欠落・デコード失敗時は次の候補へ進みます。
全候補が利用できなければ、その音種を無音で継続します。

Select / Decide / 新しいRESULT BGMはシステムBGM音量とシステムBGM正規化の対象です。
上書きしたSEとRESULT入口SEはシステムSE音量を使い、BGM正規化を適用しません。
いずれもマスター音量を乗算します。音量・正規化の実行中変更は既存の設定と同じ扱いです。

関連: [操作・設定](controls.md)、[スキン仕様](skin.md)、
[実装・検証記録](../notes/2026/2026-10-08-result-soundset.md)。
