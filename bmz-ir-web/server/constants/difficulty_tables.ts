import type { DifficultyTableSource } from '../services/difficulty_tables'

const SOURCE_URLS = [
  'https://darksabun.club/table/archive/normal1/',
  'https://darksabun.club/table/archive/insane1/',
  'https://rattoto10.jounin.jp/table.html',
  'https://rattoto10.jounin.jp/table_insane.html',
  'https://rattoto10.jounin.jp/table_overjoy.html',
  'https://stellabms.xyz/st/table.html',
  'https://stellabms.xyz/sl/table.html',
  'https://stellabms.xyz/so/table.html',
  'https://stellabms.xyz/sn/table.html',
  'https://mplwtch.github.io/Solomon/',
  'https://monibms.github.io/Dystopia/dystopia.html',
  'https://mocha-repository.info/table/ln_header.json',
  'https://ladymade-star.github.io/luminous/',
  'http://minddnim.web.fc2.com/sara/3rd_hard/bms_sara_3rd_hard.html',
  'https://egret9.github.io/Scramble/',
  'https://deltabms.yaruki0.net/table/data/dpdelta_head.json',
  'https://deltabms.yaruki0.net/table/data/insane_head.json',
  'https://stellabms.xyz/dpst/table.html',
  'https://stellabms.xyz/dp/table.html',
  'https://pmsdifficulty.xxxxxxxx.jp/PMSdifficulty.html',
  'https://pmsdifficulty.xxxxxxxx.jp/insane_PMSdifficulty.html',
  'https://pmsdifficulty.xxxxxxxx.jp/_pastoral_insane_table.html',
  'https://pmsdifficulty.xxxxxxxx.jp/_pastoral_upper.html',
  'https://hibyethere.github.io/table/',
  'https://classmaterma.github.io/4UE/table.html',
  'https://classmaterma.github.io/UE/table.html',
  'https://classmaterma.github.io/8UE/table.html',
] as const

/** IR運用者が取得を許可した難易度表。順序は表示優先度としても使う。 */
export const DIFFICULTY_TABLE_SOURCES: readonly DifficultyTableSource[] = SOURCE_URLS.map(
  (sourceUrl, priority) => ({ sourceUrl, priority }),
)
