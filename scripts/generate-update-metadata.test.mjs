import assert from 'node:assert/strict'
import { generateKeyPairSync, verify } from 'node:crypto'
import { mkdtemp, mkdir, writeFile, rm } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import test from 'node:test'
import {
  packageManifest,
  releaseManifest,
  signManifest,
  validateVersion,
} from './generate-update-metadata.mjs'

for (const layout of ['legacy', 'grouped'])
  test(`${layout} package excludes user state and records helper with its own hash`, async () => {
    const root = await mkdtemp(path.join(os.tmpdir(), 'bmz-package-test-'))
    try {
      await writeFile(path.join(root, 'bmz-player.exe'), 'player')
      const helper = layout === 'legacy' ? 'bmz-updater.exe' : 'updater/bmz-updater.exe'
      await mkdir(path.dirname(path.join(root, helper)), { recursive: true })
      await writeFile(path.join(root, helper), 'updater')
      const manifest = await packageManifest(root, 'portable', 'windows-x64', '0.5.0', layout)
      assert.equal(manifest.files.length, 2)
      assert.equal(manifest.files.find((file) => file.path === helper).sha256.length, 64)
      assert.equal(manifest.min_updater_protocol, layout === 'legacy' ? 1 : 2)
      const installer = await packageManifest(root, 'installer', 'windows-x64', '0.5.0', layout)
      assert.equal(installer.kind, 'installer')
      await mkdir(path.join(root, 'data'))
      await assert.rejects(
        packageManifest(root, 'portable', 'windows-x64', '0.5.0', layout),
        /unmanaged/,
      )
    } finally {
      await rm(root, { recursive: true, force: true })
    }
  })

test('grouped packages reject recovery records and old root helper files', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'bmz-updater-package-test-'))
  try {
    await mkdir(path.join(root, 'updater'))
    await writeFile(path.join(root, 'bmz-player.exe'), 'player')
    await writeFile(path.join(root, 'updater/bmz-updater.exe'), 'helper')
    await writeFile(path.join(root, 'updater/instance.lock'), '')
    for (const file of [
      'updater/active.json',
      'updater/job-old',
      'updater/update.lock',
      'bmz-updater.exe',
    ]) {
      await writeFile(path.join(root, file), 'must not ship')
      await assert.rejects(packageManifest(root, 'portable', 'windows-x64', '0.5.0'), /unmanaged/)
      await rm(path.join(root, file))
    }
    await writeFile(path.join(root, 'updater/instance.lock'), 'not empty')
    await assert.rejects(packageManifest(root, 'portable', 'windows-x64', '0.5.0'), /instance lock/)
  } finally {
    await rm(root, { recursive: true, force: true })
  }
})

test('grouped release cannot advertise protocol one or omit its bridge', async () => {
  await assert.rejects(releaseManifest('', '0.5.0', 1), /too old/)
  await assert.rejects(releaseManifest('', '0.5.0', 2), /bridge release/)
})

test('signed metadata binds raw bytes and checks the embedded public key', () => {
  const { privateKey, publicKey } = generateKeyPairSync('ed25519')
  const publicBase64 = publicKey
    .export({ type: 'spki', format: 'der' })
    .subarray(-32)
    .toString('base64')
  const privatePem = privateKey.export({ type: 'pkcs8', format: 'pem' })
  const signed = signManifest({ schema: 1, packages: [] }, privatePem, publicBase64)
  assert(
    verify(
      null,
      Buffer.from(signed.payload, 'base64'),
      publicKey,
      Buffer.from(signed.signature, 'base64'),
    ),
  )
  assert.throws(() => signManifest({}, privatePem, 'wrong'), /does not match/)
  assert.throws(() => validateVersion('../../payload'), /version/)
})

test('Linux release archives do not enter Windows automatic update metadata', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'bmz-release-test-'))
  try {
    for (const suffix of [
      'windows-x64-portable.zip',
      'windows-x64-setup.exe',
      'linux-x64.tar.gz',
      'linux-x64-sources.tar.gz',
    ]) {
      await writeFile(path.join(root, `bmz-player-v0.5.0-${suffix}`), suffix)
    }
    const manifest = await releaseManifest(root, '0.5.0', 2, 'v0.4.9')
    assert.equal(manifest.packages.length, 2)
    assert(manifest.packages.every((entry) => entry.target === 'windows-x64'))
    assert(
      manifest.packages.every(
        (entry) => entry.min_updater_protocol === 2 && entry.bridge_tag === 'v0.4.9',
      ),
    )
    const bridge = await releaseManifest(root, '0.5.0', 1, null, 'legacy')
    assert(
      bridge.packages.every(
        (entry) => entry.min_updater_protocol === 1 && entry.bridge_tag === null,
      ),
    )
  } finally {
    await rm(root, { recursive: true, force: true })
  }
})
