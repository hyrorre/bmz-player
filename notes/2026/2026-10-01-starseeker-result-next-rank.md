# Starseeker ResultのMAX符号とLuaロード値

## 原因と修正

Starseeker Resultのscoreframe.luaはロード時の`main_state.number(154) == 0`で
数字画像を選択する。通常はy=292（先頭行の符号画像はマイナス）、満点ではy=165
（先頭行の符号画像はプラス）を使用する。
BMZはResultのロード用number一覧に154を供給しておらず、stubの0で満点用画像が
選択された。描画時には正しい不足点が入るため、1552ノート・EX SCORE 2969の
ケースで、本来のMAX -0135ではなくMAX +0135となった。

ResultSummaryからロード状態を生成する共通処理に154を追加した。
通常ResultとCourse Resultの両方に適用され、rendererの既存の計算を使うため、
ロード時と描画時の境界判定は一致する。154を非負で返す契約、数字画像の行順、
第三者スキンは変更しない。仕様は[skin.md](../../docs/skin.md)を参照。

ADFX02のローカルdevelop（b2b2693）とbmz（b1b0f1f）のStarseeker Resultには
画像を含め差分がなく、ブランチ切替では解決しない。現在のprofileのSelectはECFN。

## 検証

- ロード用154が不足点135、満点0、AAA到達直前1、到達時344となる回帰テストを追加。
- 各ケースでrendererの値と一致し、Luaロード状態へ引き継がれることを確認する。
- 現在のADFX02/Starseeker素材を実際にdecodeし、不足点135でy=292、満点でy=165を
  選ぶ追加テストが成功。素材が無い環境では明示的にskipする。
- fmt、bmz-player check / all-targets Clippyが成功。全テスト2064件成功・6件ignore。
  追加した実素材decodeテストも単独実行で成功し、追加後のClippyも成功。
  全テストの初回はsandboxのローカルbind拒否で17件失敗したが、制限外の再実行で解消。
- 実機でのResult表示確認は未実施。
