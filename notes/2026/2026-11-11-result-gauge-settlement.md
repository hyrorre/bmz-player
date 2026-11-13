# プレー終了時とResultのゲージ値不一致

実施日: 2026-11-11。対象: `main`（`78bfc6c`）を起点にした修正。調査時点の報告コミットは`65af7ed`です。

## 報告

- プレー終了時のEASYゲージが79だったのに、Resultでは93と表示された。クリアランプはFAILED。
- 残り時間0:00の後のゲージ減少がResultに反映されていないように見える。

## 原因

1. Resultのゲージ履歴は500ms周期で記録していた。結果の先行確定時（`result_is_settled`）に
   実際のゲージ値と、その時点の履歴を複製していたが、確定時の値を履歴へ追加していなかった。
   最後の定期サンプル以降の増減（最終ノーツの見逃し・空POOR・地雷等）が欠落していた。
2. LR2 / beatoraja JSON / LuaのResultは共通処理（`result_display_gauge`）で選択ゲージの
   履歴末尾を表示値として使う。そのため数値もグラフも欠落した末尾を示した。
   クリアランプは確定時の実ゲージで判定するため、FAILEDと93%が同時に表示された。
3. 先行確定後もPlaying中は採点が続き、`Finished`遷移でworkerが結果を取り直していた。
   保存済みの結果と、後から取得した結果が食い違う経路があった。

## 確定条件の検証（`result_is_settled`）

旧条件は「全ノーツ処理済み」かつ「`audio_now > E + W`」でした（`E = chart.end_time`、
`W = window_set.result_settle_margin_us()`）。現在の判定処理に対して次の不足を確認しました。

- **人間入力の負オフセット**: 入力の判定時刻は取得時刻に`input_offset_us`を加えた値です
  （`input/translator.rs`の`estimate_audio_time`）。O<0では取得時刻が`E + W`を過ぎた入力も
  判定窓内に入ります。例: DX 7KEY・等速・O=-20msで、最終ノーツから+210msの押鍵は判定上+190msの空POORです。
  旧条件はこの入力を取得する前の更新（+205ms等）で確定し得ました。
- **フルリプレイの処理位置**: フルリプレイは1ms tickで採点しますが、旧条件は外側の`audio_now`で
  判定していました。`E=1100.5ms`、`audio_now=1300.6ms`では処理済みtickが1300msで、記録時刻1300.4msの
  空POOR入力は1301msのtickで処理されます。旧条件はその前に確定し得ました。また`next_scoring_time`は
  tickの一覧を作る時点で先へ進むため、FAILEDで中断したbatchでも処理済みとして扱われていました。
- **確定後の再取得・再分類**: 確定後の`Finished`遷移で結果を取り直していました。
  手動中断はUIの観測値で可否を判断し、所有スレッドではそのまま`Failed`にしていたため、
  確定後に届いた中断要求で完走結果がFAILEDへ再分類され得ました。
- **独立した対戦相手**: G-BATTLE等の相手の判定は確定条件に含まれていませんでした。

採用した条件は次のとおりです（詳細は[gameplay-runtime.md](../../docs/gameplay-runtime.md#採点の確定とresultのゲージ履歴)）。

- 人間入力が採点対象に残る場合: `audio_now > E + W + max(0, -O)`。
- フルリプレイ: 実際に処理を終えたtick（`ReplayPlayer::scored_until`）が`E + W`を超える。
- フルオート: `audio_now > E + W`。
- いずれも`judge.is_exhausted()`と、独立した対戦相手の同条件を要求する。

十分条件である理由:

- 自然確定後に取得する入力は取得時刻が`audio_now`以上なので、判定時刻は`audio_now + O > E + W`になる。
  O≥0では`audio_now > E + W`で足りる。
- `E`は地雷・不可視ノーツを含む全レーンのノーツとLN終端の最大時刻。最終ノーツ後の入力が影響するのは
  BAD / 空POOR（`bad_slow_us`、`empty_poor_slow_us`）と地雷（`mine_hit_us`）で、`W`はその最大値。
  見逃しPOORは`now > note.time + bad_slow_us`で確定するため、`is_exhausted()`が真なら処理済み。
- LN/CN/HCNは保持中・押し直し猶予中も`active_long`が残るため`is_exhausted()`が偽。HCNのゲージ増減は
  終端時刻までの区間だけ。
- 空POORは`delta <= empty_poor_slow_us`を受け付けるため、境界上の入力に備えて比較は厳密な`>`。
- `W`は`sync_judge_windows`がルール・キー種別・ランク・再生速度から作る実効窓。再生速度の換算は
  判定窓にだけ掛かり、`input_offset_us`は譜面時刻へ直接加えるため二重には適用されない。
  リプレイは記録済みの譜面時刻を使うため窓も換算しない。

残る前提・制約（今回は変更していません）:

- 取得時刻が`audio_now`以前でも、OSから遅れて届いた入力は確定後なら採点しません。
  任意の遅配を待つ固定待機は追加していません。
- `#EXRANK`等の判定ランク変更や再生速度変更が`E`より後にある場合、確定時点の窓を使います。
  再生速度を変えられるのはPractice / Viewerで、どちらもスコアを保存しません。

## 修正

- `GameSession.scoring_closed_at`を追加し、`advance_session_frame`がその更新の入力batchを処理した後に
  採点を閉じる。途中FAILED・手動中断・明示的な退出は停止境界で閉じる。
- 確定後の入力は判定・リプレイ記録・ゲージへ渡さず、キービーム・skin入力イベント・fallbackキー音だけを扱う。
- `ReplayPlayer`は処理を終えたtickだけを`complete_scoring_tick`で進め、`scored_until`を返す。
- gameplay workerは確定時に一度だけ結果を取得し、同じ状態から全ゲージの終端点を履歴へ記録する。
  `Finished`遷移では採点結果を再取得せず、退出時のプレー時間だけ更新する。
- `GameSession::request_manual_stop` / `request_finish`で、終了要求の可否を所有スレッドで再確認する。
  Practiceは`request_practice_round_end`を使い、拒否された中断を明示的な退出として終える
  （PRレビューの指摘。拒否したまま時計を止めると`Playing`に残り、ラウンドの結果を取得できなかった）。
- BSSの逆方向Press抑止を`JudgeEngine::consume_scratch_press_suppression`へ切り出し、確定後の
  入力経路でも消費する（PRレビューの指摘。確定後に余分なfallbackキー音が鳴っていた）。
- `ResultGraphCollector::record_terminal_gauges`は同時刻の末尾点を更新し、時刻順を保つ。
  途中FAILEDの0埋めは終端点より後の定期サンプル位置から始める。
- `GameSession`を直接使う保存経路（同期・非同期）も、同じ確定時刻と終端点を使う。

## 検証

自動テスト（追加分）:

- `bmz-gameplay` `session::tests::result_settlement`: DX 7KEY・O=-20msの+210ms押鍵（判定+190ms空POOR）、
  `E=1100.5ms`のフルリプレイで1300ms tick時点では確定しないこと、確定後の古い入力・オフセット変更・
  手動中断要求・`Finished`遷移で結果が変わらないこと、キー表示・キー音が続くこと、
  停止境界での確定、フルオート / 部分オートの待機、完走FAILEDと途中FAILEDの区別、
  BSS終端のRelease(E+190ms)→確定(E+201ms)→逆方向Press(E+205ms)が無音であること、
  全ノーツ処理後のPractice中断が完走として終わること。
- `bmz-gameplay` `replay::tests`: tick一覧の作成では処理済みにならないこと。
- `bmz-player` `gameplay_runtime::tests::result_settlement`: 実際のgameplay worker・保存経路で、
  1000msサンプル後の空POORによる低下が全ゲージの終端点に入り、確定値と一致すること。
  確定後の入力・オフセット変更・5秒待機後の`Finished`で採点結果と履歴が変わらないこと。
  最終ノーツ処理直後のPractice中断で時計を止めても、ラウンドの結果が公開されること。
- `bmz-player` `screens::play_finish::tests`: `GameSession`を直接使う同期・非同期保存の終端点。
- `bmz-player` `screens::result_model::tests`: 93→79.5、逆方向の回復、同時刻の更新、0埋めの時刻順。
- `bmz-player` `app::result_runtime::tests`: コース連結後の時刻順と曲境界マーカー。
- `bmz-render` `skin::tests::result::lr2`: 選択ゲージ別の表示値、LR2の2%刻み、beatoraja / Luaの切り捨て。

実行したコマンドと結果はPR本文に記載しています。実機・実スキンでのResult表示は確認していません。
