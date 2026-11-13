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

## 第2段階

段階名と読み方は[起動診断](../../docs/startup-diagnostics.md)に整理した。
`WinitApp::new()` の先頭から `before_catalog_ms` までの各処理をINFOの開始・完了で囲み、
初期選曲リスト内の難易度表フォルダ・お気に入り・その他DB / 整形も分けた。
共通の `Instant` からの累積時間と各段階の実時間を記録する。
エラーは呼び出し側が `?` や既存フォールバックを適用する前に観測する。
親段階が中断した場合も成功とは記録しない。設定解析エラーのTOML本文は認証情報を含み得るため、
診断には位置だけを記録し、元のエラーは従来どおり呼び出し側へ返す。

通常の `load_items_for_stack()` / worker用の一覧生成は計測情報なしで呼び、
起動時だけ明示的な入口から計測情報を渡す。プロセス共通の計測状態は作らない。
bootstrap・イベントループ準備、constructor後半、window / GPU / egui・Select待ち・初回presentも接続し、
既存集計の名前と意味を維持した。非同期化やスキンローダー全体の仕様変更は行っていない。

`startup environment` は取得済みのAppPathsと `current_exe()` の結果を使い1回出力する。
ログのためのcanonicalize、再帰探索、アセット再読み込みは追加しない。
loggerの設定方法と明示的なログレベルの優先順位は変更しない。

第2段階の `cargo check -p bmz-player --locked`、all-targets Clippy（`-D warnings`）、fmtが成功。
診断の5件（INFOでの順序・時間フィールド、エラー連鎖と中断、設定本文の省略、起動限定、伝播前の失敗）、
初期skin処理5件、選曲一覧worker 4件、難易度表fetch runtime 5件（1件無視）、
日本語ロケールの選曲flow 161件が成功した。

releaseで生成されたbmz-renderのrlibにリンクした単独プローブも実行した。
`std::hint::black_box` を通して引数なしmanifest、SkinContext、Rendererを順番に生成・破棄し、
各区間の前後にstdoutマーカーを書いた。`strace -qq -f -e trace=%file,write` で、
3区間ともファイル関連システムコールが0件であることを確認した。
プロセス開始時の動的リンカー等のアクセスは区間より前にあり、結果に混同していない。
プローブと生ログは `/tmp/bmz-default-probe.rs`、`/tmp/bmz-default-probe.strace` にある。

初回I/Oの回帰テストは新規プロセス内で単独実行するよう強化し、第1コミットに含めた。
他の並列テストによるOnceLockの事前初期化に隠されることを防ぐ。
対象テストとbmz-renderのall-targets Clippyも再確認して成功した。

最終状態の `LANG=ja_JP.UTF-8 LANGUAGE=ja LC_ALL=ja_JP.UTF-8 cargo test -p bmz-player --locked --no-fail-fast`
はlibで3122件成功・0件失敗・53件無視。他target / doc-testも成功した。

## release実行確認

`cargo build -p bmz-player --release --locked` が成功した。
両段階のproduction変更を含む作業ツリーからビルドした（ビルド識別はcommit前のdirty状態）。
Linux Xvfb 1280x720 / Mesa lavapipe Vulkanで、実行ファイル、default / mz-select、フォント、
サンプル曲を `/tmp/bmz-startup-smoke-20261113` へコピーし、空のcwdから起動した。
resource / data / cache / logsもこの検証ディレクトリ配下を明示した。
音声デバイスはなく、既存フォールバックで音声なしで続行した。

起動時scan・gamepad・更新確認・27個の難易度表sourceを無効化した検証専用設定を用意した。
予備起動後、同じ設定・DBスナップショットを毎回復元してdefaultで3回起動した。
`--renderer vulkan --smoke-exit-after-frames 30 --smoke-screenshot ...` で正常終了・PNG保存・
初回presentを確認した。OS / GPUキャッシュのcold状態は統制しておらず、修正前後の速度比較ではない。

| スキン / 回 | before_catalog_ms | skins_ms | after_skins_ms | constructor_total_ms | startup_to_first_present_ms |
|---|---:|---:|---:|---:|---:|
| default 1 | 14 | 1 | 1 | 18 | 2497 |
| default 2 | 50 | 3 | 7 | 62 | 1482 |
| default 3 | 10 | 1 | 3 | 17 | 1395 |
| mz-select | 6 | 1 | 4 | 12 | 4475 |

`renderer_default` の記録値は全て0ms（ミリ秒の表示単位未満）だった。
I/Oなしの根拠はこの数値ではなく、回帰テスト・コード経路・straceである。
4回ともINFOだけで42段階のbeginと終了が全て対応し、環境ログは1回だけだった。
同梱PNGロード、初回Select install、初回present、30フレーム描画を確認し、PNGも確認した。
このLinux条件の時間はsurface生成、egui / font初期化、初回描画に現れた。
例えばmz-selectの待ちは `initial_select_skin_ready=1776ms` として記録された。

Windows報告環境、全カスタムスキンの手動・実機比較、音声・入力機器を伴うPlay確認は未実施。
素材不在で早期returnする検査や無視したGPUテストまで実機確認済みとは扱わない。
約60秒のWindows問題の解消とは断定しない。
報告者には同条件で再計測し、`startup environment` から `first surface frame presented` まで、
まず `before_catalog_ms` 内の各phaseと未完了の最後のbegin、続いてwindow / surface / Select待ち /
初回描画を見るよう依頼する。

検証スクリプトは `/tmp/bmz-startup-smoke.py`、ログとPNGは `/tmp/bmz-startup-smoke-20261113/`、
集計は `/tmp/bmz-startup-smoke-summary.log` にある。この作業環境だけの素材でありGit管理しない。

## 2026-11-13 PRレビュー反映

`select_root_virtual_folders` の完了が `catalog.root_items()` より前に記録される指摘を修正した。
一覧生成を `Result::map` 内で行ってから `phase.result()` に渡し、文字列・Vecの構築を
当該段階の計測に含める。既存のエラー処理と一覧の追加順序は維持する。
検証はfmt、bmz-playerのcheck / all-targets Clippy（`-D warnings`）、
`startup_diagnostics`（5件）と `virtual_folder`（10件）の関連テストが全て成功した。
