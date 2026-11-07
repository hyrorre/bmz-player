# 選択したスキンフォルダに素材がない場合の互換性

`simple-play-simple`のカスタム判定`my_minimal`で、レーンごとの判定文字が表示されると報告された。
BMZは選択先の`parts.png`が欠けていると、wildcard探索で`judge_typeS/parts.png`を見つけていた。
beatorajaの`SkinLoader.getPath`は選択先をパスに埋め込み、`JsonSkinLoader`は画像が存在する場合に読み込む。
BMZの画像・フォント解決も、既存の選択フォルダ内で子素材が欠けているときは欠損を返す。
対象とする解決規則は[スキンのファイル参照](../../docs/skin.md#スキンのファイル参照と外部スキンの許可)に記載した。

## ローカルスキンへの影響

変更前の`4cd39145`で、ローカルのLua / JSONスキン161件について、既定設定とフォルダ選択4,439通りを
調べた。フォルダ選択は1項目ずつ変更した。読み込めた保存済み設定180件では、今回の補完に依存する
素材は見つからなかった。読み込めなかったスキン入口23件は判定対象外とした。

- `simple-play-simple` / `simple-play-bmz`の`pop`は`pop/0.png`を要求し、現行BMZでは
  `custom/0.png`が使われていた。補完がなくなると5・7・10・14鍵のスクラッチノーツの画像が欠ける。
  手元に保存した`simple-play-simple`の配布ZIPにも`pop/0.png`はなく、`synth_red_b.png`が入っていた。
  simple-play系の素材修正は後続作業とする。
- `Starseeker`の`dark2`と`ADFX02/Starseeker`の`PD_HappySky`（5・7鍵）は、別フレームの
  `wakeup.png`で表示していた開始演出が消える。

## 検証

Luaの回帰テストで、既定・保存済みのフォルダで子画像が欠けている場合と、画像がある選択先、
削除済みの選択先、`Random`での読み込みを確認した。
上記4スキンの7鍵設定を使ったローダーの前後比較でも、別フォルダからの補完がなくなることを確認した。

Windowsで`cargo fmt --check`、`bmz-player`の`check`とall-targets Clippyが成功した。
`bmz-player`は通常権限で2,914件、`bmz-render`は1,002件、`bmz-skin-document`は28件成功した。
今回変更した`bmz-player`に依存しない`bmz-skin`では、インストール済みLR2スキンを使う9件が
include未解決などで失敗し、402件が成功した。

調査・比較ログはGit管理外の`.local/skin-child-audit/`に保存した。実ウィンドウでの再生比較は未実施。
