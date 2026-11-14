# EXTRA NOTEを16段階の追加量に変更

#103の報告では、DEPTHを1から16に変えても追加数がほとんど変わらなかった。
旧方式のDEPTHは同時刻の上限で、BGMが1音ずつ並ぶ譜面では同じ配置になる場合があった。

- 最大時の配置を先に決め、DEPTHに応じて曲全体から追加候補を採用する。
- 片側の直近1秒に採用できる数を、前後2秒の元ノーツ数Nに対して
  `min(16, ceil(1.5 × N))` にする。LNの保持区間、同レーンの前後間隔、同時押し上限も検査する。
- BMS系の入力では、WAV定義のHashMapから生成されるSoundIdの順序が読み込むたびに変わり得た。
  WAVキー順に固定し、EXTRA NOTEの音ごとの配置と同順位の候補の選択を再現できるようにする。

実装前は、WindowsでOTHERS 124譜面とEVENTS 3,021譜面に試案を適用して比較した。
書庫内を含め、各フォルダの同一譜面と元ノーツ0件を除外した。元の配置、Auto LN、
スクラッチ追加OFF、ADD LN OFF、BMS分岐seed 0で比較した。
EVENTSは読込失敗の3譜面と暗号化書庫1件を集計から除いた。
前案の `min(12, N)` と比較すると、DEPTH 16時の譜面ごとの倍率の平均は
OTHERSで1.70倍から1.94倍、EVENTSで2.11倍から2.49倍、7鍵NORMAL 4〜9の36譜面で2.46倍から2.98倍だった。
同時押し上限を1レーン増やす案は、同じ7鍵NORMALで2.98倍から3.00倍への変化だったため見送った。
実装前の集計の生データは `.local/extra-note-depth-paired-OTHERS.tsv` と
`.local/extra-note-depth-paired-EVENTS.tsv` にあり、Git管理外。

実装後も同じ3,145譜面でDEPTH 0・1・4・8・12・16の追加数を確認した。
DEPTH 16の平均はOTHERS 1.94倍、EVENTS 2.49倍、7鍵NORMAL 4〜9で2.98倍だった。
3,053譜面は最大で16個以上を追加でき、全16段階の追加数が異なる。
EVENTSの全3,132読込成功パスを別プロセスで再計測し、全出力のSHA-256が一致した。
実装後の生データは `.local/extra-note-depth-final-OTHERS-one.tsv` と
`.local/extra-note-depth-final-EVENTS-one.tsv` にあり、Git管理外。

Windowsで整形、ワークスペース全体のcheck、全ターゲットのClippyとテストが通過した。
bmz-playerのライブラリテストは3,226件成功、88件ignoreで、EXTRA NOTE関連の25件が成功した。
実プレイによる難しさの確認は未実施。

適用順、配置制限、スコアの扱いは [EXTRA NOTEの配置仕様](../../docs/extra-note.md) を参照。
