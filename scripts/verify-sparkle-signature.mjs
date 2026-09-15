import { createPublicKey, verify } from 'node:crypto'
import { readFile } from 'node:fs/promises'
import path from 'node:path'
import { pathToFileURL } from 'node:url'

function decodeBase64(value, size) {
  const text = value?.trim()
  const bytes = Buffer.from(text || '', 'base64')
  if (bytes.length !== size || bytes.toString('base64') !== text)
    throw new Error('Invalid Sparkle signature or public key')
  return bytes
}

export function verifySparkleSignature(archive, signature, publicKey) {
  const rawKey = decodeBase64(publicKey, 32)
  const key = createPublicKey({
    key: Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), rawKey]),
    format: 'der',
    type: 'spki',
  })
  if (!verify(null, archive, key, decodeBase64(signature, 64)))
    throw new Error('Sparkle archive signature does not match the embedded public key')
}

async function main() {
  const [archivePath, signaturePath, ...extra] = process.argv.slice(2)
  if (!archivePath || !signaturePath || extra.length)
    throw new Error('Usage: verify-sparkle-signature.mjs ARCHIVE SIGNATURE_FILE')
  verifySparkleSignature(
    await readFile(archivePath),
    await readFile(signaturePath, 'utf8'),
    process.env.BMZ_SPARKLE_PUBLIC_KEY,
  )
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href)
  main().catch((error) => {
    console.error(error.message)
    process.exitCode = 1
  })
