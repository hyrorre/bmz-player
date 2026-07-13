import type { DifficultyTableSource } from '../services/difficulty_tables'

const GOOGLE_APPS_SCRIPT_ORIGINS = [
  'https://script.google.com',
  'https://script.googleusercontent.com',
] as const

const PMS_DATABASE_ORIGINS = [
  'https://pmsdatabase.github.io',
  'https://pmsdatabase-isr.vercel.app',
  ...GOOGLE_APPS_SCRIPT_ORIGINS,
] as const

const SOURCE_CONFIGS: readonly Omit<DifficultyTableSource, 'priority'>[] = [
  'https://darksabun.club/table/archive/normal1/',
  'https://darksabun.club/table/archive/insane1/',
  {
    sourceUrl: 'https://rattoto10.jounin.jp/table.html',
    allowedOrigins: ['https://rattoto10.github.io'],
  },
  {
    sourceUrl: 'https://rattoto10.jounin.jp/table_insane.html',
    allowedOrigins: ['https://rattoto10.github.io'],
  },
  {
    sourceUrl: 'https://rattoto10.jounin.jp/table_overjoy.html',
    allowedOrigins: ['https://rattoto10.github.io'],
  },
  'https://stellabms.xyz/st/table.html',
  'https://stellabms.xyz/sl/table.html',
  'https://stellabms.xyz/so/table.html',
  'https://stellabms.xyz/sn/table.html',
  {
    sourceUrl: 'https://mplwtch.github.io/Solomon/',
    allowedOrigins: GOOGLE_APPS_SCRIPT_ORIGINS,
  },
  {
    sourceUrl: 'https://monibms.github.io/Dystopia/dystopia.html',
    allowedOrigins: GOOGLE_APPS_SCRIPT_ORIGINS,
  },
  {
    sourceUrl: 'https://mocha-repository.info/table/ln_header.json',
    allowedOrigins: ['http://flowermaster.web.fc2.com'],
  },
  {
    sourceUrl: 'https://ladymade-star.github.io/luminous/',
    allowedOrigins: GOOGLE_APPS_SCRIPT_ORIGINS,
  },
  'http://minddnim.web.fc2.com/sara/3rd_hard/bms_sara_3rd_hard.html',
  {
    sourceUrl: 'https://egret9.github.io/Scramble/',
    allowedOrigins: GOOGLE_APPS_SCRIPT_ORIGINS,
  },
  'https://deltabms.yaruki0.net/table/data/dpdelta_head.json',
  'https://deltabms.yaruki0.net/table/data/insane_head.json',
  'https://stellabms.xyz/dpst/table.html',
  'https://stellabms.xyz/dp/table.html',
  {
    sourceUrl: 'https://pmsdifficulty.xxxxxxxx.jp/PMSdifficulty.html',
    allowedOrigins: PMS_DATABASE_ORIGINS,
  },
  {
    sourceUrl: 'https://pmsdifficulty.xxxxxxxx.jp/insane_PMSdifficulty.html',
    allowedOrigins: PMS_DATABASE_ORIGINS,
  },
  {
    sourceUrl: 'https://pmsdifficulty.xxxxxxxx.jp/_pastoral_insane_table.html',
    allowedOrigins: PMS_DATABASE_ORIGINS,
  },
  {
    sourceUrl: 'https://pmsdifficulty.xxxxxxxx.jp/_pastoral_upper.html',
    allowedOrigins: PMS_DATABASE_ORIGINS,
  },
  {
    sourceUrl: 'https://hibyethere.github.io/table/',
    allowedOrigins: ['https://asumatoki.kr'],
  },
  'https://classmaterma.github.io/4UE/table.html',
  'https://classmaterma.github.io/UE/table.html',
  'https://classmaterma.github.io/8UE/table.html',
].map((source) => (typeof source === 'string' ? { sourceUrl: source } : source))

/** IR運用者が取得を許可した難易度表。順序は表示優先度としても使う。 */
export const DIFFICULTY_TABLE_SOURCES: readonly DifficultyTableSource[] = SOURCE_CONFIGS.map(
  (source, priority) => ({ ...source, priority }),
)
