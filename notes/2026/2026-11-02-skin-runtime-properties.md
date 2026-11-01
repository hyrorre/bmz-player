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

## 2026-11-02 PR 60追補：譜面長の互換修正

### 参照した仕様

beatoraja `83574048ba489082eb46df7fe9806b464bf1f526`、jbms-parser
`8108cd54a19275810e1f5d2c9586f008eb6db95e`を基準にした。

- [IntegerPropertyFactory.java L473–478](https://github.com/exch-bms2/beatoraja/blob/83574048ba489082eb46df7fe9806b464bf1f526/src/bms/player/beatoraja/skin/property/IntegerPropertyFactory.java#L473-L478): 1163は`(SongData.length / 60000) % 60`、1164は`(length / 1000) % 60`。SongData不在は`Integer.MIN_VALUE`。
- [SongData.java L141–210](https://github.com/exch-bms2/beatoraja/blob/83574048ba489082eb46df7fe9806b464bf1f526/src/bms/player/beatoraja/song/SongData.java#L141-L210): `setBMSModel`で`model.getLastTime()`を保存する。
- [PlayerResource.java L156–175](https://github.com/exch-bms2/beatoraja/blob/83574048ba489082eb46df7fe9806b464bf1f526/src/bms/player/beatoraja/PlayerResource.java#L156-L175)、[L204–218](https://github.com/exch-bms2/beatoraja/blob/83574048ba489082eb46df7fe9806b464bf1f526/src/bms/player/beatoraja/PlayerResource.java#L204-L218): モデルのロード中に先頭ノーツを1000ms以降へ移し、更新したモデルをSongDataへ渡す。
- [BMSModelUtils.java](https://github.com/exch-bms2/jbms-parser/blob/8108cd54a19275810e1f5d2c9586f008eb6db95e/src/bms/model/BMSModelUtils.java#L176-L205): `setStartNoteTime`は先頭ノーツが1000ms未満のときだけ全timelineを移動する。
- [BMSModel.java](https://github.com/exch-bms2/jbms-parser/blob/8108cd54a19275810e1f5d2c9586f008eb6db95e/src/bms/model/BMSModel.java#L281-L301): `getLastTime()`は可視/不可視ノーツ、BGM、通常/Layer BGAの最終配置。Poorのみ、タイミングイベントのみ、音源の再生終了は含まない。
- [BMSPlayer.java L184–199](https://github.com/exch-bms2/beatoraja/blob/83574048ba489082eb46df7fe9806b464bf1f526/src/bms/player/beatoraja/play/BMSPlayer.java#L184-L199)、[L366–368](https://github.com/exch-bms2/beatoraja/blob/83574048ba489082eb46df7fe9806b464bf1f526/src/bms/player/beatoraja/play/BMSPlayer.java#L366-L368): Replay/Retryの分岐を再ロードして余白を適用し、コンストラクタ末尾でSongDataを再設定する。終了判定の`playtime + TIME_MARGIN`とは別。
- [BMSPlayer.java L525–570](https://github.com/exch-bms2/beatoraja/blob/83574048ba489082eb46df7fe9806b464bf1f526/src/bms/player/beatoraja/play/BMSPlayer.java#L525-L570)、[PracticeModifier.java](https://github.com/exch-bms2/beatoraja/blob/83574048ba489082eb46df7fe9806b464bf1f526/src/bms/player/beatoraja/pattern/PracticeModifier.java#L36-L51): Practice再開時もモデル再ロード後にSongDataを設定する。Practiceの速度変更・区間変更の後は再設定しない。PracticeModifier自体は範囲外の可視ノーツをBGMへ移し、timelineを切り詰めない。

### 原因と修正

初期実装のPlayロードはDBの`ChartListItem.length_ms`、描画は`RenderSnapshot.duration`、
Offlineは`session.chart.end_time`だった。DBには実分岐/余白がなく、Practice後の`end_time`は区間終了へ変わる。
さらにBMZの`end_time`はノーツがある譜面では末尾BGM/BGAを含まない。分側には`% 60`もなかった。

- 既存の終了待ち用の「最終配置」計算を共有し、`beatoraja_chart_last_time`ではbeatoraja未対応のBGA Layer2（0A）を除外する。終了待ちは従来どおりLayer2も含め、待機時間そのものは曲長計算に含めない。
- 変換済み譜面から`PlayRenderSnapshotCache.skin_chart_length_ms`を確定し、Practiceのcache再生成時は全体長を引き継ぐ。`RenderSnapshot.skin_chart_length_ms`を通して通常描画と動画用描画状態へ渡す。
- 既存の`PreparedPlaySession.chart_length_ms`はIR照合用のDB長のまま。`RenderSnapshot.duration`、161/162、163/164、ノーツ表示時間、Practiceの区間/既存余白は置き換えない。
- 分秒変換を共通化した。非負のi64を整数除算してから両方に`% 60`を適用し、乗算やi32への早期変換によるoverflowを避ける。欠損はLuaにINT_MIN、数値画像/numberRefに非表示として渡す。
- MD5/SHA256供給とLuaフレーム割り当て処理（#58/#57）は変更しない。

### 非同期処理の順序

1. direct開始も既存の非同期Play preload経路へ統一する。Decide、通常開始、Retry、Replay、コース次曲は同じ通知を使う。
2. preload generationごとに独立した`PlaySkinChartState`を作る。Playスキンは先に依頼/worker起動できるが、デコード本体はこの通知を待つ。
3. `PreparedPlayChart`の`on_chart`で値を一度だけ公開する。その後、譜面workerはWAV読込へ進み、スキンworkerは正確な値をロード状態へ入れてLuaを評価する。BMSの同期再解析や、WAV/BGA完了待ちは加えない。
4. スキンのロード依存/文書cacheのキーは、通知で確定した値を入れてから計算する。ロードの署名は通知到着前後で変えず、preload generationを含む。同じ署名の進行中decodeも再起動しない。
5. 取消/置換/失敗（譜面workerのpanicを含む）では待機を解除する。終了時も待機を解除してからskin workerをjoinする。Play preloadとskin decodeの既存generationを更新し、古い結果を破棄する。同じ配置のRetryは既存cacheを即時通知し、再解析しない。
6. Play入場時の既存のskin待機中にも変換済み譜面を一度公開し、BGAロードを開始できる。待機時間はdecodeのCPU処理時間から分離する。

性能上の変更は、譜面変換とPlay Luaデコードが直列になり、変換後のWAV読込とスキンデコードが並行になる点。
Luaが分岐後の値からロード時定数を作れることを優先した。再解析回数は増えない。
WAV進捗を意図的に止めた回帰テストでskin decodeが完了することを確認する。実測のFPS/起動時間の改善は主張しない。

### 他画面の取得元と残る差異

| 画面 | BMZの既存の取得元（今回維持） | beatorajaとの差異 |
|---|---|---|
| Select | 選択行のDB長。非譜面行は非表示 | DBの最終配置の定義は今回変更していない |
| Decide | snapshotのDB由来duration（描画時） | beatorajaのPlayerResourceはDecide前に余白適用済みモデルをSongDataへ設定する。BMZのDecideロード時に1163/1164を明示供給する対応も今回追加しない |
| Result | ResultSummaryのduration。未設定時は既存fallback | Playで確定した全体長の保存/ロード時供給への統一は別対応。beatorajaは同じSongDataを参照する |
| Course Result | 最終ステージのsummary.duration | beatorajaのSongDataも最終譜面だが、BMZの最終配置/ロード時供給には上記Resultと同じ差異が残る |

この表は1163/1164に限定する。値の取得元は維持し、60分で分表示が循環することと欠損時のLua sentinelは共通の規則に合わせる。
極端に長い譜面ではi64で安全に計算し、Javaの`getLastTime()`のint castによるoverflowは再現しない（今回の要求による仕様差）。

### 回帰テストと確認範囲

- `plan/tests/cases_01.rs`: 59,999 / 60,000 / 183,456 / 3,600,000ms、i64::MAX、時刻変更、表示時間との独立、Play欠損、161/162/163/164の既存計算（残り時間の既存1秒余白を含む）。
- `app/tests/skin_more.rs`: ロード時の名前参照、境界・60分・負値・欠損、依存捕捉。
- `app/tests/play_chart_length.rs`: 実BMSを登録し、DB180秒に対する余白後181秒、DBの分岐1=180秒に対するPlayの分岐2=241秒、余白のない次曲120秒を確認する。旧DB値/終了時刻を使う実装ではこの期待値にならない。
- 通常/Decideで共有する実preload worker、同配置Retryのcache、分岐固定Replay、次曲generation、Offline、Practice短縮と速度150%を通す。ロード時の数値ID/名前の定数、永続Lua callback、数値画像、numberRefテキストを同じ描画値と比較し、依存値も確認する。
- 音源進捗を止めてもskin decodeが完了すること、取消/失敗の待機解除、遅延した旧通知で値を置き換えられないこと、末尾BGM/BGA（Poor/Layer2除外）と演奏終了時刻の分離、終了待ちでのLayer2保持を確認する。
- 経路の回帰はCPUの譜面/skin worker・snapshot・描画評価で実行する。実機操作、GPUの見た目、macOS/Linuxは未確認。
- Windows offscreenのappテストを単独実行し、実際のskin decode/upload/install、進行中依頼の再利用、世代切り替え、退避済みsessionからのロードが成功した。譜面プロパティ待ち中のworker終了もCPUテストで確認した。
- Windows通常環境で`cargo fmt --check`、`cargo check --workspace --all-targets --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo test --workspace --locked --no-fail-fast`成功。bmz-player 2798 passed / 51 ignored、bmz-render 987 passed / 11 ignored、bmz-skin 373 passed / 1 ignored。追加のoffscreen appテストは別実行で1 passed。
- 先行した制限環境での並列実行は一度exit 3で中断し、gilrsの別thread panicも出力された。別の実行ではローカルHTTP通信のテストが失敗/停止したため、自分で起動したテストプロセスだけを停止した。通常環境での最終実行では通信テストを含め全件成功した。exit 3の直接原因は未特定。
- 生ログは`.local/issue59-complete-{fmt,check,clippy,test,gpu}.log`（Git管理外）。回帰fixtureは外部スキン不足によるskipなしで実行した。workspaceの他の互換テストには未配置の外部スキンによるskipが残る。

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
