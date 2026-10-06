# 開発環境と動作確認

作業方針・変更範囲に応じた検証・コミット規約は [AGENTS.md](../AGENTS.md) を参照します。
OS別のRust / FFmpeg等の準備は [README](../README.md) を確認してください。
以下のコマンドは、特に記載がなければリポジトリrootで実行します。

過去の調査・実装・計測記録は [notesの索引](../notes/README.md) にまとめています。

LinuxのPipeWire/SPA開発依存、任意evdev、共通診断、独立したデータでのA/B検証は
[Linux遅延検証](linux-latency.md)を参照してください。

Linuxのアイドル抑止テストには `dbus-daemon` とUnixソケットを作成できる環境が必要です。
`cargo test -p bmz-player --locked idle_inhibit --lib` はサービス自動起動を無効にした
専用のsession busでPortal／ScreenSaverの模擬サービスを動かし、実際のデスクトップには接続しません。

## ビルド識別情報

`bmz-player` はGitのHEADとコードのdirty状態をビルド時に記録します。
通常checkoutとlinked worktreeのGit管理ファイルは `git rev-parse --git-path` で解決し、
HEAD・参照先・既存のpacked-refsを監視します。packed branchにloose refがない場合は
既存の親ディレクトリを監視し、次のcommitでのref作成も検知します。

Gitを含まないソースアーカイブでは、最初のビルド前にrootへ `BUILD-COMMIT` を用意します。
`BMZ_BUILD_COMMIT_OVERRIDE` が指定されていればそれを優先します。
存在しないmetadataを監視対象へ渡さず、変更のないビルドを毎回やり直さないようにします。
Linux上の回帰テストは `python3 scripts/test_build_identity.py` で実行できます。
Git・Python 3.11以降・Rustが必要で、依存を取得済みの環境でoffline Cargoを使います。

## 同梱アセット

Git管理のデフォルトスキン・サンプル曲・フォントは、そのcheckoutのファイルを使います。
同梱スキンsubmoduleが空の場合は次で初期化します。

```bash
git submodule update --init --recursive data/skins/Rmz-skin data/skins/mz-select data/skins/Luxez-Flat
```

ライセンスは [licenses.md](licenses.md) と各submodule内のREADME / licenseを確認します。
Git管理外の第三者製スキンは参照用であり、コミット・再配布の対象にしません。

## worktree用のruntime data

動作確認に既存データが必要な場合だけ、元checkoutから作業用snapshotをコピーします。
worktree側のDB / config / profileを独立させ、元checkoutを直接更新しないようにします。
曲root等の設定に含まれる絶対パスはコピーしても変わらないため、scanやファイル操作の対象も確認します。

コピー前の条件:

1. 元と先のcheckoutを確認し、コピー先を作業中worktreeのrootにします。
2. コピー元へ書き込むBMZや関連ツールを正常終了します。DBはWALモードを使うため、
   書き込み中の `.db` 単体コピーは使いません。未反映のWALが残る場合や停止できない場合は、
   SQLiteのbackup機能等で整合したsnapshotを別途作成します。
3. コピー先に既存データがある場合は内容を確認し、必要なものを退避します。
   以下の例は衝突時に停止します。既存データの上書きは自動で行いません。

対象は `data/profiles`、`data/config.toml`、`data/library.db`、追加スキン・曲です。
`default` / `Rmz-skin` / `mz-select` / `Luxez-Flat` と `sample-playable` はコピーから除外し、
worktree自身のGit / submodule checkoutを使います。

### macOS / Linux（Bash）

`bmz_source_root` を実際の元checkoutへ置き換えます。DBについて上の条件を満たしてから実行します。

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

`$bmzSourceRoot` を実際の元checkoutへ置き換えます。DBについて上の条件を満たしてから実行します。

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

コピー後は `git status --short` を確認します。runtime dataはコミットに含めません。
DB migration / storage / scanの検証でコピー先が更新されても、元へ書き戻しません。

## 起動とsmoke

```bash
cargo run -p bmz-player
cargo run -p bmz-player -- --help
cargo run -p bmz-player -- --boot-play-sample
cargo run -p bmz-player -- --boot-play-sample --smoke-exit-after-frames 3
cargo run -p bmz-player -- --boot-play-sample --autoplay-on-start --smoke-exit-on-result
cargo run -p bmz-player --release -- --boot-play-sample --autoplay-on-start --smoke-exit-after-play-frames 360
```

`--smoke-exit-after-play-frames` はPlay sceneの描画フレームだけを数えます。
FPS等の比較には同じ譜面・設定・backend・計測条件が必要です。smoke成功だけでは性能を判断しません。
リプレイ確認は、対象profile・譜面に保存済みのslotがある場合に
`--boot-replay 1 --smoke-exit-on-result` を使います。

操作は [controls.md](controls.md)、起動時の一時設定は [cli-overrides.md](cli-overrides.md)、
外部エディタ連携は [viewer.md](viewer.md)、オフライン動画出力は [video-export.md](video-export.md) を参照します。
CLI引数・サブコマンドの一覧は `--help` と [cli.rs](../crates/bmz-player/src/cli.rs) を正とし、ここには重複して列挙しません。

IR Webの環境・DB・検証コマンドは [ir.md](ir.md) の「開発環境と実装の入口」を参照します。
