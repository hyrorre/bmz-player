# Qwilightプレイスキンの試験対応

- 作業日: 2026-11-10
- 基点: `1140fb89`、ブランチ: `feat/qwilight-play-skins`
- 対象: Playのみ。設定・DB・外部スキンは変更しない。

## 参照と方式

インストール先は`G:\SteamLibrary\steamapps\common\Qwilight`。
本体のProductVersionは`1.17.14+e9ef433d56a72aeba8a00a42cb5294d2c61a2ab8`。
同じrevisionの[公開ソース](https://vcs.taehui.net/root/qwilight/-/tree/e9ef433d56a72aeba8a00a42cb5294d2c61a2ab8)
のUI定義・ComputedState・DrawingSystem・LongNoteを参照して、ファイル契約をRustで独立実装した。
原作のC#コードと外部スキン素材はGit管理対象に含めない。

YAMLとLuaによる配置評価を`bmz-skin`、ZIP読み取りを`bmz-skin-assets`に置き、
既存SkinDocumentへ変換してappの素材デコード・GPU描画へ渡す。
分割PNGのatlas化とLNの独立したcap寸法のみ共有契約を拡張した。
スコア等はBMZの値で、Qwilightのゲームルールは移植しない。

実素材で見つかった差として、数値と`then`間の空白省略、DP lane mapの展開、
`!`を含む素材名、画像フレームの省略時の引き継ぎ、LN上下端・bodyの重なりを扱った。
曲アーカイブregistryが`!/`を所有するため、スキンZIPは`!skin/`の仮想名に分離した。

## 検証

`SavesDir (beta)/UI`のDefault、CR_Core、CR_DX、CR_Fancy、CR_Mania、CR_minimal、
CR_natural、CR_simple、QMAX_Humanoid、QMAX_templateを対象に、
4K / 5K / 6K / 7K / 8K / 9K / 10K / 14Kの計80ケースで定義と素材パスを確認。
Default / CR_DXの7KとCR_simpleの14Kは実画像をデコードし、WindowsのGPUオフスクリーンで描画した。
プレビューは`.local/qwilight-previews/`に置く（Git管理外）。

再現コマンド（PowerShell）:

```powershell
$env:BMZ_QWILIGHT_SKIN_ROOT='G:\SteamLibrary\steamapps\common\Qwilight\SavesDir (beta)\UI'
cargo test -p bmz-skin --locked installed_qwilight_play_skins -- --ignored --nocapture
$env:BMZ_QWILIGHT_PREVIEW_DIR=(Join-Path (Get-Location) '.local/qwilight-previews')
cargo test -p bmz-player --lib --locked qwilight_installed_play_skins_gpu -- --ignored --nocapture
```

通常テストでは自作YAML / Lua / PNG / ZIPによって、設定変更、レーン順序、DP、
atlasの画素、LNの形状とアニメーション、ゲージ、カタログ、root境界を確認する。
LuaのOSアクセス遮断と、保護呼び出しで命令制限を回避できないことも確認する。

最終状態で以下を実行し、すべて成功した。Windowsの既存incremental cacheの
hardlink警告を避けるため、検証時は`CARGO_INCREMENTAL=0`を指定した。

```text
cargo fmt --all --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked --no-fail-fast
```

`bmz-player`は3,003件、`bmz-render`は1,016件、`bmz-skin`は416件、
`bmz-skin-assets`は17件が成功。外部素材・GPU等が必要なignoredテストを
通常テストの成功件数へ含めない。生ログは`.local/qwilight-workspace-final.log`に置く。

手動プレイ・実ウィンドウ・実入力・音声・macOS / Linuxでの実行は未確認。
全オプション組合せや原作との画素一致は確認していない。
未対応機能は[現在の仕様](../../docs/skin.md#qwilightのプレイスキン)に記載する。
