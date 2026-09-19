#!/usr/bin/env node

import { readdirSync, readFileSync, writeFileSync } from 'node:fs'
import { basename, resolve } from 'node:path'

function fail(message) {
  console.error(`combine-client-hash-manifests: ${message}`)
  process.exit(1)
}

const options = new Map()
for (let index = 2; index < process.argv.length; index += 2) {
  const key = process.argv[index]
  const value = process.argv[index + 1]
  if (!key?.startsWith('--') || value === undefined) {
    fail('expected --input-dir, --output and optionally --expected-targets')
  }
  options.set(key.slice(2), value)
}

for (const required of ['input-dir', 'output']) {
  if (!options.has(required)) fail(`missing --${required}`)
}

const inputDirectory = resolve(options.get('input-dir'))
const output = resolve(options.get('output'))
const outputName = basename(output)
const inputNames = readdirSync(inputDirectory)
  .filter((name) => name.endsWith('-client-manifest.json') && name !== outputName)
  .sort()

if (inputNames.length === 0) {
  fail(`no client hash manifests found in ${inputDirectory}`)
}

const hashPattern = /^[0-9a-f]{64}$/
const commitPattern = /^[0-9a-f]{40}$/
const buildIdentityByTarget = new Map([
  ['windows-x64', { platform: 'windows', arch: 'x86_64', package_kind: 'portable-installer' }],
  ['macos-arm64', { platform: 'macos', arch: 'aarch64', package_kind: 'app' }],
  ['macos-x64', { platform: 'macos', arch: 'x86_64', package_kind: 'app' }],
  ['linux-x64-flatpak', { platform: 'linux', arch: 'x86_64', package_kind: 'flatpak' }],
  ['linux-x64-tar', { platform: 'linux', arch: 'x86_64', package_kind: 'tar' }],
])
const targets = new Set()
let common
const buildsByTarget = inputNames.map((name) => {
  let manifest
  try {
    manifest = JSON.parse(readFileSync(resolve(inputDirectory, name), 'utf8'))
  } catch (error) {
    fail(`${name}: invalid JSON: ${error.message}`)
  }

  if (
    manifest.schema_version !== 1 ||
    manifest.client !== 'bmz-player' ||
    typeof manifest.version !== 'string' ||
    !commitPattern.test(manifest.git_commit ?? '') ||
    typeof manifest.target !== 'string' ||
    typeof manifest.executable !== 'string' ||
    !hashPattern.test(manifest.client_hash ?? '')
  ) {
    fail(`${name}: invalid schema v1 client hash manifest`)
  }

  const identity = {
    client: manifest.client,
    version: manifest.version,
    git_commit: manifest.git_commit,
  }
  if (common === undefined) {
    common = identity
  } else if (
    identity.client !== common.client ||
    identity.version !== common.version ||
    identity.git_commit !== common.git_commit
  ) {
    fail(`${name}: client, version or git_commit does not match the other manifests`)
  }
  if (targets.has(manifest.target)) {
    fail(`${name}: duplicate target ${manifest.target}`)
  }
  targets.add(manifest.target)
  const buildIdentity = buildIdentityByTarget.get(manifest.target)
  if (buildIdentity === undefined) {
    fail(`${name}: unsupported target ${manifest.target}`)
  }

  return [
    manifest.target,
    {
      ...buildIdentity,
      client_hash: manifest.client_hash,
    },
  ]
})

const expectedTargets = (options.get('expected-targets') ?? '')
  .split(',')
  .map((target) => target.trim())
  .filter(Boolean)
  .sort()
const actualTargets = [...targets].sort()
if (
  expectedTargets.length > 0 &&
  (expectedTargets.length !== actualTargets.length ||
    expectedTargets.some((target, index) => target !== actualTargets[index]))
) {
  fail(
    `targets do not match: expected ${expectedTargets.join(', ')}, got ${actualTargets.join(', ')}`,
  )
}

const targetOrder = [...buildIdentityByTarget.keys()]
buildsByTarget.sort(([left], [right]) => targetOrder.indexOf(left) - targetOrder.indexOf(right))
writeFileSync(
  output,
  `${JSON.stringify(
    {
      schema: 'bmz-rianir-client-manifest-v1',
      ...common,
      builds: buildsByTarget.map(([, build]) => build),
    },
    null,
    2,
  )}\n`,
)
