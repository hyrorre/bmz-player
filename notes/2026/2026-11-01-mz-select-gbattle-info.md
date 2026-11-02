# mz-selectのG-BATTLE相手情報

合意した左下の独立パネルをmz-selectへ実装。ECFN / Luxez-Flatは対象外。
文字は同梱のmgenplus-1c-bold.ttfへ揃え、HARD等のゲージ名は削除した。
鍵盤は既存result/keys1p.pngの白鍵・黒鍵を切り出し、番号をTTFで重ねる。
枠・背景・区切りはLuaのpanelで構築し、新しいPNGやフォントは追加しない。

一覧の表示中だけランキング / rival data / ir dataの本文を条件20200で隠し、
閉じると復帰する。上部のライバル選択は維持。架空のMIRROR OFF切り替えは設けず、
実際のMIRRORオプションと既存CHART REPLICATION設定を表示する。

選択中の相手情報は自分のオプションと別のsnapshot / ref群20200..20229で渡す。
IRの取得済みメタデータとローカルリプレイを参照し、欠損は未取得のまま扱う。
リプレイは120msの停止待ち後、同時1件まで別処理で読む。
カーソル・譜面変更時の古い結果は破棄し、一覧を閉じると状態をリセットする。
固定配置は既存の乱数・permutation helperで計算し、動的なノート配置は固定鍵盤にしない。

仕様: [skin.md](../../docs/skin.md#g-battle一覧の相手情報)

検証:

- `cargo fmt --check`、`cargo check --workspace --locked`、workspaceのall-targets Clippyを確認。
  app内のキャッシュキー調整後もcheckとbmz-playerのall-targets Clippyを再確認。
- `cargo test --workspace --locked --no-fail-fast`はbmz-playerの既存HTTPテストで
  Windowsのローカル接続制限（os error 10013）に当たった。待機し続ける今回の
  テストプロセスだけを終了し、残りのworkspace targetはすべて成功。
  同じbmz-playerテストバイナリをPC内通信が可能な条件で再実行し、
  2780成功・失敗0・48 ignored。bmz-renderは366成功・失敗0・1 ignored。
- 実素材のパネル表示、旧3欄の非表示・復帰、未取得への切り替え、
  自分のオプションとの分離をテスト。固定配置は各キー数・旧/現行seed・FLIPで
  実プレイの変換結果と比較し、一致を確認。古いカーソル・譜面・score keyの結果も破棄。
  最終のALL-SCR表記調整後も対象4テスト、app check / all-targets Clippy / buildを確認。
- Windows GPUでSP、DP（FLIP）、未取得、一覧を閉じた画面を描画して目視確認。
  `cargo build -p bmz-player --locked`も成功。
- 実IRサービスとの対戦プレイ、物理入力、macOS / Linuxは未確認。

描画に使った相手情報は検証用snapshot。ユーザーの設定・DBは変更していない。
プレビューとログは`.local/notes/2026-11-01-gbattle-layout/implemented/`に保存し、コミットしない。

## 同日: 既存パネルの意匠に揃える調整

ユーザーの指定に従い、背景を既存irdata_bg.pngの約50%黒地と左の白線へ変更。
全周枠・見出し帯・内部の横罫線・水色を外し、外形と配置は維持した。
タイトルは`g-battle data`とし、NETWORK / score dataと同じm_select_system.fntの
サイズとタイトルの高さに揃えた。英字・鍵盤番号にも同じbitmap fontを使用し、
日本語の相手名は既存ライバル名と同じTTFを残す。

配置オプションは既存の12種類の画像を等倍で切り出す。
画像ごとの中央寄せ余白を除いて本文の左端へ揃える。
REPLICATIONへ見出しを短縮し、DPの1P/2Pオプション・鍵盤行とDP設定を分けて表示する。
NO DATA / LOADING / NOT FIXEDを区別する。新規PNG・フォントと本体APIの変更はない。

検証:

- `cargo fmt --check`、`cargo check -p bmz-player --locked`、
  `cargo clippy -p bmz-player --all-targets --locked -- -D warnings`成功。
- 実素材の既存テストを更新し、NETWORKと同じタイトル字体、オプション画像の
  切り替え、NO DATA / LOADING / NOT FIXED、旧3欄への復帰を確認。
- `cargo test -p bmz-player --locked --quiet`は2780成功・失敗0・48 ignored。
  startupの2テストは既存指定によりignored。ローカルHTTP通信が可能な条件で実行した。
- Windows GPUでSP / DP / 未取得 / 可変配置 / 読み込み中 / 閉じた画面を描画し、
  文字・配置・背景を目視確認。一時素材は
  `.local/notes/2026-11-01-gbattle-layout/skin-style/`に保存し、コミットしない。
- 実IR対戦、物理入力、他OSは未確認。

### 項目ラベルと名前の省略

続くユーザー指定で、RIVAL OPTION / LANE ORDER / REPLICATIONの3ラベルと
パネル内の相手名を削除。空いた行を詰め、上端・幅を保ってSPは200px、DPは302pxへ縮めた。
使わなくなったパネル専用TTF参照も外した。
NO DATA / LOADING / NOT FIXEDとDP情報未取得の表示は、NO PLAY素材の文字面と同じ
RGB(116,116,116)の灰色に揃えた。

追加指定後もfmt / check / all-targets Clippyと実素材テストを確認。
ラベル・相手名の省略、状態表示の灰色、既存欄への復帰が成功し、
bmz-playerの全テストも2780成功・失敗0・48 ignored（startupの既存2件もignored）。
GPU描画のNO DATA / LOADING / NOT FIXEDの文字面もRGB(116,116,116)で一致した。

### 上端の位置合わせ

右の譜面情報パネルの背景には上端4pxの透明余白があるため、G-BATTLEパネル全体を
4px下へ移動した。SP / DPとも、GPU描画の左右の白線が上原点619pxで始まることを確認。
既存の実素材テストも成功。配置のみの変更で、新しいテストは追加していない。

## 9Kの鍵盤8（2026-11-02追記）

9Kで元の鍵盤8が白鍵の枠になっていた。黒鍵を元の鍵盤2/4/6に限っていたため、
元の鍵盤番号が偶数なら黒鍵にした。5K/7K/DPで鍵盤に移った皿は従来どおり白鍵のまま。
mz-selectのbitmap fontではSと5が同じ形に見えるが、今回は変えていない。

- 実素材テスト`mz_battle_info_colors_keys_by_the_parity_of_their_original_lane`を追加
  （9Kと7K RANDOM-EXの皿。修正前の判定では失敗する）。
- Windows offscreen GPUで9Kを描画して確認。画像はGit管理外の
  `.local/notes/2026-11-02-gbattle-9k/`に置く。
