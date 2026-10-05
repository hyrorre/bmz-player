import assert from 'node:assert/strict'
import { createPrivateKey, createPublicKey, generateKeyPairSync, verify } from 'node:crypto'
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import test from 'node:test'
import {
  artifacts,
  builds,
  canonicalJson,
  generateRelease,
  legacyUpdates,
  signRelease,
  writeReleaseMetadata,
} from './generate-release-metadata.mjs'
import { hashFile } from './generate-update-metadata.mjs'

const version = '0.5.0',
  commit = 'a'.repeat(40)
async function fixture(t) {
  const root = await mkdtemp(path.join(os.tmpdir(), 'bmz-release-'))
  t.after(() => rm(root, { recursive: true, force: true }))
  for (const [target] of builds)
    await writeFile(
      path.join(root, `${target}-client-manifest.json`),
      JSON.stringify({
        schema_version: 1,
        client: 'bmz-player',
        version,
        git_commit: commit,
        target,
        executable: target === 'windows-x64' ? 'bmz-player.exe' : 'bmz-player',
        client_hash: 'b'.repeat(64),
      }),
    )
  for (const [suffix] of artifacts)
    await writeFile(path.join(root, `bmz-player-v${version}-${suffix}`), suffix)
  return root
}

test('one inventory produces signed readable metadata, legacy updates and matching checksums', async (t) => {
  const root = await fixture(t)
  const manifest = await generateRelease(root, version, commit, { bridge: 'v0.4.3' })
  const keys = generateKeyPairSync('ed25519')
  const publicKey = keys.publicKey
    .export({ type: 'spki', format: 'der' })
    .subarray(-32)
    .toString('base64')
  const release = await writeReleaseMetadata(root, manifest, {
    privateKey: keys.privateKey.export({ type: 'pkcs8', format: 'pem' }),
    publicKey,
  })
  assert.equal(release.builds.length, 5)
  assert.equal(release.artifacts.length, 7)
  assert(
    verify(
      null,
      Buffer.from(canonicalJson(manifest)),
      keys.publicKey,
      Buffer.from(release.signature, 'base64'),
    ),
  )
  const old = JSON.parse(await readFile(path.join(root, 'updates.json'), 'utf8'))
  assert(
    verify(
      null,
      Buffer.from(old.payload, 'base64'),
      keys.publicKey,
      Buffer.from(old.signature, 'base64'),
    ),
  )
  assert.deepEqual(JSON.parse(Buffer.from(old.payload, 'base64')), legacyUpdates(manifest))
  assert(
    legacyUpdates(manifest).packages.every(
      (pkg) => pkg.min_updater_protocol === 2 && pkg.bridge_tag === 'v0.4.3',
    ),
  )
  const sums = await readFile(path.join(root, 'SHA256SUMS.txt'), 'utf8')
  assert.equal(sums.trim().split('\n').length, 8)
  for (const line of sums.trim().split('\n')) {
    const [hash, name] = line.split('  ')
    assert.equal(hash, await hashFile(path.join(root, name)))
  }
  assert(!sums.includes('client-manifest'))
})

test('missing builds/artifacts or a different resolved commit/version stop publication', async (t) => {
  const root = await fixture(t),
    options = { bridge: 'v0.4.3' }
  await assert.rejects(generateRelease(root, version, 'c'.repeat(40), options), /resolved release/)
  await assert.rejects(generateRelease(root, '0.5.1', commit, options), /resolved release/)
  const file = path.join(root, `bmz-player-v${version}-linux-x64-sources.tar.gz`)
  await writeFile(file, '')
  await assert.rejects(generateRelease(root, version, commit, options), /Invalid release artifact/)
  await rm(path.join(root, 'linux-x64-tar-client-manifest.json'))
  await assert.rejects(generateRelease(root, version, commit, options), /targets do not match/)
})

test('bridge protocol is independent from helper capability and rejects invalid transitions', async (t) => {
  const root = await fixture(t)
  await assert.rejects(generateRelease(root, version, commit), /bridge release/)
  await assert.rejects(generateRelease(root, version, commit, { bridge: 'v0.5.0' }), /older/)
  await assert.rejects(generateRelease(root, version, commit, { bridge: 'v0.6.0' }), /older/)
  await assert.rejects(
    generateRelease(root, version, commit, { bridge: 'v0.4.3', minProtocol: 1 }),
    /protocol/,
  )
  const bridge = await generateRelease(root, version, commit, { bridge: 'v0.4.3', minProtocol: 2 })
  const later = await generateRelease(root, version, commit, { bridge: 'v0.4.4', minProtocol: 3 })
  assert.equal(legacyUpdates(bridge).packages[0].min_updater_protocol, 2)
  assert.equal(legacyUpdates(later).packages[0].min_updater_protocol, 3)
  await assert.rejects(
    generateRelease(root, version, commit, { bridge: 'v0.5.0-rc.1' }),
    /stable bridge/,
  )
})

test('dry run produces explicitly unsigned release metadata without update instructions for old clients', async (t) => {
  const root = await fixture(t)
  const manifest = await generateRelease(root, version, commit, { unsigned: true })
  const release = await writeReleaseMetadata(root, manifest, { unsigned: true })
  assert.equal(release.signature, null)
  await assert.rejects(readFile(path.join(root, 'updates.json')), /ENOENT/)
})

test('canonicalization is independent of key order/whitespace and rejects unsupported numbers/Unicode', () => {
  assert.equal(
    canonicalJson({ z: '\u000f日本語', a: [true, null, 123] }),
    '{"a":[true,null,123],"z":"\\u000f日本語"}',
  )
  assert.equal(canonicalJson({ '\ue000': 2, '😀': 1 }), '{"😀":1,"":2}')
  for (const value of [1.5, -1, -0, Number.MAX_SAFE_INTEGER + 1, Infinity, '\ud800'])
    assert.throws(() => canonicalJson(value))
})

test('Node and Rust share a deterministic signed fixture (public test key only)', async () => {
  const fixture = JSON.parse(
    await readFile(
      new URL('../crates/bmz-updater/tests/fixtures/release.json', import.meta.url),
      'utf8',
    ),
  )
  const { signature, ...manifest } = fixture
  const privateKey = createPrivateKey({
    key: Buffer.concat([
      Buffer.from('302e020100300506032b657004220420', 'hex'),
      Buffer.alloc(32, 7),
    ]),
    type: 'pkcs8',
    format: 'der',
  })
  const publicKey = createPublicKey(privateKey)
    .export({ type: 'spki', format: 'der' })
    .subarray(-32)
    .toString('base64')
  assert.equal(
    signRelease(manifest, privateKey.export({ type: 'pkcs8', format: 'pem' }), publicKey).signature,
    signature,
  )
})
