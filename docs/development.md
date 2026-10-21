# 開発環境と動作確認

作業方針・変更範囲に応じた検証・コミット規約は[AGENTS.md](../AGENTS.md)を参照します。
OS別のRust / FFmpeg等の準備は[README](../README.md)を確認してください。
以下のコマンドは、特に記載がなければリポジトリrootで実行します。

過去の調査・実装・計測記録は[notesの索引](../notes/README.md)にまとめています。

LinuxのPipeWire/SPA開発依存、任意evdev、共通診断、独立したデータでのA/B検証は
[Linux遅延検証](linux-latency.md)を参照してください。

Linuxのアイドル抑止テストには`dbus-daemon`とUnixソケットを作成できる環境が必要です。
`cargo test -p bmz-player --locked idle_inhibit --lib`はサービス自動起動を無効にした
専用のsession busでPortal / ScreenSaverの模擬サービスを動かし、実際のデスクトップには接続しません。

## ビルド識別情報

`bmz-player`はGitのHEADとコードのdirty状態をビルド時に記録します。
通常checkoutとlinked worktreeのGit管理ファイルは`git rev-parse --git-path`で解決し、
HEAD・参照先・既存のpacked-refsを監視します。packed branchにloose refがない場合は
既存の親ディレクトリを監視し、次のcommitでのref作成も検知します。

Gitを含まないソースアーカイブでは、最初のビルド前にrootへ`BUILD-COMMIT`を用意します。
`BMZ_BUILD_COMMIT_OVERRIDE`が指定されていればそれを優先します。
存在しないmetadataを監視対象へ渡さず、変更のないビルドを毎回やり直さないようにします。
Linux上の回帰テストは`python3 scripts/test_build_identity.py`で実行できます。
Git・Python 3.11以降・Rustが必要で、依存を取得済みの環境でoffline Cargoを使います。

## 同梱アセット

Git管理のデフォルトスキン・サンプル曲・フォントは、そのcheckoutのファイルを使います。
同梱スキンsubmoduleが空の場合は次で初期化します。

```bash
git submodule update --init --recursive data/skins/Rmz-skin data/skins/mz-select data/skins/Luxez-Flat
```

ライセンスは[licenses.md](licenses.md)と各submodule内のREADME / licenseを確認します。
Git管理外の第三者製スキンは参照用であり、コミット・再配布の対象にしません。

## worktree用のruntime data

動作確認に既存データが必要な場合だけ、元checkoutから作業用snapshotをコピーします。
worktree側のDB / config / profileを独立させ、元checkoutを直接更新しないようにします。
曲root等の設定に含まれる絶対パスはコピーしても変わらないため、scanやファイル操作の対象も確認します。

コピー前の条件:

1. 元と先のcheckoutを確認し、コピー先を作業中worktreeのrootにします。
2. コピー元へ書き込むBMZや関連ツールを正常終了します。DBはWALモードを使うため、
   書き込み中の`.db`単体コピーは使いません。未反映のWALが残る場合や停止できない場合は、
   SQLiteのbackup機能等で整合したsnapshotを別途作成します。
3. コピー先に既存データがある場合は内容を確認し、必要なものを退避します。
   以下の例は衝突時に停止します。既存データの上書きは自動で行いません。

対象は`data/profiles`、`data/config.toml`、`data/library.db`、追加スキン・曲です。
`default` / `Rmz-skin` / `mz-select` / `Luxez-Flat`と`sample-playable`はコピーから除外し、
worktree自身のGit / submodule checkoutを使います。

### macOS / Linux（Bash）

`bmz_source_root`を実際の元checkoutへ置き換えます。DBについて上の条件を満たしてから実行します。

```bash
(
  set -euo pipefail
  bmz_source_root="/path/to/main/bmz-player"
  bmz_source_root="$(cd "$bmz_source_root" && pwd -P)"
  bmz_target_root="$(pwd -P)"
  test "$bmz_source_root" != "$bmz_target_root"
  test "$bmz_target_root" = "$(git rev-parse --show-toplevel)"
  test "$bmz_source_root" = "$(git -C "$bmz_source_root" rev-parse --show-toplevel)"

  bmz_copy_paths=()
  for bmz_path in data/profiles data/config.toml data/library.db; do
    if test -e "$bmz_source_root/$bmz_path"; then
      bmz_copy_paths+=("$bmz_path")
    fi
  done
  for bmz_path in "$bmz_source_root"/data/skins/* "$bmz_source_root"/data/songs/*; do
    test -d "$bmz_path" || continue
    bmz_relative="${bmz_path#"$bmz_source_root"/}"
    case "$bmz_relative" in
      data/skins/default|data/skins/Rmz-skin|data/skins/mz-select|data/skins/Luxez-Flat|data/songs/sample-playable) continue ;;
    esac
    bmz_copy_paths+=("$bmz_relative")
  done
  for bmz_path in "${bmz_copy_paths[@]}"; do
    if test -e "$bmz_path" || test -L "$bmz_path"; then
      echo "Copy stopped: destination exists: $bmz_path" >&2
      exit 1
    fi
  done
  mkdir -p data/skins data/songs
  for bmz_path in "${bmz_copy_paths[@]}"; do
    cp -a "$bmz_source_root/$bmz_path" "$bmz_path"
  done
)
```

### Windows（PowerShell 7）

`$bmzSourceRoot`を実際の元checkoutへ置き換えます。DBについて上の条件を満たしてから実行します。

```powershell
& {
    $ErrorActionPreference = 'Stop'
    $bmzSourceRoot = (Resolve-Path -LiteralPath 'C:\path\to\main\bmz-player').Path
    $bmzTargetRoot = (Get-Location).Path
    $bmzTargetGitRoot = git rev-parse --show-toplevel
    if ($LASTEXITCODE -ne 0) { throw 'Run from the destination worktree.' }
    $bmzSourceGitRoot = git -C $bmzSourceRoot rev-parse --show-toplevel
    if ($LASTEXITCODE -ne 0) { throw 'Source is not a Git checkout.' }
    if ($bmzSourceRoot -eq $bmzTargetRoot -or
        $bmzTargetRoot -ne (Resolve-Path -LiteralPath $bmzTargetGitRoot).Path -or
        $bmzSourceRoot -ne (Resolve-Path -LiteralPath $bmzSourceGitRoot).Path) {
        throw 'Use different source and destination checkout roots.'
    }

    $bmzCopyPaths = @('data/profiles', 'data/config.toml', 'data/library.db') |
        Where-Object { Test-Path -LiteralPath (Join-Path $bmzSourceRoot $_) }
    foreach ($bmzGroup in 'skins', 'songs') {
        $bmzDirectory = Join-Path $bmzSourceRoot "data/$bmzGroup"
        if (-not (Test-Path -LiteralPath $bmzDirectory)) { continue }
        $bmzExcluded = if ($bmzGroup -eq 'skins') {
            @('default', 'Rmz-skin', 'mz-select', 'Luxez-Flat')
        } else { @('sample-playable') }
        foreach ($bmzItem in Get-ChildItem -LiteralPath $bmzDirectory -Directory) {
            if ($bmzItem.Name -notin $bmzExcluded) {
                $bmzCopyPaths = @($bmzCopyPaths) + "data/$bmzGroup/$($bmzItem.Name)"
            }
        }
    }
    foreach ($bmzPath in $bmzCopyPaths) {
        if (Test-Path -LiteralPath (Join-Path $bmzTargetRoot $bmzPath)) {
            throw "Copy stopped: destination exists: $bmzPath"
        }
    }
    New-Item -ItemType Directory -Force data/skins, data/songs | Out-Null
    foreach ($bmzPath in $bmzCopyPaths) {
        Copy-Item -LiteralPath (Join-Path $bmzSourceRoot $bmzPath) `
            -Destination (Join-Path $bmzTargetRoot $bmzPath) -Recurse
    }
}
```

コピー後は`git status --short`を確認します。runtime dataはコミットに含めません。
DB migration / storage / scanの検証でコピー先が更新されても、元へ書き戻しません。

## 起動とsmoke

```bash
cargo run -p bmz-player
cargo run -p bmz-player -- --help
cargo run -p bmz-player -- --help-dev
cargo run -p bmz-player -- --boot-play-sample
cargo run -p bmz-player -- --boot-play-sample --smoke-exit-after-frames 3
cargo run -p bmz-player -- --boot-play-sample --autoplay-on-start --smoke-exit-on-result
cargo run -p bmz-player --release -- --boot-play-sample --autoplay-on-start --smoke-exit-after-play-frames 360
```

`--smoke-exit-after-play-frames`はPlay sceneの描画フレームだけを数えます。
FPS等の比較には同じ譜面・設定・バックエンド・計測条件が必要です。smoke成功だけでは性能を判断しません。
リプレイ確認は、対象profile・譜面に保存済みのslotがある場合に
`--boot-replay 1 --smoke-exit-on-result`を使います。

操作は[controls.md](controls.md)、起動時の一時設定は[cli-overrides.md](cli-overrides.md)、
外部エディタ連携は[viewer.md](viewer.md)、オフライン動画出力は[video-export.md](video-export.md)を参照します。
CLI引数・サブコマンドの一覧は`--help`と[cli.rs](../crates/bmz-player/src/cli.rs)を正とし、ここには重複して列挙しません。
`--help`はプレイヤー向けの起動・Viewer・一時設定・サブコマンドだけを表示します。`--boot-*-sample`、
`--lua-skin-runtime`、`--smoke-*`、`audio-probe`、`--latency-*`等の開発・検証用オプションは`--help-dev`で表示します。

### Windows debug起動のスタック回帰と調査ログ

次のテストは別プロセスでdebug実行ファイルを起動し、LR2 Selectスキンの同期ロードと
3フレーム描画・正常終了を確認します。GPU・音声デバイスとウィンドウ表示が必要なため、
通常のテスト実行ではignoreします。設定・DBは一時ディレクトリに作成します。
合成スキンは常に検証し、WMIXは`data/skins/WMIX_HD`に素材がある場合に検証します。

```powershell
cargo test -p bmz-player --locked --test startup_stack -- --ignored --test-threads=1
```

通常のinfoログにはdefault manifestのロード・同期decode・installの開始と完了を記録します。
JSON変換やフォント・画像処理の段階まで調べる場合は、PowerShellで次を使います。
`RUST_LOG`は設定ファイルのログレベルより優先されます。

```powershell
New-Item -ItemType Directory -Force .local | Out-Null
$bmzPreviousRustLog = $env:RUST_LOG
try {
    $env:RUST_LOG = 'info,bmz_skin=debug,bmz_player::skin_loader=debug'
    cargo run -p bmz-player 2>&1 | Tee-Object .local/startup-console.log
} finally {
    $env:RUST_LOG = $bmzPreviousRustLog
}
```

LR2のコマンド単位の追跡は`bmz_skin::lr2=trace`、個々のフォント・画像のdecode開始は
`bmz_player::skin_loader=trace`で有効にします。コマンド番号はコメント・空行を除いた
解析済みコマンドの番号です。並列workerのログが混在するため、スキンのpathと
ファイルログのthread名・IDも合わせて確認します。
原因と修正時の計測は[debug起動のスタックオーバーフロー調査](../notes/2026/2026-10-14-debug-startup-stack-overflow.md)に記録しています。

IR Webの環境・DB・検証コマンドは[ir.md](ir.md)の「開発環境と実装の入口」を参照します。

Luaの画面寸法helperを変更した場合は、ロード・cache・非同期世代の回帰に加え、
GPUを使う次のテストで全sceneとoffscreen出力の物理寸法を確認します。

```bash
cargo test -p bmz-skin --locked screen_dimensions
cargo test -p bmz-player --locked screen_dimensions
cargo test -p bmz-player --locked screen_dimensions_follow_offscreen_output_and_target_replacement_in_all_scenes -- --ignored
```

ウィンドウの手動確認では、起動直後、リサイズ、高DPI環境でロード時とcallbackの
寸法を表示し、スキン設計解像度や内部解像度へ置き換わらないことを確認します。

### インストール済みLuaスキンの互換比較

Lua sandboxやAPIを変更するときは、変更前のcommitでインストール済みスキンの
スナップショットを取り、変更後と比較します。ハーネスは各`.luaskin`を初期ロード・
代表的なscene状態・compatモードで2回ずつ読み、選択肢、文書の各セクション、
font / source、警告、runtime callbackの評価結果を記録します。スキンrootへは書き込みません。
スナップショットは第三者スキン由来のため`.local/`へ置き、コミットしません。

```powershell
$env:BMZ_INSTALLED_SKIN_ROOT = "$PWD\data\skins"
# 変更前のcommit（別worktree等）で、揺れの検出用に複数回取得する
$env:BMZ_SKIN_COMPAT_OUT = "$PWD\.local\lua-api-compat\before1.json"
cargo test -p bmz-skin --locked --lib installed_lua_skins -- --ignored
$env:BMZ_SKIN_COMPAT_OUT = "$PWD\.local\lua-api-compat\before2.json"
cargo test -p bmz-skin --locked --lib installed_lua_skins -- --ignored
# 変更後
$env:BMZ_SKIN_COMPAT_BASELINE = "$PWD\.local\lua-api-compat\before1.json;$PWD\.local\lua-api-compat\before2.json"
$env:BMZ_SKIN_COMPAT_OUT = "$PWD\.local\lua-api-compat\after.json"
cargo test -p bmz-skin --locked --lib installed_lua_skins -- --ignored
```

読込失敗、選択肢・font・sourceの消失、objectの減少、新しい`ERROR`警告、runtime callback
失敗の増加でテストが失敗し、それ以外の差分は`after.report.txt`へ確認用に出力します。
複数のベースライン間で揺れる項目（時刻・乱数・Lua 5.4のハッシュ順による推論差）は除外します。
`BMZ_SKIN_COMPAT_FILTER`で対象を絞り、`BMZ_SKIN_COMPAT_OPTIONS` / `BMZ_SKIN_COMPAT_FILES`
（JSONオブジェクト）で既定外の選択肢・ファイル選択を指定できます。

## スキンのCPU計測

Lua API呼出しは次のrelease用probeで計測できます。ロードは計測外で、300フレームのwarmup後に
10,000フレーム測り、500件のdraw callbackとAPIを53回読むtext callbackを出力します。
callbackごとにscopeを作る場合と、描画と同じ共有scopeの場合を分けます。
`BMZ_LUA_PROFILE_FRAMES`で計測フレーム数を変えられます。

```bash
cargo run -p bmz-skin --release --locked --example lua_runtime_profile
```

ビットマップ文中の欠字を初めて描くときのCPUラスタライズは、次で計測します。
フォント読込・atlasへの格納・GPU uploadは計測外です。`BMZ_TEXT_PROFILE_ITERATIONS`で反復回数を変えられます。

```bash
cargo test -p bmz-render --release --locked --lib profile_styled_fallback -- --ignored --nocapture
```

実スキンの描画準備には`bmz-player`の`scene_plan_profile` / `play_plan_profile` exampleを使います。
CPU時間とGPU・surface・presentを含む実ウィンドウの時間は別に比較します。
条件と測定例は[スキン最適化の記録](../notes/2026/2026-10-21-skin-review-optimization.md)を参照します。

## テストの一時リソースと並列実行

`cargo test`は1つのテストバイナリの中で全テストを並列に動かします。さらに別のworktree、
CIジョブ、IDEとターミナルなど、別のテストプロセスが同時に動くことがあります。
テスト同士・テストプロセス同士が干渉しないよう、次の規則に従います。

- 一時ファイル・ディレクトリは`tempfile::TempDir`（`tempfile::Builder`でprefixを付けてよい）
  を作り、テストが寿命の間保持します。片付けはDropに任せ、手動の`remove_dir_all`は書きません。
  失敗したテストの残骸も残りません。
  `bmz-skin`と`bmz-player`のテストhelper（`unique_test_dir`等）は、`TempDir`の中の
  未作成パスを指す`TestDir`を返します。`TestDir`は変数に束縛して保持します。
  `unique_test_dir("x").join("skins")`のように一時値のまま使ったり、`fs::canonicalize(root)`のように
  値で渡したりすると、その時点でディレクトリが削除されます。パスが必要な箇所には`&root`や
  `root.to_path_buf()`を渡します。
- 名前にPIDだけ、時刻だけ、ファイルの長さ等を使って独自に一時パスを作りません。
  同じプロセスでは一意でも、PIDの再利用や別プロセスとの同時実行で衝突します。
- 一時ディレクトリ直下（`std::env::temp_dir()`そのもの）にファイルを置いたり、
  それをprofile root等として渡したりしません。譜面と参照する音声・画像のように
  「隣に置く」必要があるファイルも、テスト専用のディレクトリの中に作ります。
- 「まだ存在しないパス」が必要なテストは、`TempDir`の中の未作成パスを使います。
- ソケット等の待受先は、本番の固定アドレスを使わずOSに割り当てさせます（`127.0.0.1:0`）。
  実行中のBMZや別のテストプロセスと衝突しないようにするためです。
- 非同期の処理を待つときは、途中の状態ではなく最終状態（例: jobの`failed`）を条件にします。
- 別スレッドが一定の実時間（sleepの長さ等）の間にどこまで進むかをassertしません。
  CPUが混んでいるとスレッドの起床が遅れるためです。進行を確かめるときは、
  期限付きで条件が満たされるまで待ちます。

再現の確認には、同じテストを2つのプロセスで同時に繰り返し実行するのが有効です。

```bash
bin=$(cargo test -p <crate> --locked --lib --no-run --message-format=json \
  | python3 -c "import sys,json;[print(m['executable']) for m in map(json.loads,sys.stdin) if m.get('reason')=='compiler-artifact' and m.get('executable')]")
for i in $(seq 1 50); do
  (cd crates/<crate> && "$bin" --exact <test path>) & (cd crates/<crate> && "$bin" --exact <test path>); wait
done
```

複数のworktreeで`CARGO_TARGET_DIR`を共有すると、内容の同じクレートは別のworktreeで
作ったテストバイナリが再利用され、`env!("CARGO_MANIFEST_DIR")`に元のworktreeのパスが残ります。
そのworktreeを削除すると、同梱データを参照するテストが存在しないパスを読んで失敗します。
worktreeごとにtargetを分けるか、削除したworktreeの成果物を`cargo clean`してから実行します。

実スキンを読むテストはメモリを多く使います。`--test-threads`を大きくしたり、
複数のテストプロセスを同時に走らせたりするとOOMで強制終了されることがあります（終了コード137）。
