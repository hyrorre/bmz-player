# ADFX02 / ECFNのBMZ基本拡張

本体の基点: `feat/select-detail-options` / `2092d027ab0e136292191efe426872b4c2da0d50`。
スキンの基点: `data/skins/ADFX02`の`codex/ecfn-detail-options` /
`710d67a57dc87fa1fb5dc5698f6aa8018f398602`。開始時は両repoとも差分なし。
ユーザーの指定に従って新規ブランチは作成せず、同じブランチで実装した。
ADFX02は本体Git管理外の別repoであり、本体のsubmodule・配布物には追加しない。
スキンの実装commit: `c74f7f9ea737d71f2586f066446b5a3e8bc410be`。

## 参照と範囲

mz-selectの`c731a0b`（12配置）、`8321ca7`（既存画像の文字）、
`912a46a`・`a73b25a`・`c3e6fde`（モード表示）、`a1fe730`・`5a484d9`（FORCE）、
Luxez-Flatの`a583dde`・`39c8b1d`・`e367fb6`・`d01a305`・`2479f01`・`79fdbf2`を参照した。

- 通常パネルの両サイドへF-RANDOM/MF-RANDOMを追加し、拡張ref 344/345で12種類の選択枠を表示。
- 上部フィルターは`bmz_select_mode`でALL/7K/14K/9K/5K/10K/4K/6K/8Kを表示。
- LNの実効表示ref 308を維持し、option 19168でFORCE設定の印を追加。
- 既存event 11/308、鍵盤操作、設定保存は本体の既存処理を利用。新規API、設定、譜面変換は追加しない。

確定仕様は[Skin API](../../docs/skin.md#bmz-arrange-refs)、[操作](../../docs/controls.md)、
[互換性一覧](../../docs/skin-compatibility.md)、
スキン側`ECFN/select/BMZ_EXTENSIONS.md`に記載した。

## 実装上の判断

`bmz_select_extensions.lua`を詳細パネルより先に読み込み、完成したskin tableだけを適用する。
元のtable/配列を変更しないため、構築途中の例外でも一部だけ残らない。
experimental設定とは独立し、基本拡張と詳細拡張それぞれの欠落・失敗に耐える。
BMZ以外ではどちらも追加しない。

ECFN readmeの改変条件を確認し、許可されたLua変更として実装。
`option.png`と`system_.png`を切り出し、画像やフォントの追加・加工は行わない。
通常パネルは元の209×496を維持し、415pxの選択肢領域へ高さ30px・間隔35pxで12行を置く。
文字画像は元のサイズを保ち、FはOFF、MはMIRROR、ハイフンはR-RANDOMから再利用する。
オレンジの加算カーソルは上下12pxを残して中央を縮め、元の枠厚と発光を維持する。
カーソルは文字の後に合成し、文字切り出しの黒背景で発光が欠けないようにする。

上部モードの枠と角の装飾を残し、同梱源暎エムゴ14pxで文字を描く。
FORCEは9pxで元のLN文字の上に置く。透明なmodeクリック領域は元の158×40を維持し、
FORCE文字はクリックを遮らない。新しい表示は旧destinationと同じ順序へ挿入する。

詳細パネルが有効なら、基本拡張後の通常パネルを既存の退出生成処理へ渡す。
通常↔詳細の200ms並行フェード、単一暗幕、既存の入力保護を共用する。

## 検証

macOSで`cargo fmt --check`、`cargo check -p bmz-player --locked`、
`cargo clippy -p bmz-player --all-targets --locked -- -D warnings`、両repoの`git diff --check`が成功。

ECFN専用の9テストを実アセット付きで明示実行し、全件成功した。
12種類×両サイドの選択枠、点滅、開閉・切替時のフェード、9モード×6LN設定、
ボタン両端とFORCE上のクリックを確認。基本拡張と詳細拡張それぞれの欠落・構築途中の失敗、
experimental OFF、BMZ以外への復帰も確認した。
新規テストは`crates/bmz-player/src/skin_loader/tests/ecfn_extensions.rs`に配置し、
外部スキンがない通常環境ではignored、明示実行で素材がない場合は失敗として扱う。

```sh
cargo test -p bmz-player --locked skin_loader::tests::ecfn_ -- --ignored --nocapture --test-threads=1
```

今回の実行はGPUなしの7件と、GPUを含む実行に分けた。
後者ではテストfilterのOR条件によりdefault・Luxez-Flat・mz-selectのGPUテストも実行され、
ECFNの9件を含め13件成功。これらの追加画像はECFNの表示確認件数には含めない。
Metal offscreenでECFNの基本拡張25画像、詳細53画像を生成した。
1920×1080・1280×720・960×540・1024×768・2560×1080の代表画像を確認し、
F/MF文字、12行の枠、発光、4K/6K/8K・FORCE、並行フェードの表示を確認した。
GPU試験はsandbox外で実行。画像は一時ディレクトリ
`bmz-ecfn-extensions-preview-21033-1791172443025532000`と
`bmz-ecfn-detail-preview-21033-1791172397899882000`に保存した。

`cargo test -p bmz-player --locked`はsandbox内では2,132成功・18失敗・19 ignored。
うち17件はローカル通信の待受制限だったため、sandbox外で再実行して
**2,149成功・1失敗・19 ignored**を確認した。残る失敗は既存の
`app::tests::result::starseeker_result_selects_next_rank_sheet_from_summary_when_available`で、
`Starseeker next-rank number`を取得できない。過去のECFN作業でも発生しており、
今回Starseekerの素材・実装・テストは変更していない。全体成功とは扱わない。
ログはGit管理外の`.local/ecfn-bmz-extensions-*.log`に保存した。

テスト作成中に見つけたfixtureのヘッダ分岐と200ms以降の点滅期待値は修正済み。
本体の公開APIやruntime処理への変更はなく、Rust変更はテストのみ。

## 手動確認手順

1. `ADFX02/ECFN/select/select.luaskin`を再読み込みする。
2. experimental設定OFFのまま、上部フィルターをクリックして4K/6K/8KがALLにならないことを確認。
3. LN設定をAUTO/FORCEの各LN/CN/HCNへ変更し、FORCEの印と従来の中央表示を確認。
4. 通常パネルで1P/2PそれぞれF-RANDOM/MF-RANDOMを選び、最下部2行の選択枠と文字を確認。
5. experimental設定ONで通常↔詳細を切り替え、両サイドの退出・登場と暗幕を確認。
6. 小さいウィンドウや実際のOS表示スケール、実コントローラーでも文字・入力を確認する。

通常ウィンドウでの実入力・プレイ開始・再起動、実コントローラー、Windows/Linuxは未実施。
snapshotテストとMetal offscreenを実機操作済みとは扱わない。
