import { describe, expect, test } from 'bun:test'

type Messages = Record<string, unknown>

const globals = globalThis as typeof globalThis & {
  defineI18nLocale: <T>(loader: () => T) => T
}
globals.defineI18nLocale = <T>(loader: () => T) => loader()

const catalogs = Object.fromEntries(
  await Promise.all(
    ['ja', 'en', 'ko', 'zh-CN', 'zh-TW', 'zh-HK'].map(async (locale) => {
      const module = await import(`./${locale}.ts`)
      return [locale, module.default as Messages]
    }),
  ),
) as Record<string, Messages>

function flatten(messages: Messages, prefix = ''): Record<string, string> {
  return Object.fromEntries(
    Object.entries(messages).flatMap(([key, value]) => {
      const path = prefix ? `${prefix}.${key}` : key
      return typeof value === 'string'
        ? [[path, value]]
        : value && typeof value === 'object'
          ? Object.entries(flatten(value as Messages, path))
          : []
    }),
  )
}

function placeholders(message: string): string[] {
  return [...message.matchAll(/\{(?<name>[A-Za-z][A-Za-z0-9_]*)\}/gu)]
    .map((match) => match.groups?.name ?? '')
    .filter(Boolean)
    .sort()
}

describe('i18n catalogs', () => {
  const reference = flatten(catalogs.ja!)

  for (const [locale, messages] of Object.entries(catalogs)) {
    test(`${locale} has the same keys and placeholders as ja`, () => {
      const flattened = flatten(messages)
      expect(Object.keys(flattened).sort()).toEqual(Object.keys(reference).sort())

      for (const [key, referenceMessage] of Object.entries(reference)) {
        expect(placeholders(flattened[key] ?? '')).toEqual(placeholders(referenceMessage))
      }
    })
  }

  test('all static app translation keys exist in the catalogs', async () => {
    const usedKeys = new Set<string>()
    const glob = new Bun.Glob('**/*.{ts,vue}')

    for await (const file of glob.scan({ cwd: 'bmz-ir-web/app', absolute: true })) {
      const source = await Bun.file(file).text()
      for (const match of source.matchAll(/\bt\(\s*['"](?<key>[^'"]+)['"]/gu)) {
        if (match.groups?.key) usedKeys.add(match.groups.key)
      }
    }

    expect([...usedKeys].filter((key) => !(key in reference))).toEqual([])
  })
})
