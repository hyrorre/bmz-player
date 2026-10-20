# クライアント側の責務と重複の整理

`bmz-ir-web`を除く調査で挙げた6件を、Select、IR取得、Result入力、設定反映、
対戦相手、音声出力の順に整理する。動作契約は[仮想フォルダ](../../docs/select-folders.md)、
[IR](../../docs/ir.md)、[gameplay runtime](../../docs/gameplay-runtime.md)を参照。

## 1. Selectの定義読込と描画を分離

- `VirtualFolderCatalog`が検証済み定義を保持する。一覧更新時に一度読み、一覧構築と
  フォルダ名表示で共有する。描画中のTOML読込・展開・検証を除去した。
- 一覧更新失敗時は表示済みの一覧と定義を保持する。rootの定義失敗は従来どおり
  仮想フォルダ以外を表示できる。profile切替も一覧更新を通じて定義を入れ替える。
- 定義変更・壊れた定義・再読込・別profileの回帰を追加。

検証（Windows）: fmt、bmz-player check・all-targets Clippy成功。全testは2,622成功、
38 ignored（別途startup 2 ignored）。制限環境では一時ファイル・localhost等で失敗したため停止し、
通常権限で再実行して成功した。ログ: `.local/client-refactor-1-tests-unrestricted.log`。
実画面・実演奏・各OS/backendの確認は未実施。性能の改善量は計測していない。

## 2. IRランキング取得の共通化

- 単曲・コースのクエリ型とprovider別通信を`ir/ranking.rs`へ移動した。
  Select/Resultは同じコース取得処理を呼び、画面側にはdebounce、世代、cache、
  submission attemptと表示用変換を残した。
- provider別の認証要否、hash、取得件数、scope、rule/LN条件は維持する。
- 既存の通信・認証・古い応答破棄・コース表示テストを利用する。

検証: fmt、check、all-targets Clippy成功。IR関連419 test成功。
ログ: `.local/client-refactor-2-tests.log`。実サーバー通信は未実施。

## 4. 通常ResultとCourse Resultの入力共通化

- IRページ対象と終了actionを`ResultControlKind`で区別し、入力処理を一か所にした。
  IR操作、panel、graph、終了の優先順とpress/repeat/入力受付条件を維持する。
- fmt、check、all-targets Clippy成功。Result関連61成功・2 ignored、コース関連21成功。
  既存回帰を利用した。実画面の操作は未実施。

## 3. 設定変更後の反映経路

- `ProfileSettingsSnapshot`と共通反映処理を追加し、eguiとSelect設定の編集・確定・
  キャンセル・保存失敗時の復元から利用する。入力再構築、一覧更新、lane、
  score context、音量・連携の反映先を一か所へ揃えた。
- Selectのplay設定はprofile一式を反映し、eguiは変更されたplay項目だけをmergeする
  既存の違いを維持する。無関係な一時オプション保持と編集/復元の回帰を追加した。
- fmt、check、all-targets Clippy成功。設定関連116 test成功。
  ログ: `.local/client-refactor-3-tests.log`。実画面の設定操作は未実施。

## 5. 対戦相手の状態と表示取得口

- `GameSession`内に分散していた相手のscore・gauge・timerと独立セッションを
  `OpponentRuntime`へ集約した。gameplayの更新経路とapp側の利用箇所を追従した。
- `display_state`で独立した相手の優先を一か所に定め、snapshotの二重変換を除去した。
  従来方式と独立方式の判定処理は維持し、将来時刻のfull-combo timerの扱いも維持する。
- 実セッションからsnapshotを生成し、優先順位・ノート数・両方式のtimerを確認する回帰を追加した。

検証: fmt、workspace check・all-targets Clippy・全test成功。gameplay 249成功、
player 2,624成功（38 ignored）。既存HCN・地雷・replay・Viewer seek回帰を含む。
ログ: `.local/client-refactor-5-tests.log`。実演奏・GPU実機は未実施。
