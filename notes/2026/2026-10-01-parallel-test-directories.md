# 並列テストの一時ディレクトリ衝突対策

## 背景と変更

macOSで `cargo test --workspace --locked --no-fail-fast -- --test-threads=8` を実行し、
Luaスキン準備とbitmap font読み込みの失敗を確認した。ローカルsocket制限外のplayer再実行では
profile切替2件と更新ダウンロード2件が失敗し、同条件の直列実行では2103件成功・6件ignoredだった。
失敗箇所は実行ごとに変動し、ファイル消失や別テストの内容の混入と整合する症状が出ていた。

- `ProfileTestDir` はprofile切替、course replay、更新ダウンロード等で共有するテストhelper。
  旧方式はPIDと時刻だけでパスを生成し、同じ時刻値で別インスタンスが同じrootを所有し得た。
- bitmap fontのhelperも時刻だけでrootを生成していた。
- 両helperにatomic連番を追加し、PID・時刻・連番を名前に含めた。
  `create_dir` で排他的にrootを確保し、既存パスの場合は次の連番で再試行する。
  他テストや以前の実行のディレクトリを再利用・削除しない。
- 16スレッドでprofile用rootを作成し、内容と順次Dropによる削除が互いに影響しないことを検証するテストを追加。

アプリ本体の保存パス・runtime data・スキン仕様は変更していない。
検証手順は [開発環境](../../docs/development.md) と [AGENTS.md](../../AGENTS.md) を参照。

## 検証

- `cargo fmt --check` 成功。
- `cargo check --workspace --locked` 成功。
- `cargo clippy --workspace --all-targets --locked -- -D warnings` 成功。
- socket制限外でworkspace全体を8スレッド実行し、3585件成功・0件失敗・13件ignored。
  子プロセス内で重複実行される1件は集計から除外。
- 不定期な失敗の追加確認として32スレッドでも実行し、同じく3585件成功・0件失敗・13件ignored。
- Starseekerの `play/play7.luaskin` がないテストは早期returnしており、その素材の互換性確認には数えない。
  実機操作、Windows/Linuxでの実行は未実施。

生ログはGit管理外の `.local/test-logs/collision-check.log`、`collision-clippy.log`、
`collision-parallel-8.log`、`collision-parallel-32.log` に保存した。
