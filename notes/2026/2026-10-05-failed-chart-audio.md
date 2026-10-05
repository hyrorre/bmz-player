# 途中FAILED時の譜面音声停止（issue #26）

実施日: 2026-10-05。対象: [issue #26](https://github.com/hyrorre/bmz-player/issues/26)。
開始時のmain / origin/main: `3ddd2545`（fetchで一致を確認、作業ツリーに既存差分なし）。
現在の契約は[Gameplay runtime](../../docs/gameplay-runtime.md#途中failed時の譜面音声)、
操作は[controls](../../docs/controls.md)を参照。

## 原因と調査

- `app/play_loop_flow.rs::stop_play_like_escape()` は開始前・Viewer・最終ノーツ処理済みを
  分岐した後、通常の途中終了をworkerへの `state = Failed` で要求する。
  E1+E2長押しは `stop_play_if_exit_hold_elapsed()` から同じ経路へ入る。
- ゲージ枯渇は `session/judgement.rs::update_failed_state_from_gauge()` が `Failed` にする。
  GASはゲージ側の既存条件で続行する。`session/frame.rs` はゲージ判定前にBGM・自動キー音を
  先行予約し、FAILEDになったフレームにも発音要求・音量変更が残り得る。
- `gameplay_runtime.rs` は新しいFAILEDでPlayStopを送るだけだった。`GameplayRuntime` は
  発音とHCN音量変更の送信失敗を保留し、後続wakeで再送していた。
- `result_flow/ending.rs::finish_play_ending()` はFAILEDも通常完走も `mark_draining()` へ渡す。
  これはsource種別の変更で、音声を停止しない。長いBGMだけでなくキー音や予約も残る。
- 参照コピーのbeatorajaとLR2で、手動途中FAILEDとGAS無効時のゲージFAILEDに
  `getAudioProcessor().stop((Note) null)` があることをコード確認した。
  LR2には判定成立前の `STATE_ABORTED` 分岐もあり、全終了処理を共通仕様とは扱わない。
  今回は「途中FAILED時に譜面音声を止める」点を採用し、BMZの終了理由は変更しない。
  比較実装の実機動作は今回再確認していない。

## 実装判断

`bmz-gameplay::runtime::GameplayRuntime::flush_audio()` で `PlayState::Failed` を判定し、
保留キューを消去してsourceの停止を要求する。手動とゲージで分岐を重複させず、
最終ランプ判定やRESULT遷移には置かない。`advance()` は時計停止の早期return前にも
FAILEDを処理し、PRACTICEの中断＋pauseを扱う。PlayStopの有無には依存しない。

`bmz-audio::command` にsource単位の解除しないatomic停止フラグを追加した。
`ClearPlayback` を有界キューへ一度送るだけでは、満杯時に停止が失われるため採用しない。
既存のworker取消フラグは遅着要求の取消として残し、停止フラグは別に保持する。
これによりworker終了で停止要求自体が無効にならない。

callbackはキューから取得したコマンドにも停止判定を行い、音声出力前に既存の
`clear_playback()` でvoiceと予約を消去する。キューロック競合時もこの消去を行う。
engineのロック競合は既存どおり無音返却とし、後続callbackで再試行できる状態を維持する。
callbackにロック待ち・I/O・ビジーループ・sample bankの破棄を追加していない。
gainリセットを打ち消し、pause / clock / rateを維持する。

再プレイのengine / handleは既存どおり別sourceで、古い停止や発音は新sourceへ作用しない。
デコード済みsample bankを保持するのでPractice等のPCM共有も維持できる。
通常のquick retryは既存の音源再ロード経路を変更していない。
停止sourceをdrainingへ渡しても再発音できず、通常完走のdrainingとRESULT退出フェードは維持する。

## 終了経路への影響

| 経路 | 音声の扱い |
|---|---|
| 通常の途中Esc / E1+E2、ゲージ枯渇 | FAILED確定時に譜面音声停止 |
| 通常完走・完走時FAILEDランプ | 既存の余韻継続 |
| 最終ノーツ後の退出操作 | 既存のFinished要求と退出演出を維持 |
| GAS続行 | 停止なし |
| REPLAY / AUTOPLAY | 既存のFailedなら停止、Finishedなら余韻継続。モード固有の終了判定は変更なし |
| PRACTICE | Failed＋時計停止でも保留要求を破棄。設定画面復帰・資源再利用は維持 |
| Viewer | 終了・待機・pause・seekをFailedに変更しない。稼働中sourceの差し替えを維持 |
| コース | ステージの途中Failedは停止。コース継続判定や保存処理は変更なし |
| PlayStop / RESULT BGM・SE / スキン音声 | 別system sourceなので通常どおり再生 |

## 自動検証

Windows上でデバイス不要の合成PCMを使用。検証ログはGit管理外の `.local/issue-26-*.log`。

- `bmz-audio` の追加2テスト: 45秒sampleの再生中・未来予約、満杯キュー、queue / engine
  ロック競合、worker取消、停止重複、遅着要求拒否、gain / pause / rate維持、sample bank再利用。
- `bmz-player::gameplay_runtime::tests::failure_audio` の追加8テスト: 長いBGMと複数キー音を
  実際に鳴らした後の手動FAILED worker command、描画publicationロック中・system出力なしの停止、
  HARD / EXHARD / HAZARD / CLASSの見逃しからのFAIL、FAILフレームの先行BGM、
  満杯キューに残る発音・HCN音量変更、PRACTICE相当の時計停止、直後の再プレイ、
  通常完走と完走時FAILEDランプ、全ノーツ後Finished要求、GAS続行、REPLAYのゲージFAIL、
  AUTOPLAY完走、Viewer pause / seek、実workerからのPlayStopと後続RESULT / スキン音声。
- 最初の完走テストは既存の5秒終了marginより短い時刻を設定して失敗した。
  通常完走は7秒、最終ノーツ後の早期退出は2秒として区別し、追加10テストは全件成功。
- `cargo fmt --check`: 成功。
- `cargo check --workspace --locked`: 成功。
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: 成功。
- `cargo test --workspace --locked --no-fail-fast`: 全ターゲット成功。
  主な対象はbmz-audio 127成功 / 3 ignored、bmz-gameplay 235成功、
  bmz-player 2,122成功 / 6 ignored。今回の追加10テストは外部素材不要で全件実行した。
  workspaceの素材依存テストの成功件数を、外部スキン全般の実機互換性確認とは扱わない。
- `git diff --check`、変更文書のローカルファイルリンク: 成功。
- `cargo build -p bmz-player --locked`: Windows debug build成功。

## 実機確認の範囲と手順

音声デバイスでの聴取・実画面の入力操作は未実施。自動テストは音声callbackが使用する
`CommandedAudioEngine::render_stereo()` の出力を検証し、物理デバイスのバッファやドライバは検証しない。
Windows WASAPI / ASIO、macOS Core Audio、Linux各backendの聴取確認は未実施。

1. 約45秒のBGM sampleがある譜面を通常プレイし、キー音も鳴っている途中でEscとE1+E2をそれぞれ実行。
2. 閉店開始時にBGM・キー音が止まり、PlayStopが鳴ることを確認。RESULTで長尺sampleの残音がなく、
   RESULT BGM / SEと対応スキン音が鳴ることを確認する。
3. GAS無効のHARD / EXHARD / HAZARDでゲージを枯渇させ、同じ結果を確認する。
4. FAIL直後のquick retryとRESULTからのretryで、旧音が残らず新しいBGM・キー音が鳴ることを確認する。
5. 通常完走、NORMALで低ゲージのまま完走、最終ノーツ後の退出、GAS続行で音が不意に切れないことを確認する。
6. 実施時はOS・出力backend・buffer設定・譜面／スキンと結果をこの記録へ追記する。
