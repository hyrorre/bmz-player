# スキン文字の欠字表示・フォントfallback・カラー絵文字

Solomon難易度表の✡（U+2721）が多くのスキンで完全に消える問題を調べ、
欠字表示、`font.fallback`とOSの記号フォント、書記素クラスタとカラー絵文字の順に実装しました。
現在の仕様は[skin.md](../../docs/skin.md#フォントのfallbackと欠字表示)を参照してください。

- 実施日: 2026-10-20
- 対象: `claude/unicode-emoji-text`ブランチ（main `ebe5609`から分岐）
- 比較対象: beatoraja `83574048ba489082eb46df7fe9806b464bf1f526`
  （`SkinTextFont`、`SkinTextBitmap`、`SkinFontSource`、`JsonSkin`、`JsonSkinObjectLoader`、
  `JsonSkinSerializer`、`LuaSkinLoader`）

## 根本原因

- ビットマップフォント（`.fnt` / `.lr2font`）は、グリフが無い文字を`continue`で読み飛ばしていました。
  文字も幅も消え、Truncateの判定からも外れていました。
- スキンが明示したベクターフォントは単一フォントで描き、無い文字は`.notdef`（glyph 0）を描いていました。
  `.notdef`が空のフォントでは文字が消え、既定フォントのCJK fallbackも使われませんでした。
- 既定フォントのfallbackは文字単位で、記号・絵文字のOSフォントを含んでいませんでした。
- 処理が`char`単位のため、ZWJ・異体字セレクタ・国旗などの複数コードポイントの文字を扱えませんでした。
- 文字atlasは白い輪郭を文字色で塗る前提で、カラーグリフの画像を描く経路がありませんでした。

## beatorajaとの差

| 項目 | beatoraja | BMZ |
| --- | --- | --- |
| TTFの欠字 | fallback → □ ▢ ☐ ■ ? → 生成矩形 | 同じ候補順。間にBMZの既定フォントとOSの記号・絵文字フォントを挟む |
| `.fnt`の欠字 | fallback `.fnt` → 〜/～代用 → 候補。候補も無ければ描かない | 同じ順の後、ベクターfallback・OSフォント、最後に生成矩形 |
| `font.fallback` | 文字列 / `{path,type}` / `{value}`。Luaは文字列かtable | 同じ。単一の値も受け付ける |
| BMP外の文字 | 私用領域へ割り当てて描く | `char`で直接扱う |
| ZWJ・異体字セレクタ | 個別のコードポイントとして欠字表示になりうる | 書記素クラスタで扱い、セレクタ等は幅0 |
| カラーグリフ | `.fnt`のfallbackページのみ白×alphaで描く | COLR v0/v1・CBDT・sbixを描く |
| レイアウトキャッシュ | glyph revisionで無効化 | 解決結果はフォント構成変更時にatlasごと作り直す |

beatorajaはスキン描画でOSフォントを使いません。OSフォントでの補完はBMZ拡張です。

## 実装の判断

- 文字列はフォントセットごとに一度だけcluster列（`ShapedText`）へ解決し、
  幅計算・Truncate・Wrapping・caret・描画が同じ結果を使います。寸法はスケール1の設計単位で持ち、
  解像度が変わっても再利用します。
- 主フォントの寸法は従来のab_glyphの`PxScale`（ascent - descent）のまま、別フォントはemを揃えます。
  ASCII・CJKのグリフ選択と送り幅は従来と同じです（単一コードポイントの文字はシェーピングしません）。
- シェーピングは複数コードポイントのクラスタだけに使います。既存の描画結果を変えないためです。
- カラーグリフは文字色を掛けず、quad色を白×alphaにします。シェーダとinstance形式は変えていません。
- OSの記号・絵文字フォントはmemory mapします。Apple Color Emojiは100MBを超えるためです。
  アプリ同梱フォントはWindowsの更新時にファイルがロックされないよう、従来どおり読み込みます。

## 採用したライブラリ

| crate | 用途 | 理由 |
| --- | --- | --- |
| `unicode-segmentation` 1 | 拡張書記素クラスタ | 既存の依存（egui、bms-rs） |
| `rustybuzz` 0.20（MIT） | GSUB / GPOS / AAT morxのシェーピング | Pure Rust。既存の`ttf-parser` 0.25を共有 |
| `unicode-properties` 0.1 | Emoji_Presentation | `rustybuzz`の既存依存 |
| `skrifa` 0.40 | COLRのpaint graph、CBDT / sbixのstrike | `epaint`が既に依存。OS固有APIを使わない |
| `ab_glyph_rasterizer` 0.1 | clip用の輪郭の被覆率 | `ab_glyph`の既存依存 |
| `memmap2` 0.9 | OSフォントの読み取り専用map | Linuxでは既存の依存 |

cosmic-text / swashは、既存のatlas・レイアウトを置き換える範囲が大きく、swashはCOLR v1に未対応のため見送りました。
COLRの合成（clip・layer・合成モード・グラデーション）は`color_glyph.rs`に実装しています。

## 検証

自動テスト（Linux、x86_64）:

- `cargo test -p bmz-render`: 自作テストフォント（`scripts/generate_test_fonts.py`）で、
  欠字表示・候補順・幅・揃え・Truncate / Shrink / Wrapping・影・縁取り、`font.fallback`の順序、
  書記素クラスタ、GSUB合字（家族・国旗・肌色）、表示形式の選択、COLR v0のパレット色、
  COLR v1の線形グラデーション、sbixの配置、atlas・解決結果の再利用を確認しました。
- 依頼の検証文字列12行をCPUで合成した画像テストで、どの行も文字が消えず、
  絵文字を含む行だけにフォント由来の色が出ることを確認しました。
- 欠字の読み飛ばし、cluster単位のシェーピング、カラー判定をそれぞれ無効化すると、新規テストが失敗することを確認しました。
- `cargo test -p bmz-player`のskin_loader（同梱スキンsubmoduleあり）で、JSON / Luaの`font.fallback`のdecodeを確認しました。

実環境のフォント（このLinux環境: DejaVu Sans、FreeSerif、Noto Color Emoji（CBDT））:

- ✡がDejaVu Sansの輪郭、✡️・😀・🎵・❤️・👍🏻・🇯🇵・家族ZWJがNoto Color Emojiのカラーで描かれることを、
  合成画像を目視して確認しました。画像はリポジトリに含めていません。

未実施:

- WindowsのSegoe UI Emoji（COLR）、macOSのApple Color Emoji（sbix）での実機確認
- GPU上での実際の画面表示（選曲・決定・プレイ・リザルト画面）と動画書き出し
- 第三者スキン（Rmz-skin等）を起動しての目視確認

## 制限

- OpenType SVGグリフと単色のEBDTは未対応です。
- シェーピングはクラスタ内だけで、カーニング・クラスタをまたぐ合字・双方向テキストは扱いません。
- COLRの可変フォント軸は既定値で描きます。前景色（パレット0xFFFF）は白で描きます。
- `font.fallback`の`type`（距離フィールド等）は描画方式に使いません。
