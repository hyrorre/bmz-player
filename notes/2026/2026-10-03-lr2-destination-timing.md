# LR2 DST時刻の互換修正

## 根拠と変更

KCOOL 1.72の判定ラインは `time=30, loop=0`、ゲージは `time=1000, loop=0` の単一時刻。
従来のBMZは開始判定より先に時刻を折り返すため、これらが非表示になっていた。
ローカル参照 `.local/OpenLR2/LR2/LR2_skindraw.cpp` の `SetDSTdrawByTime` は、
元の経過時刻で開始判定し、開始と終端が同じならその表示を保持する。

CSV変換の通常・ゲージ・コンボdestinationへ `lr2Timing` を付け、rendererで評価を分岐。
判定表示の終端制限にもLR2の規則を適用する。JSON/Luaの既定動作は維持する。
仕様は [skin.md](../../docs/skin.md#lr2のdst時刻) を参照。

## 検証

- 自動テスト: 30/1000msの直前・同時刻・直後、同時刻の複数フレーム、
  負loop、終端loop、終端超過loop、開始前へのループ、条件付きフレーム、逆順の開始/終端。
- CSV decodeから描画評価までの単一時刻保持を、外部アセット不要のfixtureで検証。
- Windowsで `cargo fmt --check`、`cargo check --workspace --locked`、
  `cargo clippy --workspace --all-targets --locked -- -D warnings`、
  `cargo test --workspace --locked --no-fail-fast` が成功。
  bmz-player 2105件、bmz-render 658件、bmz-skin 234件が成功。
  外部素材の有無で早期returnする既存テストもあるため、成功件数を全スキンの互換証明とはしない。
- LR2実行との画面比較、macOS/Linux実機確認は未実施。第三者製スキンは変更しない。
