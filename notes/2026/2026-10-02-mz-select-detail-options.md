# mz-selectのDETAIL OPTIONS表示

## 対象と判断

- 親リポジトリは`feat/select-detail-options`、開始HEADは`593ed853b14eadd2c5aae502fa418c226bf74222`。
  mz-selectは`main`、開始HEADは`4a4002b`。双方の開始時作業ツリーはclean。
  ユーザー指定によりコミットは作成せず、親・submoduleの変更を未コミットで残す。
- 既存の[DETAIL OPTIONS実装](2026-10-01-select-detail-options.md)を利用し、
  mz-select用の表示部品を実装する。設定・入力・保存・API番号を追加変更しない。
  [確定仕様](../../docs/select-detail-options.md#mz-select)、[操作](../../docs/controls.md)、
  [Skin API](../../docs/skin.md#bmz-select-detail-options-v1)を更新した。
- 既存のシアン枠、暗い立体ボタン、白文字に合わせる。指定どおり三角・丸を使用せず、
  選択列は太枠と見出し背景、現在値は二重枠で示す。項目は本体の補間で中央へ移動する。
- 新規画像・フォント・runtime callbackは追加せず、同梱フォントとpanel/textを利用する。
  Lua生成ループはロード時のみ。翻訳済みラベル・状態は本体snapshotから取得する。
- 新部品は`enable.txt`のshutter直前に置き、通常UIを不透明背景で覆う。
  両端のマスクにはクリックを消費するhitも置く。
- 正常ロード時だけ対応宣言を伝播し、旧optionpanel3/4のE2 destinationを除去する。
  timer 32の閉じる演出も除去し、E2解放後に旧パネルが現れないようにする。
  部品が無効・欠落・失敗の場合やBMZ外では旧部品を保持する。E1・E1+E2は維持する。

## 検証

macOS / Apple Siliconで実施。ログは`.local/mz-detail-*.log`（Git管理外）。
追加テストはmz-select素材の存在を必須にし、未初期化による早期returnを成功扱いにしない。

- 実スキンをdecode/installし、全15項目×日本語/英語×変位-1/-0.5/0/0.5/1について
  ラベル・候補・見出し/候補クリック位置・枠外クリック遮断・slider遮断を検証。
- E2非表示、E1、E1+E2、旧E2閉鎖timer有効時の新パネル非表示とクリック非発火を検証。
- 0/1/3/6項目、編集不可、候補外ADD、GAS OFF中の編集可能・非適用を検証。
- optionpanel3/4の双方で、ロード成功・無効・欠落・例外・BMZ外の5条件を比較。
  対応宣言、旧E2開閉destination、E1/E1+E2の保持を検証。

- `cargo fmt --check`、`cargo check -p bmz-player --locked`、
  `cargo clippy -p bmz-player --all-targets --locked -- -D warnings`は成功。
- `cargo test -p bmz-player -p bmz-skin -p bmz-render -p bmz-skin-document --locked --no-fail-fast`
  を実行。skin 229件、render 659件、skin-document 10件は成功。
  playerはローカル通信の制限で既存IR/OBS/download/IPCの17件が`Operation not permitted`になった。
  通信を許可した環境で`cargo test -p bmz-player --locked`を再実行し、2,129件すべて成功した。
  通常実行のignoreはplayer 8件、render 3件で、成功件数には含めない。
- `cargo test -p bmz-player --locked mz_detail_options_gpu_previews -- --ignored --nocapture`
  をGPUへアクセスできる環境で明示実行し、1件成功。
  960×540 / 1280×720 / 1024×768 / 1920×1080に対し、日本語のSUDDEN+/GAS下限、
  英語のLN MODE、日英の左右移動途中の計20枚を生成した。
  代表画像の画素レビューで見出し・全6候補のLN MODE・長いHS CONFIG・理由・ガイドの
  重なりや文字切れがないことを確認。4:3は既存canvasのletterboxを利用する。
  初回レビューで見つけたタイトル重複と外周枠の途切れは修正し、最終画像で再確認した。
- 確認用画像は`.local/previews/mz-detail-options-{ja,en}.png`にコピーした（Git管理外）。
  差分検査は既存LuaのCRLFを許容して実行し、元の改行を保持した。

## 実機確認の範囲

GPU offscreen描画は実行し、画像を確認した。実ウィンドウでの連続入力、実コントローラー、
音声を伴うプレイ、OS表示スケール、beatoraja実行は本作業の自動検証には含めない。
profile/DBを書き換えずに表示を確認するためであり、これらを実施済みとは扱わない。
[操作確認手順](../../docs/select-detail-options.md#動作確認手順)のmz-select項目を用いる。

## 追加指定: 既存素材・フォント、登場アニメーション、背景透過

同日の追加指定に合わせて、上記の初版を以下へ変更した。コミット不要の指定は継続する。

- timer 22、300ms、acc 2を使い、元のAssistと同じ減速カーブで左から登場する。
  destinationとhitが同じフレーム座標を使い、登場途中にも見えた候補を操作できる。
- `default_optionpanel4/panel2.png`からボタン枠、シアンの外枠、文字のない見出し背景を切り出す。
  値の焼き込み文字を黒い矩形で覆ってラベルを描く。選択値の発光は同部品の`cursor.png`を利用する。
  元の画像ファイルは変更せず、新しい画像も追加しない。
- 選択肢の英数字はバージョン/IR欄と同じ`m_select_system.fnt`のglyph・atlasを使う。
  このfontは括弧を持たないため、`choices.fnt`に元のglyphを保持し、括弧2文字だけ
  同梱`m_select_profile_24`のatlasから補う。ラベルを書き換えずAUTO(LN)等を欠落なく表示する。
  元の全glyph一致と日英全候補の文字収録をテストする。
- 全画面背景は元のパネルと同じ最大alpha 168の暗幕。列と説明欄も半透明にする。
  クリックは透明な全画面hitで消費し、背後のUIには渡さない。
- 添付画像の範囲に合わせて、一覧・カテゴリ・現在値・説明・補助情報・理由だけを表示する。
  タイトル、スコープ、件数、操作ガイド、ナビゲーションボタンは削除。
  マウスでは列見出しと選択肢を直接操作する。
- 1列274pxでcanvas幅を使い、canvas内の不透明な端マスクを除去する。
  枠外2列は登場完了後から描画し、開き始めの補助列の飛び出しを防ぐ。
  横長画面の左右余白への描画だけcanvas外でマスクする。

追加分の検証ログは`.local/mz-detail-revision-*.log`。

- fmt、bmz-playerのcheck / all-targets Clippy（`-D warnings`）成功。
- `cargo test -p bmz-player -p bmz-skin --locked --no-fail-fast`成功。
  player 2,131件、skin 229件。通常実行のGPU等8件のignoreは成功件数に含めない。
- 新規のfont/material検証と登場アニメーション検証を含むmz-select専用テスト5件が成功。
  時刻0/75/150/225/300/500msで座標とhitを比較し、登場中の補助列非表示も確認した。
- GPUテストを明示実行して1件成功。従来4サイズに2560×1080を加え、静止時・項目移動中・
  登場途中の計45枚を生成した。左右余白の画素を検査し、枠外の列が漏れないことを確認した。
  日英の代表画像で、既存ボタンの文字が残らないこと、括弧を含む選択肢が読めること、
  選曲画面が透けること、登場途中の配置を画素レビューした。
- 最終プレビューは`.local/previews/mz-detail-options-refined-{ja,en}.png`、
  登場150msは`.local/previews/mz-detail-options-entering.png`（いずれもGit管理外）。
  表示の背景にはテスト用の曲行を使い、ユーザーのprofile/DBは変更していない。

実ウィンドウでの操作・実コントローラー・音声を伴うプレイは引き続き未実施。

## 2026-10-03: E2解放時の退場アニメーション

- mz-selectにtimer 32、300ms、acc 2で左へ戻る退場を追加。暗幕も透明へ戻す。
  対象は可視7列と説明欄。補助列を退場時に右から新たに進入させない。
- rootの`bmzDetailOptionsClose=true`を明示したスキンだけに、最後の表示snapshotのArcと
  列変位を短時間保持する。設定用snapshotは即座に終了し、表示・編集の寿命を分ける。
  Luaへの設定ロジック移管や設定コピーの追加はしない。
- E2解放時から設定event、クリック、slider、ホイールを遮断する。退場後は通常操作へ戻る。
  再オープン、他のパネル、フォーカス喪失、モーダル、画面遷移で古い表示を打ち切る。
- option 22とtimer 32の従来の意味は維持する。新しい数値IDは追加しない。
  対応宣言なしのスキンと本体標準表示は即座に非表示となる従来動作を保つ。
- 元Assistと同じ小さな実装に揃え、登場途中で離した場合も、退場は開き終わりの基準位置から開始する。
  閉じる途中で開き直した場合は登場timerを最初から開始する。

検証ログは`.local/mz-detail-close-*.log`（Git管理外）。

- fmt、workspace全体のcheck、all-targets Clippy（`-D warnings`）成功。
- `cargo test --workspace --locked --no-fail-fast`成功。3,618件成功、15件ignore。
  分離プロセスで同じテストを再実行する子テスト1件は合計に重複計上しない。
  主な対象はplayer 2,133件、render 659件、skin 229件、skin-document 10件。
- 最後のマウス入力ガード変更後にbmz-player全テストを再実行し、2,133件成功、8件ignore。
- 退場時刻0/75/150/225/299/300/500ms、列変位-0.5/0/0.5、日英で
  値・座標の保持、クリック・slider遮断、期限切れ、他パネル、対応宣言なしを検証した。
  resolverテストでは同一Arcからref/text/optionを解決することと、互換option 22がfalseのままであることも確認。
- GPU offscreenテストを明示実行し1件成功。5サイズ×14ケースで70枚生成。
  静止・登場・項目移動に加え退場0/75/150/225/300msを含め、左右余白への漏れも画素検査した。
  代表画像で退場中の文字・選択枠、透過、300ms後の通常画面を目視確認した。
- 退場プレビューは`.local/previews/mz-detail-options-closing-75ms.png`と
  `.local/previews/mz-detail-options-closing-150ms-en.png`へ保存（Git管理外）。

実ウィンドウでの連続開閉、実コントローラー、OS表示スケール、音声を伴うプレイは未実施。
ユーザーのprofile/DBには触れていない。[手動確認手順](../../docs/select-detail-options.md#動作確認手順)の
mz-select項目に、解放・退場中のクリック/ホイール・再オープン・別パネル・F1・フォーカス喪失を追記した。
コミットは作成していない。

## 2026-10-03: 未宣言スキンの従来UI・操作を維持

追加指定により、未宣言スキンを新UIの本体オーバーレイで覆う方針を取り消した。
以前の節にある本体新UIへのフォールバック・全スキンで同じ物理操作という記載は当時の仕様であり、
現在は[設計](../../docs/select-detail-options.md)と[Skin API](../../docs/skin.md#bmz-select-detail-options-v1)に従う。

- 新E2は`type: 5`かつ`bmzDetailOptions: 1`のときだけ有効。表示・入力・snapshotの判定を揃えた。
  未宣言、0、未知versionでは旧スキンのAssist表示・クリックをそのまま使い、スキンなしも旧7トグル表示に戻す。
- 不要になった本体新UIの描画・hit処理を削除。未宣言スキンに古い新UI snapshotが渡っても
  renderer側でref/text/optionを非表示値にし、旧パネルと旧クリックを阻害しない。
- `main`の従来割り当てを確認し、キーボード・ゲームパッドのKEY1〜7を301〜307に復元。
  新設の設定変更処理は作らず、旧skin eventと同じtoggle・保存対象更新・プリロード無効化経路へ通す。
  スクラッチは旧E2内で消費し、新しい項目移動には使わない。E1/E1+E2の処理は維持する。
- defaultとmz-selectは既に宣言があるため新UIを継続。mz-selectの部品が無効・欠落・失敗なら
  元のAssist表示・固定鍵操作に戻る。旧option/timer/eventの番号・意味は変えない。
- デバイス不要テストで未宣言・0・未知version、スキンなし、残留snapshot、301クリック、
  キーボード/ゲームパッドの7トグル対応、宣言のある既定スキンの新UIを検証する。

新E2専用の7K fallbackを使うモード有効化・保存対象解決も同じ宣言条件に限定した。
未宣言スキンでAssistを開くだけで、新E2用の編集キーモードへ切り替わることを防ぐ。

検証ログは`.local/mz-detail-legacy-*.log`（Git管理外）。

- fmt、workspace全体のcheck、all-targets Clippy（`-D warnings`）成功。
- workspace全テスト成功。3,620件成功、15件ignore（分離プロセスの子テスト1件の重複を除く）。
  player 2,135件、render 659件、skin 229件、skin-document 10件を含む。
- 最後のモード有効化・保存対象の条件修正後、player全テストを再実行して2,135件成功、8件ignore。
- GPU offscreenテストを明示実行して1件成功。4サイズ×日英・カーソル条件で、
  defaultの新UIとスキンなしの旧Assist表示を計32枚出力し、代表画像の表示経路を確認した。
  新パネルの重なりがなく、スキンなしでは旧7トグルが表示されることを確認。
- mz-select実ファイルの宣言・列表示・クリック・開閉テストと、部品無効/欠落/失敗時の
  optionpanel3/4の旧destination維持テストも成功。

実ウィンドウでのスキン変更、実コントローラー、第三者スキンを使った連続入力は未実施。
未対応スキンでE2＋KEY1〜7とマウスを試し、mz-selectの部品を一時無効化して再読込した場合にも
旧Assistへ戻ることを[操作手順](../../docs/select-detail-options.md#動作確認手順)に従って確認する。
コミットは作成していない。

## 2026-10-03: E2とE1+E2の切り替えで退出・登場を並行再生

これまでの退場実装は、E2からパネルなしへ閉じる場合だけsnapshotを保持し、
Luaの退場destinationもE1/E1+E2の表示中は無効化していた。追加指定に合わせ、
別パネルへの切り替えでE2退場を打ち切る方針を変更した。

- E2→E1+E2でも最後の表示snapshotを300ms保持し、timer 32と23を並行再生する。
  逆方向は従来のtimer 33と新E2のtimer 22を並行再生する。E1との切り替えも同じ扱い。
- E2→E1+E2→E1→閉じると続けて解放しても、元のE2閉鎖timerを維持して退場を継続する。
  E2再オープン、モーダル、フォーカス喪失、他画面では保持表示を終了する。
- 退出側のE2設定eventとhitは無効のまま。切り替え先のパネルは操作できるようにし、
  全クリック・slider・ホイールの遮断はパネルなしへ閉じている場合だけに限定した。
- Skin APIの既存ID・互換timerの意味は変更しない。退場destinationの推奨opを
  `[19300, -22]`へ更新し、E1/E1+E2を禁止条件に含めない契約を記載した。
- 自動テストではE1↔E1+E2を比較基準にし、E2↔E1+E2とE2→E1の
  0/75/150/225/299/300msの両パネル座標、現在パネルのクリック、退出E2のクリック不発を検証する。
  両方の解放順序と短時間の再切り替えで、登場・退出timerの開始と継続も検証する。

検証ログは`.local/mz-detail-switch-*.log`（Git管理外）。

- fmt、workspace全体のcheck、all-targets Clippy（`-D warnings`）成功。
- workspace全テスト成功。3,622件成功、16件ignore（分離プロセスの子テスト1件の重複を除く）。
  player 2,137件、render 659件、skin 229件、skin-document 10件を含む。
- GPU offscreenの切り替えテストを明示実行して1件成功。
  1920×1080/1280×720、E2→E1+E2/逆方向、0/75/150/225/300msの計20枚を出力した。
  代表画像で退出側と登場側が同時に表示され、300msで切り替えが完了することを確認した。

実ウィンドウと実コントローラーによる操作は未実施。mz-selectでE2を押してからE1を追加し、
E1だけ離してE2へ戻す操作と、両押しからE2→E1の順に離す操作を確認する。
各切り替えで元パネルが左へ退出しながら次のパネルが登場し、退出中のE2をクリックしても
設定が変わらず、現在パネルの操作は有効なことを確認する。コミットは作成していない。
