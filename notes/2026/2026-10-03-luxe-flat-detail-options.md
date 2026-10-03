# Luxez-FlatのDETAIL OPTIONS表示

## 対象

- 本体: `feat/select-detail-options`、開始時HEAD `c33c877f`。開始時の作業ツリーはclean。
- Luxez-Flat submodule: `main`、開始時HEAD `d01a305853fa22ac67731a749626d8bedc3681ca`、clean。
- mz-selectと同じ詳細パネルの操作・機能を、Luxez-Flatの既存デザインへ適用する依頼。
- 現行仕様: [DETAIL OPTIONS](../../docs/select-detail-options.md#luxez-flat)、[Skin API](../../docs/skin.md#bmz-select-detail-options-v1)。

## 調査と判断

既存のE1/E2/E1+E2は`select_skinparts/default_optionpanel/parts.lua`にまとまっている。
開閉はx=-1920との往復、300ms、acc 2で、今回の本体APIをそのまま利用できる。
本体のカタログ・入力・保存・設定スコープ・スコア分類は変更しない。
`load.lua`は部品をpcallで読み込み、`select_skinparts/enable.txt`が有効部品の順序を決める。

新部品の正常ロード時だけ`bmzDetailOptions=1`と`bmzDetailOptionsClose=true`を伝播する。
旧E2の表示はtimer 22/32だけでは尽くせず、timerなしのhover destinationが存在する。
`gas_low_limit_rect`等の画像IDはE1+E2にも使うため、旧部品のE2 destinationだけに印を付けて置換する。
部品無効・欠落・失敗時とBMZ以外では元の表示と固定鍵操作へ戻す。

外観は紫の角丸枠と黒い値欄、オレンジの選択カーソルを既存画像から切り出す。
角と辺を分けて拡大し、焼き込み文字を含めないことで画像の加工・再生成を不要にする。
見出し・説明は曲情報、状態・理由はIR欄のMgen+ bitmap fontを使う。
候補・現在値は部品に同梱するNoto Sans CJK JP Mediumを縁取りなしで描画する。
readmeの改変条件と`docs/licenses.md`を確認し、元のreadme・font_licenseを保持する。

表示範囲はmz-selectに揃えた7列＋補助2列と説明欄。▼・●・タイトル・操作ガイドを付けない。
背景は透過させ、退出snapshot・アニメーション・hit制限は既存APIへ委ねる。
旧EXTRA MODEのADD設定をカタログに増やさず、設定画面と旧eventを維持する。

## 検証

ログは`.local/luxe-detail-*.log`（Git管理外）。

- `cargo fmt --check`、`cargo check -p bmz-player --locked`、
  `cargo clippy -p bmz-player --all-targets --locked -- -D warnings`成功。
- 関連4クレートの全テスト成功:
  `cargo test -p bmz-player -p bmz-skin -p bmz-skin-document -p bmz-render --locked --no-fail-fast`。
  player 2,142件、render 659件、skin 229件、skin-document 10件、計3,040件成功、13件ignore。
  ログに別出力される分離プロセスの子テスト1件は二重計上していない。
- 新規のデバイス不要テスト5件で、日英・全15項目・全候補・移動中hit、空/少数項目、
  編集不可/非適用/外部ADD値、既存素材・フォントの文字収録、部品ロード成否を確認。
  E1↔E1+E2を含む遷移で両パネルの0/75/150/225/299/300msの座標を検証し、旧E2の
  timerなしhover領域を除去しながらE1+E2の同名領域は残ることを確認した。
  初回のテストでは「旧hoverの座標に新しい列が重なった場合もイベントなし」と誤って期待し失敗したため、
  新APIの列選択・値変更だけを許す期待へ修正して再実行した。本体の入力仕様は変更していない。
- ignoredのGPU offscreenテストを明示実行して成功。1920×1080、1280×720、960×540、
  1024×768、2560×1080で計50画像を生成し、日英・候補の多い項目・非適用表示・
  閉鎖・E2↔E1+E2の途中を含む代表画像を目視確認した。
  初回の描画後に文字の縦位置を調整し、最終状態で再検証した。
  横長画面のpillarboxへの補助列の漏れも画素値で検査した。
- Luxez-Flatの実アセットを使った検証であり、スキン未取得による早期returnではない。
  GPU画像は一時ディレクトリ、代表画像は`.local/previews/luxe-detail-options.png`に保存。
  ユーザーのprofile・DBや設定値を変更せず、テスト用snapshotを使用した。
- 実ウィンドウ・実コントローラー・音声を伴うプレイ、実beatorajaでの表示は未実施。
  BMZ以外の分岐はテスト用Luaで`bmz=nil`を与えて検証した。

## 実画面の指摘に伴う外観修正

ユーザーから、角の四角い塗り残り、オレンジの現在値カーソルの寸法差、候補文字の縁取りを指摘された。
24px四方の角画像には元カードの不透明な内側の塗りも含まれ、半透明の列背景との境目が見えていた。
角を輪郭だけの短い帯に分け、背景も同じ丸みに沿って描画する。非選択枠と選択枠の二重描画も避ける。
画像ファイルの加工や本体側の描画API追加は行わない。

オレンジ画像は透明な発光余白を含むため、その全体をボタンの寸法へ縮小すると中心の枠が小さくなる。
黒い値欄と同じ倍率で縦横を拡大し、元の配置の19pxの余白を保って重ねる。
bitmap fontの縁取りは画像へ焼き込まれているので、候補と現在値は既存の`VL-Gothic-Regular.ttf`へ変更する。
見出し・説明・状態・理由は元のbitmap fontを維持する。

GPU offscreenテストを再実行し、5種類の画面サイズで50画像を生成した。
日本語・英語の代表画像で四隅、発光枠、候補文字、長い候補と複数候補の収まりを確認した。
回帰テストでは、角の内側を元画像の不透明な塗りが覆わないこと、値欄と発光素材の倍率・余白、
候補用のvector fontのロードと縁取り・影なしを検証する。
修正後のfmt、check、all-targets Clippy（`-D warnings`）、bmz-playerの全テストも成功
（2,142件成功、10件ignore）。ログは`.local/luxe-detail-corners-*.log`。
実ウィンドウでの修正後の表示は未確認。

続いて候補文字が枠の中央より下に見える指摘を受け、VL Gothicの可視字形を基準に2px上へ調整した。
編集可能・非適用・編集不可とも同じ座標を使い、文字サイズとクリック領域は維持する。
Luxez-Flatの同梱フォントを確認すると、縁取りなしのvector fontはVL Gothicのみ。
Mgen+のmedium/boldはすべてbitmap版で、黒い縁取りがglyph画像に含まれている。
BMZ本体にはNoto Sans CJKもあるが、今回の調整ではフォントを変更していない。
関連テスト5件とGPU offscreenテストを再実行して成功した。
1920×1080のOFF/ONでは字形と枠の中心が一致し、1280×720・960×540でも
画素の丸めによる0.25px以下の差に収まることを描画画像から確認した。
日本語・英語の代表画像も目視確認した。ログは`.local/luxe-detail-text-center-*.log`。

ユーザー指定により、その後候補と現在値をNoto Sans CJK Regularへ変更した。
Luaの`font=""`から本体の標準フォント解決を利用し、`data/fonts/noto-cjk`にある
既存の同梱TTCを優先する。フォントファイルの複製やスキンroot外への直接参照を追加しない。
候補はNotoのメトリクスに合わせて描画高さと縦位置を調整し、字形の大きさと中央配置を維持する。
関連テストも標準フォント指定と同梱Notoの解決を確認する内容へ更新した。
変更後のfmt、check、all-targets Clippy（`-D warnings`）、bmz-playerの全テスト
（2,142件成功、10件ignore）とGPU offscreenテストが成功した。
5解像度・50画像を生成し、日本語・英語の代表画像で中央配置と長い候補の収まりを確認した。
ログは`.local/luxe-detail-noto-*.log`。実ウィンドウでの確認は未実施。

さらに「少しweightを上げたい」との指定に合わせ、RegularからMedium（500）へ変更した。
現行APIにはweight指定がなく、同梱フォントもRegularのみのため、既存font参照で読める
公式の`NotoSansCJKjp-Medium.otf`を部品内へ追加した。glyph自体が太いフォントを使い、
縁取り・影・重ね描きは追加しない。本体の標準フォントはRegularのまま維持する。
Regularと同じ上流コミットから取得し、OS/2テーブルのweight 500と出典・SHA-256を確認した。
16,554,004 bytesの未加工OTFとOFLライセンス、出典READMEを同梱し、配布物の告知も更新した。
Medium版でもfmt・check・all-targets Clippy・bmz-player全テスト（2,142件成功、10件ignore）が成功。
GPU offscreenテストで5解像度・50画像を生成し、日本語・英語の代表画像で太さ、中央位置、
長い候補の収まりを確認した。実ウィンドウでの確認は未実施。
ログは`.local/luxe-detail-medium-*.log`。配布スクリプトがスキン内のfontディレクトリも
再帰コピーすることを確認したが、配布パッケージの作成・起動確認は行っていない。

選択肢ボタンの四隅にも紫の角が見えるとの指摘を受け、元画像の176×39pxの切り出しを調べた。
角丸の黒いボタン自体は175×38pxで、四隅と右端・下端の1pxには元パネルの紫が焼き込まれていた。
色を被せて隠すと半透明背景に対応できないため、13本の横帯で無彩色のボタン部分だけを描画する。
拡大時は隣接帯の境界座標を共有して丸め、隙間や二重描画を防ぐ。
元画像・オレンジの発光・文字・クリック領域は維持し、画像加工や新しい描画APIは追加しない。
回帰テストでは、元画像のボタン全画素について無彩色部分が一度だけ含まれ、紫の画素が
切り出しに含まれないこと、拡大後に帯が隙間なく接続することを確認する。
修正後のfmt・check・all-targets Clippy（`-D warnings`）・関連テスト5件と
bmz-player全テスト（2,142件成功、10件ignore）が成功した。
GPU offscreenテストも明示実行し、5解像度・50画像を生成した。
日本語1920×1080・英語1280×720の代表画像で紫の角の解消と発光枠の維持を確認した。
ログは`.local/luxe-detail-button-corners-*.log`。実ウィンドウでの確認は未実施。

## 手動確認

実ウィンドウ・実コントローラー・音声を伴うプレイは自動テストと区別する。

1. 選曲スキンをLuxez-Flatへ切り替え、E2を保持する。紫の7列・中央選択・全選択肢を確認する。
2. 左右/スクラッチで15項目を一周し、奇偶鍵/上下/選択肢クリックで変更する。
   項目移動・登場中に描画とクリックが一致し、背景の曲選択・開始へ入力が漏れないことを確認する。
3. SUDDEN/HIDDEN/LIFTをOFF→ONにし保存量が戻ること、GAS OFF中に下限を変更できることを確認する。
   E2を閉じ、次のプレイ開始・再起動後にも保持することを確認する。
4. E2→E1+E2→E2、E2→E1+E2→E1→閉じる、短時間の切り替えを試し、両パネルの開閉を確認する。
   フォーカス喪失・F1・他画面への遷移で古い退場表示が残らないことも確認する。
5. 日英・ウィンドウサイズを変更し、候補・説明・非適用表示の欠けや重なりがないことを確認する。
6. `select_skinparts/enable.txt`から新部品を一時的に外して再読込し、従来のAssist表示・固定7鍵へ戻ることを確認する。
   確認後は部品を戻す。E1/E1+E2のクリックや表示も従来どおりであることを確認する。
