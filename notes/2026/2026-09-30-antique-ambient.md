# antique Ambient背景

## 依頼と範囲

antiqueにすりガラス風のAmbient演出を追加し、既定OFFのオプションとパネル透明度設定を用意する。
ユーザー指定により、同梱fork `data/skins/mz-select` のantiqueを変更した。
同submoduleの `readme.txt` にある利用・改変・再配布の許諾を確認済み。画像素材は変更していない。

## 実装

- `bmz-skin-document` のimage/BGA destinationに既定falseの `bmzAmbient` を追加。
- `bmz-render` でBGAのBase / Layer / Layer2またはPOORを長辺128pxへ合成し、
  横・縦のGaussian blur（sigma 4、radius 12、隣接tapを線形補間でまとめた各13 sample）を適用。
  合成後のpremultiplied alphaを維持し、動画texture uploadの後に同じcommand encoderで処理する。
- 専用GPU資源は使用時に確保・再利用し、対象がなくなったときに解放する。
  最大8 destination、各64画像・矩形に制限し、入れ子のAmbientは描画しない。
- 汎用背景動画がAmbientだけに表示される場合も、準備済みdraw planから表示中と判定する。
- ファイル選択値に拡張子なしの `default` が渡された場合、ワイルドカード `*` の直接解決に
  失敗すると探索を打ち切る問題を修正。許可root内の候補から `default.png` を選ぶ既存処理へ進む。
- antiqueの「Ambientモード (BMZ)」はOFFが既定。
  「Ambientパネル透明度」は0～100％の10％刻み、既定40％でON時だけ適用する。
  frame・lane・graph背景のalphaを調整し、ノーツ・数値・判定・graph本体は維持する。
  frame画像内に焼き込まれた装飾やラベルはframeと一緒に透過する。
- BGAサイズ「背景(1920x1080)」では、ON時に鮮明な全面BGAを重ねずAmbientを全面表示する。
  その他のBGAサイズ・左右配置・汎用BGA/BGI選択条件は既存の指定を使う。

仕様と操作は [skin.md](../../docs/skin.md#ambient背景bmz拡張)、
[controls.md](../../docs/controls.md#共通) に記載。

## 検証

検証ログ・1920×1080のGPU描画画像は `.local/performance/antique-ambient/` に保存（Git管理外）。
テスト画像は生成した格子模様をBGA入力とし、実際のantique画像・Lua・フォントを使用する。
OFF / ON透明度40％ / ON透明度80％ / 全面背景 / 曲BGAなしの汎用背景を対象とする。

- `cargo fmt --check`: 成功。
- `cargo check --workspace --locked`: 成功。
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: 成功。
- `cargo test --workspace --locked --no-fail-fast`: 全対象成功。
- `cargo build -p bmz-player --release --locked`: 成功。`target/release/bmz-player.exe` を更新。
- `cargo test -p bmz-player -p bmz-render --locked ambient -- --include-ignored --nocapture`:
  6テスト成功（うちGPUを使うignoredテスト2件を明示実行）。実素材がない場合に成功扱いにはしない。
  BGA layer黒透過、動画相当の同一texture更新、半透明の二重乗算防止、target再確保・解放を確認。
- ファイル選択値 `default` と `blue.png` を再現する回帰テストで、前者の失敗と修正後の成功を確認。

汎用背景のGPU検証は、実アプリと同じ `data/skins` を許可rootとして渡す。
entryディレクトリだけを許可した初期テストでは兄弟ディレクトリの `customize/` が対象外となったため、
テスト条件を修正し、汎用背景sourceが実際にdecodeされたこともassertする。

スキン側のcommitは `mz-select:b55b3a0`（`codex/antique-ambient`）。

実プレイ操作による手動確認、実動画ファイルの再生、FPS比較、macOS/LinuxでのGPU確認は未実施。
描画成功は性能改善の主張ではない。

## 2026-09-30 追記: レーン背景の不透明化

ユーザー指定により、左右・Sixtarのレーン背景とレーン明度用の黒オーバーレイを
Ambientパネル透明度の対象から外した。レーン背景は不透明度100％を維持し、
既存のレーン明度設定をそのまま使う。その他のパネルは引き続き透明度を変更できる。

- `cargo fmt --check` と `bmz-player` のcheck / all-targets Clippy / testは成功。
- `antique_ambient` の設定・GPUテスト2件を明示実行し、成功。
  Ambient OFF/ON・透明度40/80％・全面背景・汎用背景でレーン内の画素が一致することを確認。
  `.local/performance/antique-ambient/opaque-lane/` の80％画像も目視確認済み。
- スキン側commit: `mz-select:e0142eb`。本体の実行コードは変更せず、再ビルドは不要。
