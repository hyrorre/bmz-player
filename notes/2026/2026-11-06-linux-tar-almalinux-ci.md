# AlmaLinux 8.10のLinux tarビルド修正（#78）

[Optional Linux tar.gzの初回実行](https://github.com/hyrorre/bmz-player/actions/runs/42162366948)では、
`64cac09c`の配布用イメージを作る途中でNASM 2.16.03の`autogen.sh`が停止した。
Gitタグのアーカイブから`configure`を生成するにはAutoconf 2.71以上が必要で、AlmaLinux 8.10のパッケージでは条件を満たさなかった。

[NASM 2.16.03の公式リリース](https://www.nasm.us/pub/nasm/releasebuilds/2.16.03/)のソースには`configure`が含まれる。
同じNASMバージョンの公式アーカイブに切り替え、SHA-256を検証してからビルドする。
配布用ビルドとオフライン再ビルドの両方がこのツール構成を使う。
手順は[Linux tar配布](../../docs/linux-tar.md#ビルド段階)を参照。

macOSで公式アーカイブの`configure`、`make`、`make install`が成功し、`nasm -v`で2.16.03を確認した。
配布用スクリプトの回帰テスト26件が成功した。

[修正後の実行](https://github.com/hyrorre/bmz-player/actions/runs/42163946827)は、NASMのビルド、Rustのチェック・テスト、BMZのreleaseビルドを通過した。
ライセンス収集は`lz4-libs-1.8.3-5.el8_10.x86_64`で停止した。公式RPMには`/usr/share/doc/lz4-libs/LICENSE`が入っており、`%doc`として登録されている。
RPMが`%doc`に収録するライセンスも受け付け、対象バージョンにライセンス本文が見つからない場合は失敗させる。

AlmaLinux 8の公式ソースミラー一覧が`http://vault.almalinux.org/`を返すことも確認した。
`lz4`、`systemd`、`zlib`の対応するSRPMを同じホストでHTTPSから取得できるため、このホストのURLをHTTPSに変換して取得する。

[HTTPS対応後の再試行](https://github.com/hyrorre/bmz-player/actions/runs/42167281003/attempts/2)は、全18件のSRPMの取得とアーカイブの整合性検査、AlmaLinux上のELF検査とPulseAudio出力を通過した。
サンプル再生はVulkan初期化で停止し、[wgpu 29の最小再現](https://github.com/hyrorre/bmz-player/actions/runs/42173313675)ではLavapipeが`vkCreateInstance`に`VK_ERROR_INITIALIZATION_FAILED`を返した。
[AlmaLinuxのMesa SRPM](https://vault.almalinux.org/8.10/AppStream/Source/Packages/mesa-23.1.4-4.el8_10.alma.1.src.rpm)に含まれる`lavapipe-disable-env-var.patch`が、`RH_SW_VULKAN`未設定時にこのエラーを返す。
AlmaLinuxの検証用コンテナで`RH_SW_VULKAN=1`を設定する。
