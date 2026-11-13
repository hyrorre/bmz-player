# 書庫内動画の世代テストとFFmpegの色変換差

`delayed_audio_image_and_video_decodes_keep_the_loaded_archive_generation`は、同じZIPを更新した後、別スレッドで旧世代と新世代の音声・画像・動画を開き、動画を再度開いても同じ世代が使われることを検査する。世代の保持については[書庫内譜面の直接再生](../../docs/archive-songs.md#資源と保存先)を参照。

macOS arm64のHomebrew FFmpeg 9.0.2（libswscale 10.1.102）では、YUV420Pの新世代フレーム（Y=235、U=V=128）をRGBAへ変換すると、期待値`[255, 255, 255, 255]`に対して`[253, 253, 253, 255]`になった。書庫を介さず同じY4Mを`ffmpeg`の`scale=flags=fast_bilinear,format=rgba`へ渡しても再現し、`-cpuflags 0`でも同値だった。旧世代（Y=16）は`[0, 0, 0, 255]`だった。FFmpeg 9.0.1を使った[v0.5.2のLinux配布ジョブ](https://github.com/hyrorre/bmz-player/actions/runs/43477270245/job/132064652078)では修正前のテストが通過している。

テスト内で書庫と同じY4Mを通常ファイルにも保存し、使用中のFFmpegで変換した結果を期待値にする。旧・新の期待フレームが異なることを確認し、書庫からの遅延読み込みと再度の読み込みで、両世代の寸法と全RGBAバイトを照合する。アプリのデコード処理や色変換設定に変更はない。

macOS arm64・Rust 1.100.0でfmt、`bmz-player`のcheckとall-targets Clippy、書庫リソースのテスト12件が通過した。関連3クレートのテスト実行も成功した。`bmz-player`のlibは3,225件成功・66件無視、`bmz-video`は49件成功・2件無視、`bmz-ffmpeg`にテストケースはない。修正後のLinux・Windows実行は未確認。
