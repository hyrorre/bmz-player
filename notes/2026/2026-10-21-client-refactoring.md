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
