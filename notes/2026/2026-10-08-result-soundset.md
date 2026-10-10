# RESULT BGMとサウンドセット（Discussion #29）

## 要件と実装

既存のBGMフォルダーへRESULT音源を追加する連動方式だけを実装する。
独立抽選、profileの新モード、フォルダーの移行、音源用設定ファイルは追加しない。
仕様は[サウンドセット](../../docs/system-sound.md)、操作案内は[controls](../../docs/controls.md)にまとめた。

- サウンドセット内に`clear` / `fail` / `a` / `aa` / `aaa`を追加できる。
- 補完用SEセット・既定音源にも`a` / `aa` / `aaa`を追加できる。
  同ランクBGM、汎用clear BGM、同ランクSE、clear SEの順で選ぶ。FAILEDはfail BGM、fail SEの順。
- Result表示の既存rank optionを使い、FAILED優先、ランク完全一致、汎用BGM、従来SEの順で選ぶ。
- 新しいRESULT音種を既存SoundIdの後ろへ追加し、既存SEの分類とIDを維持する。
- 新RESULT BGMは`.loop`付きならループ、通常名なら単発。再生方式を準備結果からmanagerへ渡す。
- SEをサウンドセット、従来SEセット、既定音源の順で補完し、破損した候補は次へ進める。
  RESULT入口音は配置先で区別し、サウンドセット内ではBGM、SEセット・既定音源では単発SEとして扱う。
- Selectを離れた後は適用済みセットを保持する。後着のセット準備結果による切り替えを防ぐ。
- Result終了フェード、画面遷移、コースRESULTへの移行で新BGMを停止する。
- 設定UIと6言語の説明を更新し、保存キーと既存のフォルダー構成を維持する。
- Playの動画出力にもSE補完を適用し、使用しないRESULT BGMはロードしない。

## 回帰テスト

- 既存21種類と追加3種類のSE / RESULT BGM上書きとSE / defaultへの補完。
- 拡張子の大小文字、Windowsの従来のファイル名大小文字扱い、`.loop`版優先、RESULT BGMの探索範囲、SEのループ禁止。
- 壊れた上書きと`.loop`音源の補完、解析対象がBGMだけであること。
- 単発 / ループの実PCM出力、停止・フェード、BGM停止でSEが止まらないこと。
- BGM正規化と音量の実行中変更。
- ResultのA / AA / AAA境界、FAILED、ノーツ数0、ランク欠落時に下位ランクへ落とさないこと。
- ランク別SEの選択、汎用BGMの優先、FAILED時のランクSE抑制。
- 後着セットの適用条件と、全画面でのRESULT BGM停止対象。

## 検証

2026-10-09（JST）、Windowsで以下を確認した。

- `cargo fmt --check`: 成功。
- `cargo check -p bmz-player --locked`: 成功。
- `cargo clippy -p bmz-player --all-targets --locked -- -D warnings`: 成功。
- `cargo test -p bmz-player --locked --no-fail-fast`: 2,316件成功、20件除外、失敗なし。
- `cargo test -p bmz-audio -p bmz-ffmpeg --locked`: bmz-audio 128件成功、3件除外、失敗なし。
  bmz-ffmpegと両crateのdoc testはテスト0件で成功。
- `cargo build -p bmz-player --locked`: 成功。
- DX12・サンプル譜面の自動プレイで、RESULT BGM付きセットとランク別SE補完セットをそれぞれ起動。
  音声出力開始、セット適用、Resultの120フレーム描画、正常終了を両方で確認した。

最初のsandbox内の全体テストでは、保存・ダウンロード・ディレクトリリンクなどの失敗と
ダウンロードテストの待ちが発生した。対象テストプロセスだけを停止し、通常権限で再実行して全件成功を確認した。
incremental cacheのhard linkを作れずコピーへ切り替える環境由来のwarningは残る。

起動確認では既存データに触れず、`.local/validation/2026-10-08-result-soundset/runtime-bgm`と
`runtime-se`に専用profile・DB・検証音源を作った。スクリプトは同ディレクトリの`run-smoke.ps1`、
実行ログはそれぞれ`logs/bmz-player.2026-10-08.log`、最終テストログは`.local/result-soundset-test-final.log`。
起動確認のmaster音量は0とし、手動聴取は行っていない。単発・ループ・停止・フェードは実PCMの回帰テストで確認した。
macOS / Linuxでの実機確認と、実音源を使った手動の聴取・リトライ・コース遷移確認は未実施。

## 2026-10-09 レビュー指摘の修正

対象は`22a1047d`のレビューで判明した2件。ユーザー指定により、正規化とファイル探索を
それぞれ`gpt-6-luna`のサブエージェントで修正し、親側で統合レビューと検証を行った。

- ファイル名はOSやファイルシステムに依存せずASCIIの大小文字を区別しない。
  `select` / `clear`によるセット検出、通常音源、`.loop`、拡張子に同じ規則を使う。
  大小文字違いが共存する場合は同拡張子内で正式な小文字名、残りはファイル名順とする。
  拡張子とループ版の優先順位は維持する。
- 正規化OFFで読み込んだセットをPlay / Result中にONへ変更した場合、
  セットの抽選・差し替えとは別に、現在のBGMの解析結果を反映する。
  SEの再読み込みやBGMの停止・再開を行わず、解析済みゲインだけを更新する。

追加解析は通常セットのロードと別のpending・世代で管理する。セット適用とprofile切替で古い解析を無効化し、
ロード時の実パス・ファイルサイズ・更新時刻と一致しない結果も破棄する。
音量反映は既存の設定同期を使い、Selectプレビュー中のフェードを維持する。

検証（Windows）:

- `cargo fmt --check`、対象crateの`check` / all-targets Clippy（`-D warnings`）/ `build`: 成功。
- `cargo test -p bmz-player --locked --no-fail-fast`: 2,319件成功、21件除外、失敗なし。
- `system_sound_normalization_poll_preserves_set_load_and_rejects_stale_results`:
  Winitのイベントループを使うためWindows限定・通常suiteでは除外し、`--ignored`指定の単独実行で成功。
  Play / Result中の解析適用、通常セットpending保持、古い世代とmanager不在の結果破棄を確認した。
- PCM回帰でゲイン更新後も次のサンプル位置から再生が継続することを確認した。
- 全体テスト後に追加した`disabled_prepare_can_analyze_only_loaded_bgms_and_reject_changed_sources`も
  単独実行で成功。実音源をOFFでロードしてからBGMだけを解析・適用し、SEの除外とファイル差替時の拒否を確認した。
- `cargo test -p bmz-audio -p bmz-ffmpeg --locked`: bmz-audio 128件成功、3件除外。
  bmz-ffmpegとdoc testも成功。
- 専用profile・DBへ`SELECT.WAV` / `Clear.LOOP.Wav`等を配置したWindows/DX12の起動確認が成功。
  音源のロード、音声出力開始、Resultの120フレーム描画と正常終了を確認した。master音量は0。

全テストのログは`.local/validation/2026-10-09-result-soundset-review/test.log`、
起動確認は同ディレクトリの`run-smoke.ps1`と`runtime/logs/bmz-player.2026-10-09.log`に保存した。
手動聴取とmacOS / Linuxの実機確認は未実施。

## 2026-10-09 共通音源と子フォルダのバリエーション

`0a32da63`を基点に、ユーザー指定の`gpt-6-luna`で実装し、別のLunaによる読み取りレビューと
親側の統合確認を行った。専用の`variants`フォルダを設けず、セット直下の任意名フォルダを使う。

- `bgm_dir`から整理用フォルダを再帰走査し、対応するBGM / SEを直接含む最初のフォルダを
  セットのルートとして確定する。`select`は必須ではなく、共通SEだけの親も認識する。
- ルート直下で対応音源を持つ子をバリエーション候補にする。孫以降は探索しない。
  非対応名の音声ファイルや、音源名と同じディレクトリは認識対象外とする。
- セット、次にその子を均等に抽選する。子の数はセット自体の当選確率に影響しない。
  候補一覧は起動・profile切替時に保持し、Select復帰時はキャッシュから再抽選する。
- 同名音源は子、親、従来の補完先の順で試す。欠落・デコード失敗時は次の候補に進み、
  別の子から補完しない。子の通常版は親の`.loop`版より優先する。
- Resultのランク優先は維持する。AAAなら子の`aaa`、親の`aaa`、子の`clear`、親の`clear`、
  ランクSE、clear SEの順とする。親子の音源も配置先に従ってBGM / SEとして扱う。
- 親子の組み合わせはDecide / Play / ResultとSelectを経由しないリトライで保持する。
  正規化には各音源が実際にロードされた子・親のパスを記録する。
- 補完用SEセットは従来の`clear`マーカーで検出する。
  動画出力は同じ候補構造から名前順で最初のセット・子を選ぶ。

以前は親子それぞれの`select`を独立したセットとして検出していたが、現在は1セットとその子になる。
独立したセットは、音源を含まない整理用フォルダの下へ兄弟として置く。
配置例と優先順位を[サウンドセット仕様](../../docs/system-sound.md)に反映した。

検証（Windows）:

- `cargo fmt --check`、`cargo check -p bmz-player --locked`、
  `cargo clippy -p bmz-player --all-targets --locked -- -D warnings`、
  `cargo build -p bmz-player --locked`: 成功。
- `cargo test -p bmz-player --locked --no-fail-fast`: 2,324件成功、21件除外、失敗なし。
  SEのみのルート、Resultのみの子、孫の除外、二段抽選、候補の優先順、
  子のデコード失敗時の親への補完、正規化パス、リンクの循環・別名重複を確認した。
- `cargo test -p bmz-audio -p bmz-ffmpeg --locked`: bmz-audio 128件成功、3件除外。
  bmz-ffmpegとdoc testも成功。
- 共通SEだけの親と、Select / Result音源を持つ`style-a` / `style-b`を専用データに配置し、
  Windows/DX12でサンプル譜面を自動プレイした。親と`style-b`の選択、10音源のロード、
  音声出力開始、Resultの120フレーム描画、正常終了を確認した。

全体テストは保存・ローカル通信・ディレクトリリンクを扱える通常権限で実行した。
incremental cacheのhard link失敗に伴うコピーへの切り替えwarningは環境由来として残る。
ログと起動確認スクリプトは`.local/validation/2026-10-09-soundset-variants/`に保存した
（ローカルのみ、Git管理外）。既存データは変更せず、専用profileとDBを使用した。
起動確認はmaster音量0で、手動聴取、macOS / Linuxの実機確認は未実施。

## 2026-10-09 Result退出時のオプションパネルSE重複

E1を押したままResultを退出すると、`leave_result()`内で物理押下状態をSelectへ同期する際に
オプションパネルも即時更新され、`OptionOpen`が再生されていた。その後、Selectシーン開始処理が
パネル状態を閉じ、最初の通常更新で再度開くため、同じSEがもう一度再生される。

Result退出時は押下状態とEアクション保持だけを再構築し、パネル更新を行わないようにした。
Selectのシーン開始処理後、通常の更新経路がパネルを一度だけ開く。通常の入力同期は従来どおり
押下状態の再構築後にパネルを更新する。

検証（Windows）:

- E1を保持したままの状態再構築でOptionOpen遷移が起きず、Selectの最初の通常更新で一度だけ
  開く回帰テストが成功。
- `cargo fmt --all --check`、`cargo check -p bmz-player --locked`、
  `cargo clippy -p bmz-player --all-targets --locked -- -D warnings`が成功。
- `cargo test -p bmz-player --locked --no-fail-fast`: 2,325件成功、21件除外、失敗なし。
- 実機での手動操作・聴取は未実施。

## 2026-10-10 レビュー指摘: キャッシュ競合と長尺RESULT BGM

正規化キャッシュは、通常セット準備と正規化専用workerがそれぞれ古い全体snapshotを保存すると、
後発workerが先発workerの追加entryを消せる状態だった。保存時にmutex下で最新ファイルを再読込し、
今回のworkerが解析したpathだけをマージするよう変更した。2 workerが同じ空snapshotを読んでから
並行保存するbarrier付き回帰テストを追加し、両entryが残ることを確認した。

長尺RESULT BGMはWindows debug testで、合成した48 kHz stereo PCM16 WAVを5本・各60秒用意して計測した。
正規化を無効にした状態で初回prepareのdecodeは1,143 ms、同じセットの再prepareは1,157 ms、
同一source再利用時は0 msだった。f32 PCM保持量は1セット109.86 MiB、旧セットを保持したまま
同じ5本を再ロードすると219.73 MiBだった。3分音源ならそれぞれ約329.59 MiB、約659.18 MiBになる
単純比例の見積りである。これはDecodedSampleのPCM領域量で、OSが報告するプロセスpeak RSSではない。

Select復帰時、前回と選択root/variant/SE set、候補ファイル一覧、各候補のsize・mtime・loop指定、
出力sample rateが一致する場合は、workerが既存セットを再利用して再デコードしない。
ファイルの追加・削除・変更や別セット選択時は通常どおり5種のRESULT BGMをロードするため、
別セットへの切替時に長尺5本を同時保持するメモリ上限は残る。これを解消するにはRESULT入口時の
遅延decodeまたはstreaming化が必要で、BGM/SE fallbackとRESULT開始タイミングを変える設計になるため、
今回は既存挙動を保つ範囲で同一セット再利用までを実装した。
