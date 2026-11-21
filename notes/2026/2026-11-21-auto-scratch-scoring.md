# AUTO SCRATCH / BATTLE ASのスクラッチを採点から外す（#119）

- 実施日: 2026-11-21
- 対象: `3bbcf6109`（v0.5.3）からの作業ツリー。AUTO SCRATCHのノーツ非表示設定と同じ作業。
- 仕様: [スコア保存・AUTO SCRATCHの採点](../../docs/score-persistence.md#auto-scratchの採点)、
  [AUTO SCRATCHのノーツ表示](../../docs/settings.md#auto-scratchのノーツ表示)

## 背景

[#119](https://github.com/hyrorre/bmz-player/issues/119) でAUTO SCRATCHのノーツ非表示を実装する途中で、
オートスクラッチのノーツが通常ノーツと同じ見た目で、自動演奏でコンボが増えることを確認した。
BMZはスクラッチレーンへ自動入力を作り、通常の判定でPGREATにしていた。そのため
コンボ・EXスコア・ゲージが増え、総ノーツ数にもスクラッチを含めていた。

## 参照実装との比較

| 実装 | 表示 | 判定・コンボ | 総ノーツ数 |
|---|---|---|---|
| beatoraja（BATTLE AS） | しない | しない | 含めない |
| LR2oraja Endless Dream（AUTO SCRATCH） | しない | しない | 含めない |
| LR2（AUTO SCRATCH） | AUTO用のノーツ画像で表示 | しない（キー音だけ鳴らす） | — |
| BMZ v0.5.3 | 通常のノーツ | PGREAT・コンボ加算 | 含める |

- beatorajaは`BMSPlayer`でBATTLE AS（`doubleoption == 3`）のときに
  `AutoplayModifier(model.getMode().scratchKey)`を掛ける。`AutoplayModifier`は
  `PatternModifier.moveToBackground`で、対象レーンのノーツを背景音へ移す。LNは終端も移し、
  地雷は削除する。ゲージはこの変換の後に作るので、総ノーツ数もスクラッチを除いた数になる。
- Endless DreamのAUTO SCRATCHは、全譜面オプションの後に同じmodifierを掛ける。Kaleidの分岐にも掛ける。
- LR2はAUTO SCRATCHのレーンを判定せず、キー音だけを鳴らす。ノーツはスキンの
  `#SRC_AUTO_NOTE` / `#SRC_AUTO_LN_*` / `#SRC_AUTO_MINE`で描く。

## 判断

採点はbeatoraja / Endless Dreamに合わせ、判定・コンボ・EXスコア・ゲージ・総ノーツ数から外す。
表示はプレイヤーが選べるよう残し、非表示の設定で隠せるようにする。

- ノーツはダミーノーツ（表示だけで判定しない種別）へ変え、キー音をノーツの時刻のBGMへ移す。
  判定・描画・LNの既存のダミー処理をそのまま使える。
- 変換は`transform_chart_for_play`の最後に掛ける。配置でスクラッチへ移ったノーツも対象になり、
  Kaleidの分岐で譜面を作り直すときにも同じ変換が掛かる。Kaleidの実効機能の判定より後に掛け、
  自動演奏用のダミーを譜面の機能として扱わない。
- AUTO SCRATCHのアシスト判定（スクラッチノーツの有無）は、変換の前の譜面で行う。
- 自動演奏のレーンには引き続き`AutoplayController`を割り当てる。人の入力を除く処理、
  `auto_note_lanes`（LR2のAUTO画像、option 54..57、非表示の設定）がこれを参照するため。
- CLIの`--auto-scratch`はpreloadのオプションに含まれないので、`build_game_session`でも
  同じ変換を掛ける。変換済みの譜面では何もしない。

## 影響

- SP AUTO SCRATCHは実効アシストで、IRへ送らずランプ・回数だけ保存する。影響はプレイ中の表示だけ。
- BATTLE ASは専用bucketへ保存し、BMZ IR・BMS-IRへ送る。新しい記録の総ノーツ数とEXスコアは
  スクラッチを除いた値になる。v0.5.3までの記録はスクラッチ込みのまま残る。
- IRの一括送信は、BATTLE ASの総ノーツ数をライブラリの数から補正しない。配置で変わる
  スクラッチの数をライブラリから再現できないため。
- 譜面に`#TOTAL`が無い場合の既定TOTALは、BMZでは変換後の採点ノーツ数から計算する。
  beatorajaは譜面オプションの前のノーツ数から計算するので、BATTLE / BATTLE ASでは差がある。
  BATTLEの複製から続く既存の差で、今回は変えていない。

## 検証

- 追加したテスト
  - `assist::auto_scratch::tests`: ダミー化、BGMへの移動、見えないノーツ、フリーゾーン、再変換
  - `profile_auto_scratch_does_not_judge_or_combo_the_scratch`: ライブプレイでコンボ・EXスコアが増えない
  - `load_transformed_chart_marks_auto_scratch_and_removes_it_from_scoring`: preloadでの変換、
    リプレイでは単体AUTO SCRATCHを掛けない、BATTLE ASは両側を変換
  - `local_backfill_payload_keeps_battle_as_count_without_auto_scratch`: 一括送信のノーツ数
- 期待値を変えたテスト: `double_battle_as_replay_leaves_both_scratches_unscored_on_the_replay_clock`
  （EXスコア8・最大コンボ4から、4・2へ）
- workspace全体の`cargo fmt --check`・`cargo check --workspace --locked`・all-targets Clippy・
  `cargo test --workspace --locked --no-fail-fast`が成功（5922件成功、144件ignored）
- 未実施: 実機でのプレイ確認（キー音のBGM再生、LR2スキンのAUTO画像、非表示の設定）

## 旧BATTLE ASの記録の移行

リプレイから新しい採点で計算し直す案と、旧記録を集計から外す案を比べ、後者にした。
SQLのmigrationだけで済み、履歴が残るので元に戻せる。旧記録のランプとベストは消える。

- score DB migration v40: `BattleAutoScratch`の履歴を`normal_best_eligible = 0`にし、
  `score_best`を削除し、replay slotの比較値を空にする。
- `normal_best_eligible`は、本番の保存経路ではこれまで0にならなかった列で、意味は
  「通常ベストの比較基準にしない履歴」。デバイス別ベストと取り込みのBP集計は既に
  この列で除外していた。集計の作り直し（`rebuild_score_aggregates`）、LR2のクリアした
  オプション、IRの一括送信にも同じ除外を加えた。
- ベストの書き込みと読み出しには「過去の履歴があるか」の判定がある。clear-onlyの行
  （H-RANDOMなどのアシスト付きBATTLE AS）の後に通常プレイをすると、旧履歴があるだけで
  BPのベストが0になったため、この判定でも除外する。migrationの途中のスキーマへ直接
  書き込むテストがあるため、列がある場合だけ条件を加える。
- network DB migration v9: 送信待ち・失敗のBATTLE ASのscore jobを`ineligible`にする。

### 移行の検証

- `storage::score_db::tests::battle_as_migration`: 履歴を残してベスト・ランプ・slotの比較値を
  外すこと、次のプレイで作り直すこと、clear-onlyの後の通常プレイでBPが0にならないこと
  （除外を外すとBPが0になり失敗することを確認）
- `network_migration_stops_unsent_old_battle_as_jobs`: 送信待ち・失敗のjobだけを止めること

## BMZ IRの旧BATTLE ASの記録

サーバーの`scores`には、受理判定の`accepted`と`rejection_reason`がある。bestの再構築・
ランキング・本人のスコア取得・日報は、すでに`accepted = true`だけを使っている。

- 新旧の採点は、クライアントのバージョンでは区別できない。採点を変えた後のmainの
  開発版も0.5.3を名乗るため。新しいクライアントはBATTLE ASのpayloadに
  `play_options.auto_scratch_scoring = "unscored"`を付け、サーバーは印の無い
  `battle_auto_scratch`を旧採点として扱う。
- 旧採点のスコアは`accepted = false`、`rejection_reason = "battle_auto_scratch_legacy_scoring"`
  で保存し、bestを作らず、`accepted: false`を返す。BMZ公式IRのクライアントは応答の
  `accepted`を見ないので、旧クライアントのjobは完了扱いになり、再送を繰り返さない。
  同じidempotency keyの再送も、保存済みの`accepted`を返す。
- D1 migration 0020は、保存済みの印の無い`battle_auto_scratch`を同じ扱いにし、
  それを含むキーの`best_scores`を削除する。更新triggerで`score_history_revision`も進む。
- 0020より前に印付きのスコアが同じキーにあった場合も、そのキーのbestは消える。
  印はこのcommitから送るので、対象は開発版で送った記録だけになる。

### サーバーの検証

- `bmz-ir-web/server/services/ir/submission.test.ts`の`BATTLE AS auto scratch scoring`:
  印の無いスコアは不受理で保存されbest・ランキングに出ない、再送も不受理を返す、
  印付きは受理される、migration 0020が保存済みの旧記録を外し通常のbestを残す。
- 本番のD1へのmigrationの適用とデプロイは未実施。

## 残課題

- 本番のD1へmigration 0020を適用し、サーバーをデプロイする（クライアントの公開より前）。
- BMS-IRへ送信済みのBATTLE ASの記録は、BMZからは変えられない。
