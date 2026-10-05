# ADFX02 / ECFNのDETAIL OPTIONS表示

本体の基点: `feat/select-detail-options` / `91fe074fd48a88ba39e2a8471313b31e2a284dcd`。
スキンの基点: `data/skins/ADFX02` の `bmz` / `8cc5bb16b32ebb520d581f7100e8adfe7c09ee2c`。
開始時は両repoとも差分なし。スキン側は`codex/ecfn-detail-options`を作成した。
スキンの実装commit: `7bf8a51ad4ca0b47aba2bffae50ab2e6f5eb68af`。
ユーザーの実装依頼に従い既存のローカルスキンを拡張し、本体のsubmoduleや配布物には追加しない。

## 実装と判断

- `select.lua`の最後に任意モジュールのロードを追加。`bmz_detail_options.lua`が新しいskin tableを
  完成させた時だけ対応宣言を公開する。OFF・欠落・例外・BMZ以外では旧3パネルを保持する。
- カタログ・入力・保存・翻訳は既存の本体APIを共用。全21項目、7可視列＋補助2列、中央へ循環する補間。
  数値は現在値のみ、±ボタンなし。短押し200ms未満、通常／詳細切替、奇数鍵・上で数値増加も共通仕様。
- `option.png`の既存領域のみを参照し、画像・フォントの追加／変更はしない。
  源暎エムゴHeavyを縁取りなしで使い、直線枠・黒い値欄・オレンジの選択表示へ合わせる。
- 元ECFNは200msの登場フェードのみだったため、新表示の開閉と通常の退出を200msで追加する。
  暗幕を1枚にまとめ、既存timerとdraw callbackで登場／退出の最大不透明度を反映する。
  本体の退場データと入力保護の300msは変更しない。
- ECFN readmeの改変許可・改変Luaを上書き形式で配布する条件を確認した。
  素材の再配布・pushは実施しない。ルートLICENSEだけから同梱可能とは判断しない。

確定仕様は[設計](../../docs/select-detail-options.md#adfx02--ecfn)、
[操作](../../docs/controls.md)、[API](../../docs/skin.md#bmz-select-detail-options-v1)を参照。

## 検証

macOSで`cargo fmt --check`、`cargo check -p bmz-player --locked`、
`cargo clippy -p bmz-player --all-targets --locked -- -D warnings`が成功。
ECFN専用テストは明示実行して5成功（GPUを含む）。日英の全21項目、移動中のhit、数値ボタンの不存在、
空き行・編集不可・非適用・既存ADD値、OFF／欠落／構築途中の失敗／BMZ以外の復帰、
既存通常・サブオプションの定義保持、並行フェード・単一暗幕・再オープンを確認した。

`cargo test -p bmz-player --locked`は2,149成功・1失敗・15 ignored。
失敗は既存の`app::tests::result::starseeker_result_selects_next_rank_sheet_from_summary_when_available`で、
導入済みStarseekerの`RANK_Diff_Exscore`を取得できず、単独実行でも再現した。
今回Starseeker、rank_diff.lua、本体の実装は変更していない。別機能の修正やskip追加は行わず、
全テスト成功とは扱わない。新規5テストは外部素材を要求するため通常実行ではignoredになるが、上記で明示実行済み。

Metal offscreenで日英、1920×1080・1280×720・960×540・1024×768・2560×1080の53画像を生成。
代表画像で既存素材の枠・フォント・長い候補名・数値欄・日英説明・GAS非適用理由・透過背景・
100ms時点の通常／詳細の同時表示・200ms時点の退出完了・横長余白のマスクを確認した。
フォントのcmapも検査し、日英それぞれ51件の`detail-options-*`翻訳に欠落glyphがないことを確認。
GPU試験はアダプタへアクセスできるsandbox外で実行した。

ログはGit管理外の`.local/ecfn-detail-*.log`、画像は一時ディレクトリ
`bmz-ecfn-detail-preview-10052-1791169032144416000`に保存した（恒久保存・共有対象ではない）。
実行中に見つけたテストfixtureのヘッダ分岐、許可root、canvas外矩形の判別は修正済み。
暗幕はSelectでも既存サポートのdraw callbackで実装し、本体のcustom timer対応範囲は変更していない。

テストは`crates/bmz-player/src/skin_loader/tests/ecfn_detail_options.rs`。
外部スキンが必要なため通常はignoredとし、明示実行では素材の欠落を失敗として扱う。
`data/skins/ECFN`等の別パスを探して早期returnするテストの成功件数には依存しない。

```sh
cargo test -p bmz-player --locked ecfn_detail_options -- --ignored --skip gpu_previews --nocapture --test-threads=1
cargo test -p bmz-player --locked ecfn_detail_options_gpu_previews -- --ignored --nocapture --test-threads=1
```

## 実機での確認手順

1. 拡張済み`ADFX02/ECFN/select/select.luaskin`を選曲スキンに選ぶ。
2. 「選曲 > 実験的な詳細オプション」OFFで旧E1／E2／両holdの表示・操作を確認する。
3. ONで再読込後、E1を200ms未満で離して通常を固定表示し、E2で詳細へ切り替える。
4. 通常↔詳細の両方向を繰り返し、退出・登場が200msで並行し、暗幕が二重にならないことを確認する。
5. scratch／左右／wheelで項目を移動し、中央への循環と全候補表示、クリック位置を確認する。
   緑数字・判定表示オフセットは奇数鍵／上で増加、偶数鍵／下で減少し、±ボタンがないことを確認する。
6. SUDDEN+等、GAS方式・下限、LN MODEを変更し、パネルを閉じる・次のプレイ・再起動後の保持を確認する。
7. E1長押しの解放、固定中のE1押下、フォーカス喪失、他モーダルへの移行で誤操作がないことを確認する。
8. 日英と実際のOS表示スケール、SP/DP・ゲームパッド・アナログscratchで確認する。

通常ウィンドウでの実入力・プレイ開始・再起動、実コントローラー、Windows/Linux、OS表示スケールは未実施。
自動snapshotテストとGPU offscreenの確認を実機操作済みとは扱わない。

## 追補: 既存パネルに合わせた枠・発光・文字サイズ

ユーザーの比較画像を受け、通常パネルの2P「譜面の配置」を基準に表示を調整した。
開始時は本体`ccea81d4fe3a1d74576480fe1ccbac187dd2a579`、スキン`7bf8a51`で、両repoとも差分なし。
スキンの修正commitは`710d67a57dc87fa1fb5dc5698f6aa8018f398602`。

- `option-detail4`の209×496の枠から、四隅・辺・上部グラデーションを分割して再利用。
  縁の太さを拡大せず、中央列だけ明るくする。上端17px以降には焼き込み文字があるため、
  上部の切り出しは16pxで止める。元の文字や別の素材の背景色が残らないことをGPU画像で確認した。
- 値欄の灰色枠も2P側から流用。選択時のオレンジは`option-selector101`の先頭175×37とし、
  通常の`option-random2`と同じ加算合成・255→170→255の2秒周期を使う。
  以前の内側に小さく重ねる表示をやめ、灰色枠と同じ範囲へ描画する。
- 候補と数値の実効文字サイズを28pxから16px、項目名を30pxから20pxへ縮小。
  rendererは文字定義のsizeとdestinationの高さの大きい方を使うため、両方を変更して中央へ配置する。
  説明・補助情報のサイズも調整し、同梱フォントと縁取りなしの設定は維持した。
- Lua表示と説明文書のみを変更。本体の入力・設定保存・APIは変更していない。

最終状態で`cargo test -p bmz-player --locked ecfn_detail_options -- --ignored --nocapture --test-threads=1`
を実行し、5成功・0失敗。Metal offscreenで日英・5サイズ・開閉途中を含む53画像を生成し、
代表画像の枠・発光位置・文字サイズ・長い候補名・数値・非適用表示・並行フェードを確認した。
ログは`.local/ecfn-detail-style-tests.log`、最終画像は一時ディレクトリ
`bmz-ecfn-detail-preview-14220-1791170278038583000`に保存。
両repoの`git diff --check`も成功。Rustの変更はないためworkspace全体のcheck／Clippy／testは再実行していない。

実機ではスキンを再読み込みし、E1で通常・E2で詳細へ切り替えて、文字サイズと2秒周期の発光を比較する。
通常ウィンドウ・実コントローラー・Windows/Linuxでの確認は今回も未実施。
