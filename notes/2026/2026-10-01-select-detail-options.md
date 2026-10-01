# Select E2 DETAIL OPTIONS

## 対象と判断

- 基点: `main`、`753f81cbd2f3a8b1dd7c4ea08211c77b4edd7a84`。
- ブランチ: `feat/select-detail-options`。開始時は未コミット変更なし。
- [設計と採用理由](../../docs/select-detail-options.md)、[操作](../../docs/controls.md)、
  [Skin API](../../docs/skin.md#bmz-select-detail-options-v1)を現在仕様とする。
- E1とE1+E2を維持し、E2は15項目のカタログと7可視行へ変更。
  profile/registryを設定の保存先とし、skin/native描画は同じ読み取り専用snapshotを使用。
- SCROLL/LN/MINEのADD系は既存の変換・Assist分類・送信ルールに差があるため、
  新パネルの変更候補から除外。既存設定経路と保存済み値を維持し、分類規則は変更しない。
- GAS方式の適用条件に加えて、CLASS系コースゲージでは下限が作用しないことを確認。
  GAS OFF等でも事前設定できる。Practice内の個別ゲージ変更は開始時runtime設定が優先する。
- 宣言`bmzDetailOptions: 1`以外は不透明な本体表示と専用hit判定を利用。
  旧event、option、timerの意味を保ち、第三者製スキンのファイルは編集しない。

## 自動検証と環境

macOS / Apple Silicon、リポジトリの固定依存関係で検証。
一時ログは`.local/detail-options-*.log`（Git管理外、他環境では存在を保証しない）。

- カタログ: 15項目の前後循環、安定ID、数値clamp/no-op、独立カバーと量保持、
  対象モード、未解決mode、GAS適用、候補外ADD、旧event・registry同等性。
- 入力: 押下エッジ、同方向・反対方向chord、OS repeat、保持中の行変更、
  7K/14Kの各サイド内奇偶、ゲームパッドslot、9Kの鍵盤優先。
  既存のhold遷移・フォーカス喪失・アナログtick変換テストも対象に含む。
- 設定: TOML往復と次のGameSessionでのカバー設定、GAS OFF中の保持と有効化後の下限。
- API: 同一snapshotのnumber/text/option/Lua text、負option、互換panel 22、
  0〜80項目のviewport、空き行、非表示値。
- 表示: デフォルトJSONの全15カーソル×日本語/英語、専用クリックとslider遮断、
  宣言なし/0/未知version/スキンなしでの標準表示。
- 初回workspaceテストは制限環境のローカル待受禁止で既存IR/OBS/download/IPCの17件が
  `Operation not permitted`となった。ローカル接続可能な環境で再実行してすべて成功した。
  製品コードへの回避変更は行っていない。
- `cargo fmt --check`、`cargo check --workspace --locked`、
  `cargo clippy --workspace --all-targets --locked -- -D warnings`は成功。
- `cargo test --workspace --locked --no-fail-fast`は成功。
  主な対象はbmz-player 2,121件、bmz-render 658件、bmz-skin 229件。
  workspace内のignore 14件は通常実行の成功件数に含めない。
  GPUテストは別途明示実行して1件成功。外部素材依存テストの早期returnを
  第三者製スキンの実機互換確認済みという根拠にはしない。

## 描画確認と未実施事項

`cargo test -p bmz-player --locked --lib detail_options_gpu_previews -- --ignored --nocapture`
で実スキンをdecode/installし、Metalを使用したoffscreen描画をPNGへ読み戻す。
ウィンドウ、profile、DB、音声、入力デバイスは使用しない。制限環境内ではGPU取得不可のため、
GPUへアクセス可能な実行範囲で実施した。

デフォルト/本体表示×日本語/英語×960×540・1280×720・1024×768・1920×1080の
16枚を生成。日本語/英語、各サイズと両経路から代表画像を画素レビューし、
行とガイドの重なり・文字切れがないことを確認した。4:3では既存canvas変換によるletterboxを使用する。
全行の存在はGPU不要テストでも検証する。

実コントローラーのアナログ感度・軸の反転・OSから届く同時押し順、実ウィンドウの
フォーカス切替、OS表示スケール、第三者製スキンを選択した実操作、音声を伴う実プレイは未実施。
GPUテストや正常起動を性能改善・実機入力の確認結果として扱わない。
手順は[動作確認手順](../../docs/select-detail-options.md#動作確認手順)に記載。

## 2026-10-01: 項目を横並び、選択肢を縦並びへ変更

ユーザーの追加要望により、7可視スロットを横方向の列に変更した。
列内にOFF/ONや全enum候補を縦並びで表示し、編集中の列を「▼」、設定中の値を「●」で示す。
項目カタログ・並び・物理操作・保存と副作用は維持する。選択肢クリックによる直接指定は
既存registryの変更を集約し、最終値に対して副作用を一度だけ実行する。同じ値はno-op。

選択肢は本体snapshotから供給し、registryの表示formatterと既存i18nを利用する。
追加の19500..19947帯は既存定数・resolver・LR2変換領域との重複を確認した。
19300/19400帯と`bmzDetailOptions: 1`の意味は維持し、既存の縦リストスキンを壊さない。
1項目8セルを予約し、現行の最大6候補を全表示する。ADDなど候補外の保存値も列内に補足する。

全15カーソル×日英の全候補ラベルとクリック対象、標準表示の選択肢クリック、
registry変更とラベルの一致、直接指定のno-op・編集不可、Lua/number/optionの一致を検証する。
ログは`.local/detail-columns-*.log`に保存する（Git管理外）。

追加変更後のfmt / workspace check / all-targets Clippy / workspace testは成功。
bmz-playerは2,122件、bmz-renderは658件、bmz-skinは229件成功。
GPUの16画像生成も成功し、デフォルト日本語1280×720、英語960×540、
標準表示日本語1024×768を画素レビューした。全選択肢が表示され、ガイドとの重なりがない。
実コントローラー・実ウィンドウ操作の未実施範囲は上記から変更なし。

## 2026-10-01: 中央選択の循環とスクロール、矢印操作、7K fallback

追加指定に合わせて、UI左右を項目移動、上を前の値、下を次の値へ変更。
鍵盤の奇偶・スクラッチ・E1/E1+E2の割り当ては維持する。
キーモード未解決時は7Kを編集する方針へ変更し、7K fallbackには譜面変換を重ねない。
解決済み譜面には既存の変換を適用する。毎フレームの同期と閉鎖・終了時保存で同じ
編集先を使い、別モードへのHS-FIX転記を避ける。7K変更と9K保持、TOML往復を検証した。

7枠を環状に並べ、中央slot 3を常に選択項目にする。少数項目も重複させず中央に配置する。
選曲の低速/高速スクロール時間と同じ線形補間で移動し、中央マーカーは固定する。
補間中の再入力は残り変位を引き継いで高速時間へ切り替える。パネル遷移時は補間を解除する。
snapshotの文字列キャッシュと補間スカラーを分離し、描画フレームごとの翻訳・配列cloneを避ける。

`bmzDetailScroll: [dx,dy]`でskin destinationの描画とhit位置を共に補間する。
GPU画像で端の空白を発見したため、枠外補助slot 7/8を追加して両端を描画する。
追加領域は行19470..19489、選択肢19948..20075、event 19317/19318。
既存定数・resolver・LR2・beatoraja SkinPropertyとの同一名前空間の衝突がないことを確認した。
デフォルトと本体表示の双方で、移動途中の通常列・補助列へのクリック、端のマスクによる
クリック遮断、中央マーカー固定をテストした。旧スキンの表示経路は維持する。

検証ログは`.local/detail-carousel-*.log`（Git管理外）。

- `cargo fmt --check`、workspace check、workspace all-targets Clippy（`-D warnings`）成功。
- workspace全体テストを実行。bmz-player 2,126件、bmz-skin 229件等は成功。
  bmz-renderの既存CIM画像テスト1件が`truncated CIM pixel data`、bmz-videoの既存
  clocked decoderテスト1件が`decoder did not reach 0`で失敗した。
  CIMの一時ファイル名は時刻のみで衝突し得るが、今回の失敗原因は断定しない。
  `cargo test -p bmz-render -p bmz-video --locked --lib -- --test-threads=1`で
  両クレートを逐次再検証し、renderer 659件、video 36件がすべて成功した。
  今回と無関係な画像・動画コードの修正はしていない。通常実行のignoreは合計14件。
- GPUテストを明示実行して成功。デフォルト/標準表示×日英×4サイズについて、
  先頭/末尾の静止時と正負0.5列の移動途中の計32枚を生成した。
  最終画像のデフォルト日本語1280×720と標準英語1024×768の移動途中を画素レビューし、
  補助列によって端の空白がなく、マスク内で連続して表示されることを確認した。
  静止時の中央配置・全カーソルの日英ラベル・クリックはGPU不要の自動テストでも確認する。

実コントローラー入力、実ウィンドウのフォーカス切替、音声を伴うプレイとOS表示スケールは
引き続き未実施。追加の手動確認手順は[設計書](../../docs/select-detail-options.md#動作確認手順)を参照。
