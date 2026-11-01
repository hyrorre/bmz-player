# スキンの譜面情報供給とLuaフレーム処理

対象: [#57](https://github.com/hyrorre/bmz-player/issues/57)、[#58](https://github.com/hyrorre/bmz-player/issues/58)、[#59](https://github.com/hyrorre/bmz-player/issues/59)。
仕様は[skin.md](../../docs/skin.md)、対応範囲は[skin-compatibility.md](../../docs/skin-compatibility.md)を参照します。

## #59: Playの譜面長

- number 1163 / 1164がResult以外ではSelectの曲長しか見ず、Playでは0になっていた。
- Play / Decideの描画状態へ`RenderSnapshot.duration`から譜面長を渡す。ノーツ表示時間の`total_duration_ms`とは区別する。
- 通常開始・先読み・コース次曲・オフライン出力のPlayロード状態へも渡す。第三者スキンのソースは変更しない。
- ローカルにはSocialSkin（曲長表示・ログ）、LITONE9（Playの合計時間）、Hub / mz-select antique（区間ログ）、ModernChic（曲長に依存する数値・グラフ）がある。
- 回帰テストは分秒境界、開始前から終了後までの時刻変更、別曲での再ロード、負の長さを確認する。
- Windows: `cargo fmt --check`、workspace check / all-targets Clippy成功。workspace testはbmz-player以外が成功。bmz-playerの素材不足23件は同梱submodule初期化後に全libテストを再実行し、2790 passed / 50 ignoredだった。
- 実機での見た目とmacOS / Linuxは未確認。

## #58: 譜面ハッシュ

- text 1030 / 1031は名前の定義だけがあり、描画・Luaの値供給がなかった。
- Selectの行、Play / Decideのsnapshot、ResultSummary / ResultSnapshotへ元の譜面ハッシュを渡す。譜面変換やコース識別用のハッシュは使用しない。
- 難易度表の未所持譜面も、既知のMD5 / SHA256を渡す。フォルダ等の欠損値は空文字にする。コースResultは最後にプレイした譜面を使う。
- Select / Decideの再ロード、Playの通常開始・先読み・コース次曲・オフライン出力、Resultロードにも供給する。既存のロード依存・要求世代の管理を利用する。
- ローカルにはSocialSkin（楽曲情報照合）、Starseeker（譜面別オプション）、ModernChic471 / Old（難易度表出力）がある。SocialSkinのHTTP通信制限は別の対応範囲であり、今回変更しない。
- 回帰テストで4画面の曲切替・欠損、名前 / 数値ID、optionの同番号との独立、ロード時依存、コース最終譜面を確認する。
- Windows: `cargo fmt --check`、workspace all-targets check / all-targets Clippy / test成功。ハッシュ関連の追加3テストとコース最終譜面の回帰も成功した。
- 実機表示、外部サービス、macOS / Linuxは未確認。

## #57: Luaのフレーム更新時の割り当て

- `with_state`がcustom timer / custom eventごとにscoped Lua関数を生成していた。描画callback向けの共有scopeは既にあったが、更新処理では共有されていなかった。
- `advance_frame`でキュー済みのact / FloatWriter、timer、eventの順に1つのscopeで実行する。既存のtimer / event単独APIも、それぞれ更新全体でscopeを共有する。
- 借用した`LuaMainState`は同期呼び出し内に閉じ、既存guardで入れ子・例外時に復元する。永続closure、同一フレームのtimer読み書き、callback単位とframe全体の命令数制限を維持する。
- 合成スキン（timer 40個 + event 40個）、Windows debug、200フレーム、ウォームアップ後にGCを停止して未回収のLua割り当てを測定した。修正前は2,048,000 bytes、単独APIの共有後は51,200 bytes、`advance_frame`経由は25,600 bytesだった。1組と40組で割り当て量が増えないことを回帰テストにする。
- 入力 → timer → eventの順、1フレーム1回、同一フレームの受動timer書き込み、外側providerへの復元を追加テストで確認した。
- 生ログはworktreeの`.local/issue57-before.log` / `.local/issue57-after.log`に保存する（Git管理外）。
- 合成テストはLuaの一時割り当ての確認であり、実プレイの表示FPS・GC停止時間・surface/present待ちの改善を計測したものではない。報告された9〜294msの停止、macOS / Linuxは未確認。
- Windows: `cargo fmt --check`、workspace check / all-targets Clippy / test成功。bmz-playerは2791 passed / 50 ignored、bmz-renderは986 passed / 11 ignored、bmz-skinは373 passed / 1 ignoredだった。
- 主checkoutの第三者スキンを読み取り専用で参照し、互換ハーネスも`advance_frame`経由で実行した。SocialSkin `lua/play_7.luaskin`は3条件すべてでdecode成功・callback失敗0・warning 0。ModernChic471はSelect / Play 4種 / Result / Course / SkinSelectが3条件すべてでdecode成功・callback失敗0・warning 0。
- ModernChic471のDecideは譜面情報のない条件だけRGB不足で失敗し、Compat / scene条件は成功した。KeyConfigは3条件とも`isOutlineFont`不足で失敗した。どちらも[既存の調査](2026-11-01-modernchic-compatibility.md)に記録されている今回のIssueとは別の制限であり、実スキン全体の完全互換を確認したものではない。
- 実スキンのsnapshotは`.local/issue57-social.json` / `.local/issue57-modernchic.json`、全体検証は`.local/issue57-{check,clippy,test}.log`に保存した（Git管理外）。
