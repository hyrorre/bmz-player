# スレッド分離後のスキンtimer更新とLN残光

## 調査

[READYの回転停止修正](2026-11-08-ready-turntable-rotation.md)（`1b1816b0`）からの追加調査。
`1e73de02`のgameplay専用スレッド化と、その後のconsumer側projectionを照合した。

- producerが計算したKEYON / KEYOFF・HOLD・HCN・ゲージ・FULLCOMBO等の経過時間を、
  consumerで再計算していなかった。譜面時刻だけ進み、演出の時間は古いsnapshotへ留まる。
- 実際のGameplayClientでpublication更新を止めた再現では、譜面時刻0→4→20msに対し、
  KEYON / KEYOFFは1000ms、RHYTHMと4分拍は0msのままだった。
- キービームは画面時刻からtimer経過時間を引いて開始時刻を逆算し、値の変化を新しい入力と
  判断していた。LN解放後に古いKEYOFFを再利用すると、4ms後に抑制済みの残光が再び有効になる。
  rendererの実コードを使う再現プログラムでも、正しい経過時間ならfalse、据え置くとtrueとなった。
- worker起動前のキャンセルは別の終了snapshot経路を通り、画面経過時間を設定した後の
  回転・入力timer更新が抜けていた。session準備前のplaceholderも更新する必要があった。

## 修正

- 描画用projectionへ各timerの元の開始時刻を保持する。演出の経過時間を描画時点の
  譜面時計で再計算し、無効なtimerの再起動やelapsedへの加算による累積誤差を避ける。
- 1P / 相手のゲージ・FULLCOMBO、HOLD、HCN、RHYTHM、4分拍、ENDOFNOTEを更新する。
  FULLCOMBOの未来時刻の扱いと、HOLD / HCNの負の経過時間を維持する。
- snapshotからrendererへ押下 / 解放の開始時刻を渡し、キービームの入力識別に使う。
  開始時刻を持たない単発のdraw stateでは、timerの有効化・経過時間の減少で再起動を検出する。
- 準備済みsessionの終了snapshotは既存の終了helperを実際の本番経路で共用する。
  sessionのないplaceholderもpending入力状態を使って更新する。
- 判定・入力・音声の進行とscoreの確定条件は変更しない。

仕様は[gameplay runtime](../../docs/gameplay-runtime.md)と[スキン仕様](../../docs/skin.md)を参照。

## 検証

- snapshot交換停止、50 / 100 / 200%再生、READYとイベント境界、1P / 相手、LN / HCN、
  timerの無効化、ms未満の開始時刻、LN解放後の残光、開始前キャンセルの回帰テストを追加。
- Windowsで`cargo fmt --check`、`cargo check --workspace --locked`、
  `cargo clippy --workspace --all-targets --locked -- -D warnings`が成功。
- `cargo test --workspace --locked --no-fail-fast -- --test-threads=1`を制限外で実行し、全target成功。
  bmz-playerは2967 passed / 57 ignored、bmz-renderは1008 passed / 11 ignored、
  bmz-skinは411 passed / 2 ignored。他crate・integration・doc testも失敗なし。
  native windowを使うintegration test 2件は既定どおりignored。
  ログは`.local/skin-timers-workspace-tests.log`、check / Clippyのログは
  `.local/skin-timers-check-final.log`と`.local/skin-timers-clippy.log`。
  incremental cacheのhard link失敗warningはcopyへのfallbackで、ソースのwarningではない。
- 独立したdata / cache / logsでantique 7Kと同梱サンプル譜面を起動し、GET READYの描画と
  自動演奏からリザルトへの正常終了が成功。検証素材は`.local/validation/skin-timers-20261108/`。
- 開始前キャンセルは本番で共用する終了helperの回帰テストで確認した。実画面操作は
  computer-useのBMZアプリ承認が`Computer Use app approval timed out`となり未実施。
  検証用プロセスは停止した。物理音声の確認、実コントローラー、macOS / Linuxも未実施。
