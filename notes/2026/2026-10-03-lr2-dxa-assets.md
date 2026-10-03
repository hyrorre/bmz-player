# LR2のDXAアセット直接読込

## 対象と構成

KCOOL 1.72の `Font/barfont.dxa`、`Font/SystemFont.dxa`、`Font/title.dxa` を対象に、
展開なしで `.lr2font` と参照画像をdecodeする。
実ファイルのヘッダーは形式v3。フォント定義はLZ圧縮、参照PNGは非圧縮で格納されている。
`title.dxa` 内の `aq-kaic+.lr2font` を含め、CSVの参照名が実際に存在することを確認した。

GPU依存を持たない `bmz-skin-assets` を追加し、仮想パスとアーカイブ実体を区別する。
`SkinPathContext` のroot検証、LR2フォント相対パス、bitmap fontのページ探索、
静止画像decode、font/sourceキャッシュへ接続した。
仕様・対応版・制限は [skin.md](../../docs/skin.md#lr2のdxaアセット) を参照。

## 参照

- `.local/OpenLR2/LR2/LR2_skinload.cpp`: `FileRead_open` / `LoadGraph` 経由の仮想フォルダー読込。
- `.local/OpenLR2/DXAissue.txt`: 旧アーカイブと新DXライブラリの互換差。
- [DXライブラリ公式説明](https://dxlib.xsrv.jp/dxtec.html): 仮想フォルダーとDxaEncode/Decode。
- [旧DXArchive 1.02ソース](https://github.com/henteko/2012TeResAI/tree/master/2012TeResAI/2012teresAI/DX%20lib/DxLib_VC/Tool/DXArchive/Source/DxArchive): v1/v2の反転、v3/v4の既定キー、索引。
- [GARbro ArcDX.cs](https://github.com/morkt/GARbro/blob/master/ArcFormats/DxLib/ArcDX.cs): 索引とLZ展開。
  MIT noticeを [THIRD-PARTY-NOTICES.txt](../../THIRD-PARTY-NOTICES.txt) に保持。

## 検証

- KCOOLの実アーカイブ3つから全フォント・ページのdecodeが成功。各フォント1000字以上、複数ページを確認。
- 合成DXAでv1〜4、圧縮・非圧縮、Shift_JISと大小文字、サブディレクトリ、通常ファイル優先を確認。
- 切断データ、巨大サイズ、循環directory、不正offset・ファイル名・LZ参照を拒否する回帰テスト。
- 合成bitmap fontと画像で、CSVからdecode、キャッシュhit、アーカイブ更新後の再decode、
  Luaのroot外参照拒否を確認。JSON/CSVには従来どおりLuaのsandbox境界を追加していない。
- Windowsで `cargo fmt --check`、`cargo check --workspace --locked`、
  `cargo clippy --workspace --all-targets --locked -- -D warnings`、
  `cargo test --workspace --locked --no-fail-fast` が成功。
  bmz-player 2108件、bmz-render 658件、bmz-skin 234件、bmz-skin-assets 9件が成功。
  KCOOLは手元の実アーカイブを使って実行。その他の外部素材依存テストには早期returnがある。
- `cargo build -p bmz-player --locked` によるWindows debugビルドも成功。
- GPUでのKCOOL画面確認、LR2との実画面比較、macOS/Linux実機確認は未実施。
  KCOOLの実ファイルはGit管理外のままで、変更・展開していない。
