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

type DifficultyTableConfig = Omit<DifficultyTableSource, 'priority'> & { id: string }

// id is the public ID used by URLs and API responses.
const SOURCE_CONFIGS: readonly DifficultyTableConfig[] = [
  { id: 'normal', sourceUrl: 'https://darksabun.club/table/archive/normal1/' },
  { id: 'insane', sourceUrl: 'https://darksabun.club/table/archive/insane1/' },
  {
    id: 'normal2',
    sourceUrl: 'https://rattoto10.jounin.jp/table.html',
    allowedOrigins: ['https://rattoto10.github.io'],
  },
  {
    id: 'insane2',
    sourceUrl: 'https://rattoto10.jounin.jp/table_insane.html',
    allowedOrigins: ['https://rattoto10.github.io'],
  },
  {
    id: 'overjoy3',
    sourceUrl: 'https://rattoto10.jounin.jp/table_overjoy.html',
    allowedOrigins: ['https://rattoto10.github.io'],
  },
  { id: 'stella', sourceUrl: 'https://stellabms.xyz/st/table.html' },
  { id: 'satellite', sourceUrl: 'https://stellabms.xyz/sl/table.html' },
  { id: 'solar', sourceUrl: 'https://stellabms.xyz/so/table.html' },
  { id: 'supernova', sourceUrl: 'https://stellabms.xyz/sn/table.html' },
  {
    id: 'solomon',
    sourceUrl: 'https://mplwtch.github.io/Solomon/',
    allowedOrigins: GOOGLE_APPS_SCRIPT_ORIGINS,
  },
  {
    id: 'dystopia',
    sourceUrl: 'https://monibms.github.io/Dystopia/dystopia.html',
    allowedOrigins: GOOGLE_APPS_SCRIPT_ORIGINS,
  },
  {
    id: 'ln',
    sourceUrl: 'https://mocha-repository.info/table/ln_header.json',
    allowedOrigins: ['http://flowermaster.web.fc2.com'],
  },
  {
    id: 'luminous',
    sourceUrl: 'https://ladymade-star.github.io/luminous/',
    allowedOrigins: GOOGLE_APPS_SCRIPT_ORIGINS,
  },
  { id: 'scratch', sourceUrl: 'http://minddnim.web.fc2.com/sara/3rd_hard/bms_sara_3rd_hard.html' },
  {
    id: 'scramble',
    sourceUrl: 'https://egret9.github.io/Scramble/',
    allowedOrigins: GOOGLE_APPS_SCRIPT_ORIGINS,
  },
  { id: 'dp-delta', sourceUrl: 'https://deltabms.yaruki0.net/table/data/dpdelta_head.json' },
  { id: 'dp-insane', sourceUrl: 'https://deltabms.yaruki0.net/table/data/insane_head.json' },
  { id: 'dp-stella', sourceUrl: 'https://stellabms.xyz/dpst/table.html' },
  { id: 'dp-satellite', sourceUrl: 'https://stellabms.xyz/dp/table.html' },
  {
    id: 'pms-db',
    sourceUrl: 'https://pmsdifficulty.xxxxxxxx.jp/PMSdifficulty.html',
    allowedOrigins: PMS_DATABASE_ORIGINS,
  },
  {
    id: 'pms-db-insane',
    sourceUrl: 'https://pmsdifficulty.xxxxxxxx.jp/insane_PMSdifficulty.html',
    allowedOrigins: PMS_DATABASE_ORIGINS,
  },
  {
    id: 'pms-insane',
    sourceUrl: 'https://pmsdifficulty.xxxxxxxx.jp/_pastoral_insane_table.html',
    allowedOrigins: PMS_DATABASE_ORIGINS,
  },
  {
    id: 'pastoral',
    sourceUrl: 'https://pmsdifficulty.xxxxxxxx.jp/_pastoral_upper.html',
    allowedOrigins: PMS_DATABASE_ORIGINS,
    requestTimeoutMs: 30_000,
  },
  {
    id: '5aery',
    sourceUrl: 'https://hibyethere.github.io/table/',
    allowedOrigins: ['https://asumatoki.kr'],
  },
  { id: '4ue', sourceUrl: 'https://classmaterma.github.io/4UE/table.html' },
  { id: '6ue', sourceUrl: 'https://classmaterma.github.io/UE/table.html' },
  { id: '8ue', sourceUrl: 'https://classmaterma.github.io/8UE/table.html' },
]

/** IR運用者が取得を許可した難易度表。順序は表示優先度としても使う。 */
export const DIFFICULTY_TABLE_SOURCES: readonly (DifficultyTableConfig & { priority: number })[] =
  SOURCE_CONFIGS.map((source, priority) => ({ ...source, priority }))
