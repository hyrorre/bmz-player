import assert from 'node:assert/strict'
import { generateKeyPairSync, verify } from 'node:crypto'
import { mkdtemp, mkdir, writeFile, rm } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import test from 'node:test'
import { packageManifest, releaseManifest, signManifest, validateVersion } from './generate-update-metadata.mjs'

test('package excludes user state and records helper with its own hash', async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), 'bmz-package-test-'))
  try {
    await writeFile(path.join(root, 'bmz-player.exe'), 'player')
    await writeFile(path.join(root, 'bmz-updater.exe'), 'updater')
    const manifest = await packageManifest(root, 'portable', 'windows-x64', '0.5.0')
    assert.equal(manifest.files.length, 2)
    assert.equal(manifest.files.find((file) => file.path === 'bmz-updater.exe').sha256.length, 64)
    await mkdir(path.join(root, 'data'))
    await assert.rejects(packageManifest(root, 'portable', 'windows-x64', '0.5.0'), /unmanaged/)
  } finally {
    await rm(root, { recursive: true, force: true })
  }
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
    for (const suffix of ['windows-x64-portable.zip', 'windows-x64-setup.exe',
      'linux-x64.tar.gz', 'linux-x64-sources.tar.gz']) {
      await writeFile(path.join(root, `bmz-player-v0.5.0-${suffix}`), suffix)
    }
    const manifest = await releaseManifest(root, '0.5.0')
    assert.equal(manifest.packages.length, 2)
    assert(manifest.packages.every((entry) => entry.target === 'windows-x64'))
  } finally {
    await rm(root, { recursive: true, force: true })
  }
})
