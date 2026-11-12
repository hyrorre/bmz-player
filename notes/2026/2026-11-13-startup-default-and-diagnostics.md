# デフォルト描画状態の暗黙アセット探索と起動診断

開始時の最新main: `b31b83b53f56666292ea125c189dd99528ce0816`。
関連仕様・手順: [スキン](../../docs/skin.md)、[開発・smoke](../../docs/development.md)。

## 第1段階

`Renderer::default()` が4個の `SkinContext::default()` を作るとき、従来は
`default_skin_manifest()` のプロセス共通キャッシュ初期化から、ビルド時の
`CARGO_MANIFEST_DIR/../../data/skins/default` に対して画像読み込みを開始していた。
画像がなければ通常画像ローダーから大小文字補正・祖先ディレクトリのDXA探索へ進み得た。

引数なしのmanifest生成は、定義と検証可能な同梱PNGの寸法だけを使う。
開発checkoutへの参照は明示的なテスト用関数に限定する。
`default_skin_manifest_for_root()` と `with_texture_source_sizes()` は明示的な読み込みAPIとして維持し、
前者の結果をルート間でキャッシュしない。appの正規ロードは引き続き
`AppPaths.resource_dir` 由来のルートを既存の `SkinPathPolicy` で検証して使う。
通常画像・DXA・大小文字補正・許可root検証の実装は変更しない。

`source_size` はNineSliceのPixels境界の正規化、および線形補間時のUV端の半texel補正に使われる。
ノーツ・レセプターのレーン別画像は同寸法であり、初期manifestのplay定義全体が明示的に
同梱PNGを読み込んだ場合と一致することをテストする。これによりLNの通常ノーツへの
フォールバック、ゲージ枠、コンボパネル、スキンロード前の状態も従来の情報を保持する。

回帰テストは、Default生成で画像ローダーを呼ばないこと（スレッド別の呼び出しカウンター）、
同梱PNGとの寸法一致、異なる明示ルートでの寸法の独立性、別resource_dirへコピーしたPNGの
app側正規ロードを確認する。時間しきい値は使わない。

報告者のWindows v0.5.1での約60秒の遅延との因果関係は未確認であり、解消とは断定しない。

検証環境はDebian 13 / Rust 1.99.1。Rustと不足する開発ヘッダーは作業領域に置き、
システムやリポジトリの依存設定は変更していない。同梱submodule 3種をmainの固定版で取得した。
`cargo fmt --check`、`cargo check --workspace --locked`、
`cargo clippy --workspace --all-targets --locked -- -D warnings` が成功。
`cargo test -p bmz-render -p bmz-skin-assets --locked` はそれぞれ1019件（23件無視）・16件成功。
`cargo test -p bmz-player --locked skin_loader --lib` は271件成功・36件無視。
素材なしで早期returnする外部スキンのケースまで、実素材確認済みとは扱わない。

英語ロケールのworkspaceテストでは既存の日本語初期表示を期待するappテスト4件が失敗した。
`cargo test --workspace --locked --no-fail-fast` の失敗targetはbmz-playerのlibだけで、
その結果は3113件成功・4件失敗・53件無視。他のworkspace targetは成功した。
該当テストは `LANG=ja_JP.UTF-8 LANGUAGE=ja LC_ALL=ja_JP.UTF-8` で成功した
（select_snapshot_rowsの10件、およびsettings edit復元の1件）。
依存準備途中のFFmpeg / PipeWire bindgenのヘッダー不足は、ヘッダーを準備した再実行で解消した。
生ログはこの作業環境の `/tmp/bmz-stage1-*.log` に置き、Git管理には含めない。
