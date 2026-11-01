# RIVALCHART MIRROR（相手の譜面を左右反転して借りる）

CHART REPLICATIONにBMZ拡張の`RIVALCHART MIRROR`を追加した。G-BATTLEの相手（KEY4）と
7で選択中のrianIRのライバル（KEY7）の両方に適用する。切り替え順は
`NONE → RIVALCHART → RIVALCHART MIRROR → RIVALOPTION`で、設定ファイルとtext 86 /
`bmz_select_chart_replication`の値も`RIVALCHART MIRROR`。現在の規則は
[controls.md](../../docs/controls.md)のG-BATTLEの項と[rian-ir.md](../../docs/rian-ir.md)を参照する。

## LR2orajaEDの類似機能（調査）

`.local/lr2oraja-endlessdream`（`bcd84503`時点）で確認した。

- LR2IRのランキング行から始めるGhost Battle（`LeaderBoardBar.java`）で、自分の1P配置オプションが
  MIRRORのときだけ相手の譜面を反転する（`BMSPlayer.java`、2025-10-21の`db14fe22`）。
- 相手が正規ならMIRROR、MIRRORなら正規。RANDOMは鍵盤の並び（例: 2537146）を逆順にし、
  jar同梱の`resources/randomtrainer.dat`（7Kの5040通りの並び→beatoraja seed）でseedを引いて
  通常のRANDOMとして適用・保存する。
- 7K SPで、相手が正規・MIRROR・RANDOMの場合だけ。S-RAN / H-RANは非対応。
  CHART REPLICATION自体はbeatorajaと同じ値のまま。

## 方式

ユーザー指定により、反転後の鍵盤の並びから通常のオプションとseedを逆算して保存する。
S-RANDOM系は反転せず、通知もしない。設定はCHART REPLICATIONの4つ目の値（B案）とした。

| 相手の配置 | 反転後のオプション | seed |
|---|---|---|
| 正規 / MIRROR | MIRROR / 正規 | 不要 |
| F-RANDOM / MF-RANDOM | MF-RANDOM / F-RANDOM | 逆算 |
| RANDOM / RANDOM-EX / R-RANDOM | 同じ | 逆算 |
| S-RANDOM / SPIRAL / H-RANDOM / ALL-SCR / S-RANDOM-EX | 反転しない | RIVALCHARTと同じ |

- F-RANDOMは左右の半分の中だけで並べ替えるため、反転すると左右の半分が入れ替わりF-RANDOMでは表せない。
  BMZのMF-RANDOMは「F-RANDOMの並びの左右反転」（`f_random_lane_permutation`）なので、
  反転後は必ずMF-RANDOMになる。同じseedのままでも表せるが、ユーザー指定で他と同じくseedを探す。
- R-RANDOMは正規とMIRRORを除く回転と鏡映（7Kで12通り）で、反転してもこの中に収まる。
- SPIRALは時刻ごとに「元の鍵盤i → (i + head) mod n」の回転を使うため、反転すると鏡映になり表せない。
  S-RANDOM / H-RANDOM / ALL-SCR / S-RANDOM-EXはノートごとに乱数を引くため表せない。
- DPは1P/2Pを別々に反転し、seedもサイドごとに探す。FLIPはそのまま。BATTLE系のDP OPTIONは
  2P側を1P側から作るため反転しない。
- 相手のseedがない場合（オプションだけ写った場合）は、seedの要らない正規とMIRRORだけを入れ替える。
- ローカルのリプレイの相手は、保存された鍵盤の並び（`arrange_pattern`）を元に反転する。
- 旧BMZのlegacy seedは現行方式のseedへ置き換える。反転しないサイドにlegacy seedが残る場合は、
  読み方が変わるため全体を反転しない。
- G-BATTLEの相手側は相手自身の配置で作った別の譜面で判定するため（`play_session/preload.rs`）、
  自分側を反転しても相手のリプレイ判定は変わらない。2P側の表示は従来どおり自分と同じ配置。
  選曲画面のG-BATTLEパネルは相手の元の配置を表示する。

## seedの逆算

beatoraja互換のJavaRandomとシャッフルを再現したプログラムで、24bitの全seedを総当たりした
（作業用のため保存していない）。

| レーン数 | 並び | 作れる並び | 1並びあたりのseed数 | 0から探した最悪の回数 |
|---|---|---|---|---|
| 4 | 24 | 24 | 約70万 | 6,152 |
| 5 | 120 | 120 | 約14万 | 472 |
| 6 | 720 | 720 | 約2.3万 | 4,021 |
| 7 | 5,040 | 5,040 | 3,206〜3,452 | 47,587 |
| 8 | 40,320 | 40,320 | 350〜478 | 476,557 |
| 9 | 362,880 | 362,880 | 20〜78 | 4,694,340 |

全ての並びがseedで作れるため、EDのような対応表は持たず、プレイ開始時に0から探す。
探索は1回目の不一致で打ち切る判定（`shuffle_lane_group_matches`）を使い、割り当てを行わない。
最小のseedを使うため、同じ相手の譜面からは常に同じseedになる。

## 実装

- `screens/play_session/arrange/mirror.rs`の`mirror_copied_arrangement`。seedや保存された並びから
  各サイドの並びを作り、反転して最初に一致するseedを探す。
- seedから並びを作る処理は、G-BATTLEパネルのプレビューと共通の`fixed_side_permutation`へ切り出した。
- `app/play_flow/launch/preload.rs`のKEY7の写し方を`apply_rival_score_replication`へ切り出し、
  G-BATTLEの`apply_battle_target_replication`と同じく、写した後に`mirror_replicated_chart`で書き換える。

## 検証

- 全キー数（4/5/6/7/8/9/10/14K）、固定配置7種、新旧seed、DPのOFF/FLIPで、反転後のオプションと
  seedだけから実際の配置処理で作った譜面が、元の譜面の各サイドの左右反転と一致することを確認した。
  リプレイの並びからの反転、S-RANDOM系・seedなし・legacy・BATTLEの扱いも単体テストで確認した。
- KEY7 / G-BATTLEの写し方と、設定値の保存名・切り替え順をテストした。
- `cargo fmt --check`、`cargo check -p bmz-player --locked`、
  `cargo clippy -p bmz-player --all-targets --locked -- -D warnings`: 成功。
- `cargo test -p bmz-player --locked`: 2,787件成功、50件ignore。別のstartup smoke 2件はignore。
- 実際のIRライバル・ローカルのリプレイを相手にした通常起動でのプレイと、スコア・IR送信の実機確認は未実施。
