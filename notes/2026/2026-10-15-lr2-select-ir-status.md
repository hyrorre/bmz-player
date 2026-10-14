# LR2選曲スキンのIR状態表示の重複

ECBE Selectのフッターで「IR READY」と「IR NO PLAYER」、取得失敗時に「DISCONNECTED」と
「IR BUSY」が同じ位置に重なって表示されていた。

## 原因

- ECBEの `Select/_csv/parts_footer.csv` は、option 600..608の画像をすべて同じDST (115,706,96,7) に描く。
- BMZはLR2 CSVの600..608を共通のIR optionとして評価していた。共通側はbeatoraja互換で、
  参加者0人では602（LOADED）と603（NOPLAYER）、取得失敗では604（FAILED）と608（BUSY）が同時に成立する。
- 未登録譜面のHTTP 404はrianIR clientで空ランキングになり、Loaded・0人になる。
  登録済みで参加者0人の場合も同じ。
- LR2では600..608が単一の `IRstatus` との一致判定で、常に1つだけが成立する。
  選曲時に1、自動更新待ちで3、接続中に4へ進み、結果で2（参加者あり）/ -1（参加者0人）/ -3（接続失敗）になる。
  IR非対象・自動更新なしは0、BAN曲は-2、更新間隔の制限中は5。

## 対応

- Select / Decideの600..608をLR2専用のoption名前空間へ移し、BMZの状態から単一の値を決める。

  | BMZの状態 | LR2 option |
  |---|---|
  | Offline（IR未設定・非対象の行） | 600 |
  | Waiting（取得待ち） | 606 |
  | Loading（取得中。LR2の接続中に当たる） | 607 |
  | Loaded・参加者あり | 602 |
  | Loaded・参加者0人（未登録譜面を含む） | 603 |
  | Failed | 604 |

  BMZに無い一時読み込み（601）、BAN曲（605）、更新制限（608）は成立しない。
- beatoraja / JSON / Luaの600..608、IR通信と空ランキングの意味、接続timerは変更しない。
  Resultの600..608も従来どおり共通側で評価する。
- 第三者製CSV・画像は変更していない。

## 検証

- 自動テスト
  - 変換: Select / Decideで600..608（`#IF`内を含む）がLR2名前空間になり、Resultは共通のまま。
  - 評価: Offline / Waiting / Loading / Loaded(0) / Loaded(正数) / Failedで成立するoptionが1つだけ。
    正数→0人→失敗→正数の遷移。共通側の重複する意味は維持。
  - 実ECBE（存在時のみ）: 上記の各状態と遷移で、フッターの状態画像がちょうど1枚。
    修正を外すとこのテストが失敗することを確認した。
  - `cargo fmt --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、
    `cargo test --workspace --locked --no-fail-fast` が成功。
- 他のLR2選曲スキン（一時プローブ、コミットしていない）
  - 手元のLR2 / RED_BELT / Seraphic / ECBEの選曲スキンで、各状態のIR状態画像を描画して同一矩形の重複を調べた。
  - LR2同梱スキンだけ、取得失敗時に (552,473,51,5) へ2枚出る。同梱CSVの「IR BUSY」画像が、
    コメントでは608なのに実際はop 604で定義されているためで、LR2本体でも同じ表示になる。スキン側の定義なので変更しない。
- GPU確認（オフスクリーン、`.local/lr2-play-audit/src/bin/ecbe_ir_gpu.rs`）
  - ECBEを実際のテクスチャで描画し、フッターを切り出した。NOT SUPPORTED / WAIT / ACCESSING... /
    READY / NO PLAYER / DISCONNECTEDがそれぞれ1つだけ表示された。
  - 実IRへの接続と、実アプリ画面での確認はしていない。
