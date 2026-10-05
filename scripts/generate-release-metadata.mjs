import { lstat, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { pathToFileURL } from 'node:url'
import { combineClientHashManifests } from './combine-client-hash-manifests.mjs'
import { hashFile, signBytes, signManifest, validateVersion } from './generate-update-metadata.mjs'

// These identities also define the complete official release inventory.
export const builds = [
  ['windows-x64', 'windows', 'x86_64', 'portable-installer'],
  ['macos-arm64', 'macos', 'aarch64', 'app'],
  ['macos-x64', 'macos', 'x86_64', 'app'],
  ['linux-x64-flatpak', 'linux', 'x86_64', 'flatpak'],
  ['linux-x64-tar', 'linux', 'x86_64', 'tar'],
]
export const artifacts = [
  ['windows-x64-portable.zip', 'windows-x64', 'portable'],
  ['windows-x64-setup.exe', 'windows-x64', 'installer'],
  ['macos-arm64.app.zip', 'macos-arm64', 'app'],
  ['macos-x64.app.zip', 'macos-x64', 'app'],
  ['linux-x64.flatpak', 'linux-x64-flatpak', 'flatpak'],
  ['linux-x64.tar.gz', 'linux-x64-tar', 'tar'],
  ['linux-x64-sources.tar.gz', 'linux-x64-tar', 'sources'],
]

// RFC 8785 on the manifest's integer-only JSON profile. Reject rather than round
// values outside that profile. No JSON.stringify replacer/property whitelist:
// every field other than the top-level signature must be authenticated.
export function canonicalJson(value) {
  if (value === null || typeof value === 'boolean') return JSON.stringify(value)
  if (typeof value === 'number') {
    if (!Number.isSafeInteger(value) || value < 0 || Object.is(value, -0))
      throw new Error('Expected a nonnegative safe integer')
    return JSON.stringify(value)
  }
  if (typeof value === 'string') {
    if (!value.isWellFormed()) throw new Error('Invalid Unicode string')
    return JSON.stringify(value)
  }
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(',')}]`
  if (value && Object.getPrototypeOf(value) === Object.prototype)
    return `{${Object.keys(value)
      .sort()
      .map((key) => `${canonicalJson(key)}:${canonicalJson(value[key])}`)
      .join(',')}}`
  throw new Error('Unsupported JSON value')
}

export function signRelease(manifest, privateKey, publicKey) {
  if ('signature' in manifest) throw new Error('Manifest already has a signature')
  return {
    ...manifest,
    signature: signBytes(Buffer.from(canonicalJson(manifest)), privateKey, publicKey),
  }
}

export async function generateRelease(
  directory,
  version,
  commit,
  { minProtocol = 2, bridge = null, layout = 'grouped', unsigned = false } = {},
) {
  validateVersion(version)
  if (!['grouped', 'legacy'].includes(layout)) throw new Error('Invalid updater layout')
  if (
    !Number.isSafeInteger(minProtocol) ||
    minProtocol < (layout === 'grouped' ? 2 : 1) ||
    minProtocol > 3
  )
    throw new Error('Invalid updater protocol for this release')
  if (bridge !== null) {
    validateVersion(bridge.replace(/^v/, ''))
    // Compare via the same restricted release-version grammar (including prereleases).
    if (compareVersions(bridge.replace(/^v/, ''), version) >= 0)
      throw new Error('Bridge must be older than this release')
    if (!version.includes('-') && bridge.includes('-'))
      throw new Error('Stable release requires a stable bridge')
  }
  if (!unsigned && minProtocol > 1 && !bridge)
    throw new Error('A published bridge release is required')
  const combined = combineClientHashManifests(
    directory,
    builds.map(([id]) => id),
  )
  if (combined.version !== version || combined.git_commit !== commit)
    throw new Error('Client manifests do not match the resolved release version/commit')
  const releaseBuilds = builds.map(([id, platform, arch, package_kind]) => ({
    id,
    ...combined.builds.find(
      (build) =>
        build.platform === platform && build.arch === arch && build.package_kind === package_kind,
    ),
  }))
  const releaseArtifacts = []
  for (const [suffix, build, kind] of artifacts) {
    const name = `bmz-player-v${version}-${suffix}`
    const file = path.join(directory, name)
    const info = await lstat(file)
    if (!info.isFile() || info.isSymbolicLink() || info.size < 1 || info.size >= 2147483648)
      throw new Error(`Invalid release artifact: ${name}`)
    releaseArtifacts.push({
      name,
      build,
      kind,
      url: `https://github.com/hyrorre/bmz-player/releases/download/v${version}/${name}`,
      size: info.size,
      sha256: await hashFile(file),
      ...(build === 'windows-x64'
        ? { update: { min_updater_protocol: minProtocol, bridge_tag: bridge } }
        : {}),
    })
  }
  return {
    schema: 'bmz-release-manifest-v1',
    client: 'bmz-player',
    version,
    git_commit: commit,
    builds: releaseBuilds,
    artifacts: releaseArtifacts,
  }
}

function compareVersions(left, right) {
  const parts = (value) => {
    const [core, pre] = value.split('-')
    const [label, sequence] = (pre ?? '').split('.')
    return [
      ...core.split('.').map(BigInt),
      BigInt(pre ? ['alpha', 'beta', 'rc'].indexOf(label) : 3),
      BigInt(sequence ?? 0),
    ]
  }
  const a = parts(left),
    b = parts(right)
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return a[i] < b[i] ? -1 : 1
  return 0
}

// Compatibility metadata and checksums are projections of exactly the same hashes.
export function legacyUpdates(manifest) {
  return {
    schema: 1,
    packages: manifest.artifacts
      .filter((artifact) => artifact.update)
      .map((artifact) => ({
        version: manifest.version,
        target: artifact.build,
        kind: artifact.kind,
        ...artifact.update,
        name: artifact.name,
        url: artifact.url,
        size: artifact.size,
        sha256: artifact.sha256,
      })),
  }
}

export async function writeReleaseMetadata(
  directory,
  manifest,
  { privateKey, publicKey, unsigned = false } = {},
) {
  const release = unsigned
    ? { ...manifest, signature: null }
    : signRelease(manifest, privateKey, publicKey)
  const releasePath = path.join(directory, 'release.json')
  await writeFile(releasePath, `${JSON.stringify(release, null, 2)}\n`)
  if (!unsigned)
    await writeFile(
      path.join(directory, 'updates.json'),
      `${JSON.stringify(signManifest(legacyUpdates(manifest), privateKey, publicKey))}\n`,
    )
  const sums = manifest.artifacts.map(({ name, sha256 }) => [name, sha256])
  sums.push(['release.json', await hashFile(releasePath)])
  sums.sort(([a], [b]) => a.localeCompare(b, 'en'))
  await writeFile(
    path.join(directory, 'SHA256SUMS.txt'),
    sums.map(([name, hash]) => `${hash}  ${name}\n`).join(''),
  )
  return release
}

async function main() {
  const [directory, version, commit, flag] = process.argv.slice(2)
  if (
    !directory ||
    !version ||
    !commit ||
    (flag && flag !== '--unsigned') ||
    process.argv.length > 6
  )
    throw new Error('Usage: generate-release-metadata.mjs DIRECTORY VERSION COMMIT [--unsigned]')
  const unsigned = flag === '--unsigned'
  const layout = process.env.BMZ_WINDOWS_UPDATER_LAYOUT || 'grouped'
  const manifest = await generateRelease(directory, version, commit, {
    layout,
    unsigned,
    minProtocol: Number(process.env.BMZ_MIN_UPDATER_PROTOCOL || (layout === 'grouped' ? 2 : 1)),
    bridge: process.env.BMZ_UPDATE_BRIDGE_TAG || null,
  })
  await writeReleaseMetadata(directory, manifest, {
    unsigned,
    privateKey: process.env.BMZ_UPDATE_PRIVATE_KEY,
    publicKey: process.env.BMZ_UPDATE_PUBLIC_KEY,
  })
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href)
  main().catch((error) => {
    console.error(error.message)
    process.exitCode = 1
  })
