# Qwilightの終了演出とプレイ表示の補完

## 対象

2026-11-13、`feat/qwilight-play-skins`で、既存のQwilightプレイスキン変換へ残っていたプレイ表示を追加した。
前段は[表示とLIFT](2026-11-13-qwilight-play-visuals.md)、[情報表示](2026-11-13-qwilight-play-information.md)、
[タイミングとフェード](2026-11-13-qwilight-timing-and-fades.md)。現在の仕様は[skin.md](../../docs/skin.md#qwilightのプレイスキン)に集約する。

Qwilight固有のオンライン・ポーズUI・カメラ入力・アプリ設定用オーバーレイ・専用SE、別の`@`テーマと他画面は対象外。
採点・判定・HS・LIFT/SUDDEN+はBMZのまま。第三者製スキンのファイルは変更しない。

## 実装

- J 9 / J 10 / J 11の画像列を、BMZのフルコンボ・通常クリア・全PGREATへ接続した。
  終了演出が終わってから黒フェードへ移る。通常クリア演出の待機は自然終了かつクリア時に限り、動画出力にも適用する。
  PERFECT専用の配置・FPS等が省略された場合はFULL COMBOの定義を継承する。
- ゲージ%、FREQ倍率、進行位置に対する目標EX差、判定内訳の帯、進行バーとつまみ、残り時間を追加した。
- AUTOPLAYと自動処理レーン、レーン別FAST/SLOW画像、キー押下・解放の4モード、コンボの画像列と拡大、
  LNボムの再開周期、拍連動画像を追加した。レーン別FAST/SLOWはPGREATも生の判定誤差の符号を使う。
- ゲージ画像の種類・アニメーション・4方向の切り取り、Windowsの名前付き色、CanvasComposite 0〜12を追加した。
  Copy等の画像外の置換とBoundedCopyの違いも変換する。
- フレームごとに素材名のLua関数を評価し、途中の欠番は直前画像を保持、先頭の欠番は透明とした。
  表示ON/OFFは34グループ。終了演出OFFではその画像を読み込まない。
- Defaultのレーン別FAST/SLOWやCR_simpleの拍連動画像では、フレーム数が素数だと従来のatlasが
  1列になり、高さ上限を超えた。複数列へ並べ、末尾の空きセルはアニメーションの対象外とした。

主な参照はローカルの`.local/qwilight-reference/{UI.cs,DefaultCompute.cs}`と、
[Qwilight公式ソース](https://vcs.taehui.net/root/qwilight)の`ModeComponent.cs` / `PaintComponent/JudgmentPaint.cs`。
合成演算は[CanvasComposite](https://microsoft.github.io/react-native-windows/docs/CanvasComposite)と
[Direct2D Composite](https://learn.microsoft.com/en-us/windows/win32/direct2d/composite)を参照した。
フォント書体はQwilightでもプレイスキンの宣言ではなくアプリ共通設定であり、BMZのフォント解決を使う。

## 終了演出のメモリ

CR_minimalの大きな画像列を全てRGBAとして保持・GPUへ転送すると負荷が大きいため、
終了演出は可逆圧縮してCPUに保持し、現在の描画に必要なフレームだけGPUへ展開する。
透明画素のRGBと半透明のalphaも保持する。再プレイ・seekでは圧縮画像を再利用し、ZIPを読み直さない。
通常の小さな画像列は既存atlas経路のまま。

- 最大4096フレーム、1画像RGBA 16 MiB、圧縮後合計512 MiB、同時GPU展開128 MiB。
- 通常atlasの最大セル数は4096、入力RGBA合計・atlasそれぞれ128 MiBの既存上限を維持。
- 大きな画像の遅延転送はPlayの実描画前に行い、別のフレームへ進むと不要なGPU画像を解放する。

## 検証

以下は成功。全体テスト後の画像置換処理の補強については、描画crate全テストと実素材GPUテストを再実行した。

- `cargo fmt --all --check`、`cargo check --workspace --locked`。
- `cargo clippy --workspace --all-targets --locked -- -D warnings`。
- `cargo test --workspace --locked --no-fail-fast -- --test-threads=4`。
- `cargo test -p bmz-render --locked`：1027成功、24件は明示的なGPU等の検証用でignore。
- `cargo test -p bmz-render --lib qwilight_deferred_gpu --locked -- --ignored`：成功。
  可逆画像のGPU展開・解放・再使用、通常スキンの画像への置換、CanvasComposite 13種類のalphaを検査。
- `cargo test -p bmz-player --lib qwilight --locked -- --include-ignored --test-threads=1 --nocapture`：26成功。
  Default / CR_DXの7K、CR_simpleの14K、Defaultの14KとCR_DXのカスタマイズ、
  CR_minimalの6Kで終了演出・既存HUD・LIFT・情報表示・フェードをオフスクリーンGPU描画。
  FULL COMBO / PERFECT / CLEAR、およびDefault / CR_simpleの出力画像も目視した。
- `cargo test -p bmz-skin --lib installed_qwilight_play_skins --locked -- --ignored --nocapture`：成功。
  Steamに入っている10スキン×4/5/6/7/8/9/10/14Kについて、通常と位置調整付きの変換・素材パスを検査。

実素材テストは`BMZ_QWILIGHT_SKIN_ROOT`を`G:/SteamLibrary/steamapps/common/Qwilight/SavesDir (beta)/UI`へ設定した。
最初の実素材検証で見つかった素数フレーム数のatlas上限超過は、修正後に再検証して解消した。
描画crate単独テストは、制限付き実行環境のTEMPパスの大文字小文字が変わることで既存fontテスト3件が失敗したが、
通常環境で同じテストを実行して成功した。fontの処理・テストは変更していない。

検証環境はWindows。自動検証と実素材のオフスクリーンGPU描画を使う。
実ウィンドウでのプレイ、聴感、コントローラー、macOS/Linux、全オプションの組合せ、原作との画素一致は未確認。
GPU readbackを含むdebugテストの時間は実プレイの性能測定ではない。

検証ログと画像はGit管理外の`.local/qwilight-expansion-*.log`および
`.local/qwilight-expansion-previews/`に保存する。
