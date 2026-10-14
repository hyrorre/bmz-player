# LR2選曲スクロールバーのalpha

2026-10-15。WMIX HDの選曲画面でスクロールバーが見えない報告を調査した。
スキン素材・CSV・ユーザー設定・DBは変更せず、共通のスライダー描画を修正する。

## 原因と変更

`data/skins/WMIX_HD/select/select.csv:1412`のtype 1スライダーは、
1000 msのDSTでalpha 0、1100 msでalpha 255、loop 0、blend 0を指定している。
LR2はblend 0でDSTのalphaを無視するため、表示開始後は透明な区間でもつまみが見える。

BMZの通常画像はこの契約を共通helperで処理していたが、スライダーだけは
`SkinRenderItem::Image`を直接構築し、補間後のDST alphaをそのまま適用していた。
loop後の大部分でalpha 0となり、クリック領域は残ってもつまみが消えていた。

スライダーも`skin_image_item_for_frame`へ渡す。移動位置・SRCのUV・素材の透過は
既存経路を使い、通常画像と同じblend・filter・center・変形の評価を適用する。
LR2属性を持たないJSON/LuaとLR2 blend 1では、従来どおりDST alphaを維持する。

## 検証

修正前に合成テストでalpha 0を再現し、WMIX HDの実素材をGPU描画して欠落を確認した。
開始前、1000 / 1050 / 1100 / 1500 / 5000 ms、リスト先頭・中央・末尾、
つまみ中心の押下と範囲外へのドラッグを回帰テストで確認した。
JSON/Lua相当の属性なしとLR2 blend 1のalphaも確認した。

- `cargo fmt --check`、`cargo check -p bmz-render --locked --offline` 成功。
- `CARGO_INCREMENTAL=0 cargo clippy -p bmz-render --all-targets --locked --offline -- -D warnings` 成功。
  Windowsのincremental cacheでhard linkが拒否されるため、Clippyはincrementalを無効にした。
- `cargo test -p bmz-render --locked --offline` 成功（744 passed、既存GPUテスト6 ignored）。
- `cargo test -p bmz-player --lib --locked --offline skin_loader::tests::lr2` 成功（55 passed）。
  WMIX HDの実素材が存在し、app経由で5個の画像sourceをdecodeしたことも確認した。
- Windowsのwgpu offscreen描画で、WMIX HDの実素材と保存済みスキン設定を使用。
  リスト101件の先頭・中央・末尾、1500 / 5000 msの修正前後画像を生成した。
  修正前はつまみが消え、修正後は青いつまみが上端・中央・下端に表示されることを確認した。
  DBや起動中appを使わず、合成したSelectSnapshotを描画している。
- 物理マウスによる実ウィンドウ操作、他スキン全設定、macOS / Linuxは未確認。
- `cargo build -p bmz-player --locked --offline`は、実行中のPlay画面が
  `target/debug/bmz-player.exe`を使用しているため、更新時にos error 5で停止した。
  プレイ中プロセスは停止していない。変更後のライブラリによるGPU描画は上記のとおり成功。
  実行ファイルへの反映は、プレイヤー終了後の再ビルドが必要。

生ログ・画像・GPUプローブは`.local/lr2-play-audit/scroll-*`に置き、コミットしない。

現在の契約は[スキン仕様](../../docs/skin.md#lr2のselect--result)と
[対応状況](../../docs/skin-compatibility.md)へ反映した。
