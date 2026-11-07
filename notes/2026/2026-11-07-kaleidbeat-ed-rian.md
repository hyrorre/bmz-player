# ED-rian KaleidBeat拡張の統合

現在の契約は[docs/kaleid.md](../../docs/kaleid.md)。単一PRで実装し、機能別コミットに分ける。
実装開始前にSSH署名を設定し、コミットの署名検証を行った。

## 参照

- BMZ開始main: c8209e227b0233f84532638fe7c8a976a22e1341。
  作業中にmain 015942f402d551ca3b2a0cc62591a80c175f1881のEXTRA NOTE修正を統合した。
- [旧BMZ PR #20](https://github.com/hyrorre/bmz-player/pull/20): closed/unmerged、
  head 0404fb6aed5b7d2e1eb2d06e37b77afb98cda6dd、base a4a1509f9e853b07949aff2b5eebc18f89bdffa5。
  HLN・終端音・分岐の設計／テストを選別し、mainの保存・入力スレッド・IRへ作り直した。
  旧migration番号・旧IR一律除外・予約済み旧枝BGMの方針は引き継がない。
- [rian-eimu版ED PR #39](https://github.com/rian-eimu/lr2oraja-endlessdream/pull/39):
  head 6d795bb9bf0222743938efc40801414143572cc6、merge cfac8e203dfa3b09176549d385a528038a5b4ec6。
  導入前v1.5.0は9f9ba101aa1177055390215525ea6d6c66db2f17。
  安定版v1.6.0／調査時開発mainは871f9a092ad5ad4a8d9c9c7541294c043003fc86。
  seraxis版とは区別した。パーサーrian-eimu/jbms-parserは71bac5808ac2a7ee11df141e44a97381c1a36416。
- Kaleid公開資料v0.5.8の取得アーカイブSHA-256:
  54f02b549f2f341386952d07d4f7f654724e5fdbd70664cf70c77b173f06fe95。
  旧PRが参照する.local/KaleidBeatは見つからず、読了扱いにしない。
- 添付rianIR分割7zは結合・展開してサーバーコードを参照した。Git情報なし、555ファイル。
  結合SHA-256はc0692c08773c487bb9b0dfb9e9a9b87a9bd050ee2c16573669a5f379d0ce43a9。
  添付ZIP／7zや第三者ソースはリポジトリへ追加しない。
- [BMS-IR](https://www.bms-ir.org/)の公式資料とBMZ専用API実装を区別した。
  Javaプラグインの対応フラグをBMZ APIの受入証拠とみなさない。

## 合意と差分

ED実装済み機能のみを対象とし、AUTO/FORCEは既存6設定、既存3rule_modeを維持する。
RATEは全曲DEFAULT分母でゼロなら100。通常・コース・BATTLEで動的評価、PracticeはDEFAULT。
17/27は非PMSでmtimeによらず解釈する。BATTLE動的評価とこの年代判定はEDとの差分。
ローカル／BMZ IRは原本単位で枝や部分対応を同じランキングに集約する。
rianIRはED＋サーバー契約、BMS-IRは未確認機能の送信を保留する。

最新mainのEXTRA NOTE配置・地雷保持を維持し、ダミー・ポーション除去を接続する。
HLN・ゾーンの占有区間へ追加ノーツを置かない。非拡張譜面への配置変更を避ける。
フリーゾーン内のHCN/HLN保持ゲージ、採点後のHLN音声、分岐で変わるBMSON音声区間、
途中終了Replay、独立ベストの元履歴、未採択枝のIR検査を重点的に確認した。

## 検証記録

Linux x86_64、Rust 1.99.1、Bun 1.4.4。FFmpeg等の開発ヘッダーは検証環境へ導入した。
既存日本語期待値に合わせLANG=ja_JP.UTF-8でRustテストを実行する。
同梱Luxez-Flat／Rmz-skin／mz-selectはmain指定submoduleを初期化し、参照を変更していない。

- 最新main統合後にfmt、workspace check、all-targets Clippy（-D warnings）、
  workspace全テスト（--locked --no-fail-fast）が成功。
  player 2,997、gameplay 307、chart 197＋fixture 42、audio 173、render 1,008、skin 416ほか。
  既存の実機向けignoreは維持。追加したGPUテストは下記のとおり明示的に実行した。
  任意外部素材を持たず早期returnする既存テストを、実機互換の根拠にはしていない。
- IR Webのtest:ir 218件とvue-tscが成功。変更ファイルのPrettier checkも成功。
- Mesa 25.0.7 lavapipeのoffscreen画素テストでポーションRGB反転・alpha維持を確認。
  物理GPU・実機操作の検証ではない。Vulkanドライバー未導入時の失敗とは区別する。
- コース未プレイ曲の拡張判定は、ライブラリスキャンではなく各試行の固定RANDOM seedでの
  事前取込から伝搬する。BMS-IR対象外キューが認証前に拒否され、再度自動処理されないこと、
  書庫内BMC、分岐後の結果待機末尾、フリーゾーン内部HCN/HLN保持ゲージも回帰確認した。

## 再構築の限定計測

Rust release、BPM120、800小節の通常ノーツ3,200個に独立分岐16個を追加した合成BMC
（DEFAULT計3,216個、14,146 bytes）を使用。各枝は4個／DEFAULT1個。
選択済み枝を先頭から増やす16通りを5巡し、materializeを80回計測した。
他のビルド・テスト終了後の1実行で、取込289.5ms、materialize中央値4.146ms、
p95 4.656ms、最大4.871ms、プロセス最大RSS 8,448KiBだった。
これは譜面再構築だけの測定で、音声・BGAの実資源ロード、判定状態の移送、描画キャッシュ更新、
実機入力を含まない。全体のフレーム時間・音声レイテンシの保証やEDとの性能比較には使わない。

ログは実行環境の/tmp/bmz-kaleid-*.logに置いた一時資料で、Git管理せず永続性は保証しない。
ED本体を同一入力で起動する比較は未実施。ED準拠という記述はコード照合とBMZ側fixtureの
検証範囲を指し、Kaleid本体との完全互換を保証しない。
本番IR送信・問い合わせ・サーバーデプロイは未実施。添付rianIRと本番の同一性、
ビルドclient_hash受入、物理音声／GPUの遅延、大規模実譜面のレイテンシは未確認。

## PR #82レビュー後の修正

- HLNだけを含む全レーンReplayがprocess_hln_inputsへ入り、見逃し・地雷通過・保持ゲージの
  フレーム処理を通らない問題を再現した。全レーンReplayはHLNでも既存のReplay時計を使う。
  既存の速度・終端境界テストにHLNを追加し、全採点数の確定をassertする。
  入力がある場合／空Replayの両方で、10秒までにPOOR確定・past=5・Finishedになることを確認する。
- BATTLE相手のViewer seekで、復元済みprefixをprocessed_timeへ反映する。非Replayの
  時刻列は遅延計算し、保持・判定待ちのない空白を次の配置・分岐まで飛ばす。
  120秒seek、116秒の空白、HLN/HCNを含む125秒の一括送りと通常刻みの結果一致を検証する。
  保持中の1ms判定周期は維持し、全ての長時間保持を定数時間化したとはみなさない。
- charts.kaleidの欠落／version 0を既知版へ更新可能にする。譜面単位では
  version・declared・sourceだけを保持し、effective・diagnosticsはプレイごとの証拠に残す。
  旧方式で保存済みのフィールドは次回送信時に除去する。欠落／0→1、後続旧キューによる
  逆戻り防止、プレイ証拠の保持、既存値の除去をSQLサービスの回帰テストで確認する。

修正前のHLN終了テストとIRの3ケースで失敗を確認し、修正後に成功した。
player全2,997件を確認した後、最初のゾーン入力より前にAUTO状態を設定する調整を加え、
最終コードでgameplay全312件、playerのBATTLE 65件・Replay 115件を再検証した。
対象2crateのcheck・all-targets Clippy（-D warnings）、fmt、IR Web 221件・vue-tsc・
変更ファイルのPrettier checkも成功。本番IRへの送信・サーバー変更は行っていない。
