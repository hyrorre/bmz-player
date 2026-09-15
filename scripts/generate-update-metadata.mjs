import { createHash, createPrivateKey, createPublicKey, sign, verify } from 'node:crypto'
import { createReadStream } from 'node:fs'
import { lstat, readdir, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { pathToFileURL } from 'node:url'

export async function hashFile(file) {
  const hash = createHash('sha256')
  for await (const chunk of createReadStream(file)) hash.update(chunk)
  return hash.digest('hex')
}

export function validateVersion(version) {
  if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-(alpha|beta|rc)\.[1-9]\d*)?$/.test(version))
    throw new Error('Expected a release version or alpha.N / beta.N / rc.N prerelease')
  return version
}

export async function packageManifest(root, kind, target, version) {
  validateVersion(version)
  if (
    !['portable', 'installer'].includes(kind) ||
    !['windows-x64', 'windows-arm64'].includes(target)
  )
    throw new Error('Invalid package kind/target')
  const files = []
  async function walk(relative = '') {
    for (const name of (await readdir(path.join(root, relative))).sort()) {
      const file = relative ? `${relative}/${name}` : name
      if (file === 'bmz-package.json') continue
      if (file === '.bmz-instance.lock') {
        const lock = await lstat(path.join(root, file))
        if (!lock.isFile() || lock.isSymbolicLink() || lock.size !== 0)
          throw new Error('Invalid instance lock')
        continue
      }
      if (
        file !== 'resources' &&
        !file.startsWith('resources/') &&
        !['bmz-player.exe', 'bmz-updater.exe'].includes(file) &&
        !/^[^/]+\.dll$/i.test(file)
      )
        throw new Error(`Refusing to package user/unmanaged file: ${file}`)
      const absolute = path.join(root, file)
      const info = await lstat(absolute)
      if (info.isSymbolicLink()) throw new Error(`Symlink in Windows package: ${file}`)
      if (info.isDirectory()) await walk(file)
      else if (info.isFile())
        files.push({ path: file, size: info.size, sha256: await hashFile(absolute) })
      else throw new Error(`Special file: ${file}`)
    }
  }
  await walk()
  for (const required of ['bmz-player.exe', 'bmz-updater.exe'])
    if (!files.some((file) => file.path === required)) throw new Error(`Missing ${required}`)
  return { schema: 1, kind, target, version, min_updater_protocol: 1, files }
}

export function signManifest(manifest, privateKey, expectedPublicKey) {
  const key = createPrivateKey(privateKey)
  if (key.asymmetricKeyType !== 'ed25519')
    throw new Error('Expected an Ed25519 private key (PKCS8 PEM)')
  const pub = createPublicKey(key)
  const rawPublic = pub.export({ type: 'spki', format: 'der' }).subarray(-32).toString('base64')
  if (rawPublic !== expectedPublicKey)
    throw new Error('Signing key does not match the embedded public key')
  const payload = Buffer.from(JSON.stringify(manifest))
  const signature = sign(null, payload, key)
  if (!verify(null, payload, pub, signature)) throw new Error('Signature self-check failed')
  return { payload: payload.toString('base64'), signature: signature.toString('base64') }
}

export async function releaseManifest(directory, version, minProtocol = 1, bridge = null) {
  validateVersion(version)
  if (!Number.isSafeInteger(minProtocol) || minProtocol < 1)
    throw new Error('Invalid updater protocol')
  if (bridge !== null) validateVersion(bridge.replace(/^v/, ''))
  if (minProtocol > 1 && !bridge)
    throw new Error('A bridge release is required for a new updater protocol')
  const packages = []
  for (const kind of ['portable', 'installer']) {
    const name = `bmz-player-v${version}-windows-x64-${kind === 'portable' ? 'portable.zip' : 'setup.exe'}`
    const file = path.join(directory, name)
    const info = await lstat(file)
    packages.push({
      version,
      target: 'windows-x64',
      kind,
      min_updater_protocol: minProtocol,
      bridge_tag: bridge,
      name,
      url: `https://github.com/hyrorre/bmz-player/releases/download/v${version}/${name}`,
      size: info.size,
      sha256: await hashFile(file),
    })
  }
  return { schema: 1, packages }
}

async function main() {
  const [mode, ...args] = process.argv.slice(2)
  if (mode === 'package' && args.length === 4) {
    const [root, kind, target, version] = args
    await writeFile(path.join(root, '.bmz-instance.lock'), '', { flag: 'a' })
    await writeFile(
      path.join(root, 'bmz-package.json'),
      JSON.stringify(await packageManifest(root, kind, target, version), null, 2) + '\n',
    )
  } else if (mode === 'release' && args.length === 2) {
    const [directory, version] = args
    const manifest = await releaseManifest(
      directory,
      version,
      Number(process.env.BMZ_MIN_UPDATER_PROTOCOL || 1),
      process.env.BMZ_UPDATE_BRIDGE_TAG || null,
    )
    const key = process.env.BMZ_UPDATE_PRIVATE_KEY
    if (!key || !process.env.BMZ_UPDATE_PUBLIC_KEY)
      throw new Error('Update signing keys are required')
    await writeFile(
      path.join(directory, 'updates.json'),
      JSON.stringify(signManifest(manifest, key, process.env.BMZ_UPDATE_PUBLIC_KEY)) + '\n',
    )
  } else throw new Error('Usage: package ROOT KIND TARGET VERSION | release DIRECTORY VERSION')
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href)
  main().catch((error) => {
    console.error(error.message)
    process.exitCode = 1
  })
