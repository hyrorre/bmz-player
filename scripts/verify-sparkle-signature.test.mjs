import assert from 'node:assert/strict'
import { generateKeyPairSync, sign } from 'node:crypto'
import test from 'node:test'
import { verifySparkleSignature } from './verify-sparkle-signature.mjs'

test('verifies archive bytes against the embedded public key', () => {
  const { privateKey, publicKey } = generateKeyPairSync('ed25519')
  const publicBase64 = publicKey
    .export({ type: 'spki', format: 'der' })
    .subarray(-32)
    .toString('base64')
  const archive = Buffer.from('signed Sparkle archive')
  const signature = sign(null, archive, privateKey).toString('base64')
  verifySparkleSignature(archive, signature + '\n', publicBase64)
  assert.throws(
    () => verifySparkleSignature(Buffer.from('tampered'), signature, publicBase64),
    /does not match/,
  )
  const other = generateKeyPairSync('ed25519')
    .publicKey.export({ type: 'spki', format: 'der' })
    .subarray(-32)
    .toString('base64')
  assert.throws(() => verifySparkleSignature(archive, signature, other), /does not match/)
  for (const bad of [undefined, '', 'invalid', publicBase64 + '!'])
    assert.throws(() => verifySparkleSignature(archive, signature, bad), /Invalid/)
  assert.throws(() => verifySparkleSignature(archive, signature + '!', publicBase64), /Invalid/)
})
