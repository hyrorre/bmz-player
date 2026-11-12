# Qwilightの情報表示とビジュアライザー

対象: `feat/qwilight-play-skins`。CR_minimalのQwilight画面を参照した表示追加。
先行修正は[判定・背景・LIFT](2026-11-13-qwilight-play-visuals.md)、現在の仕様は[skin.md](../../docs/skin.md)。

## 変更

- 幅を省略したタイトル等が読み捨てられていた。文字サイズと配置矩形を分け、点を基準とする配置と色を変換。
- NPS、経過/総時間、文字列の譜面レベル、BMZの難易度表情報、難易度画像を接続。
- 打鍵タイミングは直近800ms、判定推移は既存のノーツ判定履歴を使用。BMZの判定仕様を維持。
- 実PCMを採取し、描画側でステレオFFTを計算。固定長atomicリングを使い、音声callbackで解析やロックはしない。
- 実プレイと動画出力へ接続。OFF時・終了時は採取を停止し、再開・シーク時は古いデータを捨てる。
- 26種類の表示を既存スキンオプションに追加。pipelineヒントのある装飾も連動。外部スキン資産は未編集。

## 検証

- Windowsでfmt、workspace check、all-targets Clippy（警告をエラー扱い）、workspace testを通過。
  主な結果はbmz-player 3103件、bmz-render 1018件、bmz-skin 422件、bmz-skin-document 36件、bmz-audio 191件成功。
  外部素材・GPU・音声デバイスを必要とするignoredテストは、通常の全体テストの成功件数へ含めない。
- Qwilight関連の通常14件で、点基準の文字、NPS、時分秒・60分を超える譜面長、表示切り替え、FAST/SLOWの位置・消灯を検証。
  最後に表名を含む1003から「表記号＋レベル」の1002へ参照を絞り、GPU 2件を含む関連16件とClippyを再確認。
- `cargo build -p bmz-player --locked`でWindows debug実行ファイルを生成。
- インストール済み10種類×8キー数で、標準設定と調整値ありの定義・素材パス検証を通過。
- CR_minimalの6Kを67素材atlasでGPUオフスクリーン描画。合成PCMを実際にAudioEngineで再生処理し、
  出力から算出したスペクトラム、入力履歴9回によるNPS、1:06/2:38、曲情報、INSANE/LV18、表レベルを確認。
  追加情報をOFFにした画像と比較し、対象領域の変化を検証。
- 音声単体テストで低音/高音、逆相ステレオ、無音、無効化、リセット、非有限PCMを検証。
- ローカル画像は`.local/qwilight-information-previews/`、生ログは`.local/qwilight-information-validation/`。
  いずれもGit管理外で、この環境だけに保存。
- 実ウィンドウでの手動プレイ、聴感、実音声デバイスの負荷、他OS、Qwilightとの画素一致は未確認。
