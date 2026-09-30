# Frame pacingとノーツ投影の実測・検証

2026-09-08の計測と、その後の投影変更・検証結果を記録します。
現在の待機・投影仕様と診断手順は [frame-pacing.md](../../docs/frame-pacing.md) を参照してください。

## 2026-09-08 の実測

Windows、RTX 5090、DX12、3840x2160 Native、borderless、実効Immediate、frame latency 1。
同梱sample / default skin / autoplayをreleaseで実行。修正前は計測追加のみの
`c6ad9245`、修正後は`0a1150cf`。PresentMon 2.5.1を双方で併用した。
GPUに余裕がある環境での短時間測定であり、GTX 1660等の報告環境の再現ではない。

| FPS制限 | 描画開始間隔の誤差 avg 前→後 | 同 p99 前→後 | PresentMon表示間隔 p99 前→後 |
|---|---:|---:|---:|
| 120 | 220 → 27 us | 730 → 417 us | 9.774 → 9.531 ms |
| 240 | 201 → 46 us | 686 → 454 us | 5.709 → 5.319 ms |

CPU cadenceはgameplay snapshot consume開始から1秒を除外した約14秒、
PresentMonはIndependent Flipの先頭2秒と末尾1秒を除外した約12秒を集計。
表示間隔はtearing有効のIndependent Flip更新間隔であり、完全な1枚のscanoutを
数えた値ではない。小さい表示側の差は反復測定や別環境での確認が必要。
120FPSの実行全体のCPU時間は4.25→3.75秒、240FPSは5.55→6.95秒だった。
起動・ロードも含むためCPU時間の減少を最適化の成果と解釈しない。

報告された持続的な30/60FPS相当のカクつきは、この条件では修正前にも再現しなかった。
この測定時点ではsnapshot ageが120FPSで約7ms、240FPSで約2.8ms残っており、
snapshotの時刻に固定されたノーツ位置には別の周期の揺れが残っていた。
この表はCPU待機処理の比較であり、gameplay分離前の `e974998e` に対する
ノーツの滑らかさの改善を示すものではない。

## 投影変更の実測

`604ff9d2` と `301c6b79` のrelease buildを、同じ
`target/release/bmz-player.exe` パスで比較した。Windows / RTX 5090 / DX12 / 4K Native /
borderless / Immediate / frame latency 1、同梱sample・default skin・autoplay。
各runでPresentMonを併用し、Playのsnapshot sampleの先頭・末尾1秒を除外した約13秒を集計。

| FPS制限 | 描画間隔と投影時刻差のずれ p99 前→後 | 理想周期と投影時刻差のずれ p99 前→後 |
|---|---:|---:|
| 120 | 2091 → 203 us | 1990 → 420 us |
| 240 | 2147 → 182 us | 2114 → 474 us |

前者は `abs(snapshot時刻の差分 - CPU描画開始間隔)`、後者は
`abs(snapshot時刻の差分 - 1/FPS)`。描画開始とclock取得は異なる位置なので、前者にも
描画開始後のCPU処理時間の揺れが含まれる。修正後のpublication ageは平均約7.17ms / 2.85ms
だが、ノーツ座標の時刻にはその遅れを持ち込まない。判定状態を先読みするものではない。
この計測はCPU側の描画時刻の追従を示すもので、GPU/presentや物理scanoutのstall解消を
示すものではない。

同じDX12/4K条件のECFNスキンでも、VSync（実効Fifo）とFastVSync（実効Mailbox）の
Unlimited設定でsample autoplayをResultまで実行した。5秒ごとの集計では投影後ageと
同一時刻の繰り返しはいずれも0だった。FPS半減の調査はこの修正の対象外。
判定・replay仕様を変えて描画の不連続を隠さない。

Vulkanでも同条件の最終ビルドを計測し、描画間隔の誤差は120FPSでavg 30us / p99 405us、
240FPSでavg 48us / p99 442usだった。Vulkanの修正前比較は行っていない。

## 検証結果

実行結果: `cargo fmt --check`、`cargo clippy --workspace --all-targets --features experimental-gameinput`
成功、`cargo test --workspace` は3250 passed / 7 ignored。
追加histogramで既存のWinitApp stack上限テストが失敗したため、統計を作成時に確保する
Boxへ移して修正した。再実行中に既存file loggerテストが一度だけファイル数不一致で
失敗した。PIDを再利用した一時ディレクトリに8/27のログが残り、今回のログと2個になった
ことを確認した。単独実行と最後のworkspace全体実行は成功した。
