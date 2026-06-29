#!/usr/bin/env node
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')

function usage() {
  console.error(`Usage:
  node scripts/generate-web-license-report.mjs --metafile PATH --output PATH [--policy PATH]
  node scripts/generate-web-license-report.mjs --all-installed --output PATH [--policy PATH]

Options:
  --metafile PATH     Wrangler/esbuild metafile used to find bundled npm packages.
  --sourcemap-root    Directory used to resolve bundled Nuxt/Nitro .map files.
  --all-installed     Scan all installed node_modules packages instead of a metafile.
  --output PATH       Report output path.
  --policy PATH       Policy JSON path (default: web-license-policy.json).
`)
}

function parseArgs(argv) {
  const args = {
    policy: 'web-license-policy.json',
    allInstalled: false,
  }

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i]
    switch (arg) {
      case '--metafile':
        i += 1
        args.metafile = argv[i]
        break
      case '--output':
        i += 1
        args.output = argv[i]
        break
      case '--sourcemap-root':
        i += 1
        args.sourcemapRoot = argv[i]
        break
      case '--policy':
        i += 1
        args.policy = argv[i]
        break
      case '--all-installed':
        args.allInstalled = true
        break
      case '-h':
      case '--help':
        usage()
        process.exit(0)
        break
      default:
        throw new Error(`unknown argument: ${arg}`)
    }
  }

  if (args.metafile && args.allInstalled) {
    throw new Error('--metafile and --all-installed are mutually exclusive')
  }
  if (!args.metafile && !args.allInstalled) {
    throw new Error('pass --metafile or --all-installed')
  }
  if (!args.output) {
    throw new Error('missing --output')
  }
  return args
}

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'))
}

function normalizePath(value) {
  return value.replaceAll('\\', '/')
}

function packageNameFromNodeModulesPath(input) {
  const parts = normalizePath(input).split('/').filter(Boolean)
  let index = -1
  for (let i = 0; i < parts.length; i += 1) {
    if (parts[i] === 'node_modules') {
      index = i
    }
  }
  if (index < 0) {
    return null
  }

  if (parts[index + 1] === '.pnpm') {
    const nestedIndex = parts.indexOf('node_modules', index + 2)
    if (nestedIndex >= 0) {
      index = nestedIndex
    }
  }

  const first = parts[index + 1]
  if (!first || first === '.bin' || first.startsWith('.')) {
    return null
  }
  if (first.startsWith('@')) {
    const second = parts[index + 2]
    return second ? `${first}/${second}` : null
  }
  return first
}

function findPackageJsonForFile(filePath) {
  let current =
    fs.existsSync(filePath) && fs.statSync(filePath).isDirectory()
      ? filePath
      : path.dirname(filePath)
  while (current && current !== path.dirname(current)) {
    if (path.basename(current) === 'node_modules') {
      return null
    }
    const packageJson = path.join(current, 'package.json')
    if (fs.existsSync(packageJson) && normalizePath(current).includes('/node_modules/')) {
      const manifest = readJson(packageJson)
      if (manifest.name) {
        return packageJson
      }
    }
    current = path.dirname(current)
  }
  return null
}

function packageJsonFromSourcePath(input, relativeToFile, installedIndex) {
  if (normalizePath(input).includes('/node_modules/.')) {
    return null
  }

  if (relativeToFile) {
    const relativePath = path.resolve(path.dirname(relativeToFile), input)
    const packageJson = findPackageJsonForFile(relativePath)
    if (packageJson) {
      return packageJson
    }
  }

  const rootRelativePath = path.resolve(repoRoot, input)
  const packageJson = findPackageJsonForFile(rootRelativePath)
  if (packageJson) {
    return packageJson
  }

  const packageName = packageNameFromNodeModulesPath(input)
  return packageName ? (installedIndex.get(packageName) ?? null) : null
}

function packagesFromMetafile(metafilePath, sourcemapRoot) {
  const meta = readJson(metafilePath)
  const installedIndex = buildInstalledPackageIndex()
  const packageJsonPaths = new Set()
  const missing = new Set()

  for (const input of Object.keys(meta.inputs ?? {})) {
    const packageJson = packageJsonFromSourcePath(input, null, installedIndex)
    if (packageJson) {
      packageJsonPaths.add(packageJson)
    }
  }

  if (sourcemapRoot) {
    const root = path.resolve(repoRoot, sourcemapRoot)
    for (const input of Object.keys(meta.inputs ?? {})) {
      const sourceMapPath = path.join(root, `${input}.map`)
      if (!fs.existsSync(sourceMapPath)) {
        continue
      }
      for (const source of readJson(sourceMapPath).sources ?? []) {
        const packageJson = packageJsonFromSourcePath(source, sourceMapPath, installedIndex)
        if (packageJson) {
          packageJsonPaths.add(packageJson)
          continue
        }
        const packageName = packageNameFromNodeModulesPath(source)
        if (packageName) {
          missing.add(packageName)
        }
      }
    }
  }

  return { packageJsonPaths, missing }
}

function* installedPackageJsons(nodeModulesDir) {
  if (!fs.existsSync(nodeModulesDir)) {
    return
  }

  for (const entry of fs.readdirSync(nodeModulesDir, { withFileTypes: true })) {
    if (!entry.isDirectory() || entry.name === '.bin') {
      continue
    }
    const entryPath = path.join(nodeModulesDir, entry.name)
    if (entry.name.startsWith('@')) {
      for (const scoped of fs.readdirSync(entryPath, { withFileTypes: true })) {
        if (!scoped.isDirectory()) {
          continue
        }
        const packageJson = path.join(entryPath, scoped.name, 'package.json')
        if (fs.existsSync(packageJson)) {
          yield packageJson
        }
      }
      continue
    }

    const packageJson = path.join(entryPath, 'package.json')
    if (fs.existsSync(packageJson)) {
      yield packageJson
    }
  }
}

function buildInstalledPackageIndex() {
  const byName = new Map()
  for (const packageJsonPath of installedPackageJsons(path.join(repoRoot, 'node_modules'))) {
    const manifest = readJson(packageJsonPath)
    if (manifest.name) {
      byName.set(manifest.name, packageJsonPath)
    }
  }
  return byName
}

function packageJsonPathsFromInstalled() {
  return { packageJsonPaths: new Set(buildInstalledPackageIndex().values()), missing: new Set() }
}

function manifestLicense(manifest) {
  if (typeof manifest.license === 'string' && manifest.license.trim()) {
    return manifest.license.trim()
  }
  if (Array.isArray(manifest.licenses) && manifest.licenses.length > 0) {
    return manifest.licenses
      .map((license) => {
        if (typeof license === 'string') {
          return license
        }
        return license?.type ?? ''
      })
      .filter(Boolean)
      .join(' OR ')
  }
  return 'NOASSERTION'
}

function licenseFileNames(packageRoot) {
  const names = fs.readdirSync(packageRoot)
  return names
    .filter((name) => /^(licen[cs]e|notice|copying|third[-_ ]?party)/i.test(name))
    .filter((name) => fs.statSync(path.join(packageRoot, name)).isFile())
    .sort((a, b) => a.localeCompare(b))
}

function readLicenseFiles(packageRoot) {
  return licenseFileNames(packageRoot).map((name) => ({
    name,
    text: fs.readFileSync(path.join(packageRoot, name), 'utf8').replace(/\s+$/u, ''),
  }))
}

function stripOuterParens(value) {
  let out = value.trim()
  while (out.startsWith('(') && out.endsWith(')')) {
    out = out.slice(1, -1).trim()
  }
  return out
}

function resolveLicenseExpression(expression, policy) {
  const accepted = new Set(policy.accepted ?? [])
  const expr = stripOuterParens(expression)
  if (accepted.has(expr)) {
    return { selected: expr, ok: true }
  }

  const orParts = expr.split(/\s+OR\s+/u).map(stripOuterParens)
  if (orParts.length > 1) {
    const selected = orParts.find((part) => accepted.has(part))
    return selected ? { selected, ok: true } : { selected: expr, ok: false }
  }

  const andParts = expr.split(/\s+AND\s+/u).map(stripOuterParens)
  if (andParts.length > 1 && andParts.every((part) => accepted.has(part))) {
    return { selected: andParts.join(' AND '), ok: true }
  }

  return { selected: expr, ok: false }
}

function isReviewLicense(expression, policy) {
  return (policy.reviewLicenses ?? []).some((pattern) => new RegExp(pattern, 'iu').test(expression))
}

function packageDetails(packageJsonPaths, missingPackages, policy) {
  const packages = []

  for (const packageJsonPath of [...packageJsonPaths].sort((a, b) => a.localeCompare(b))) {
    const manifest = readJson(packageJsonPath)
    const overrideKey = `${manifest.name}@${manifest.version}`
    const declaredLicense = manifestLicense(manifest)
    const effectiveLicense =
      policy.licenseOverrides?.[overrideKey] ??
      policy.licenseOverrides?.[manifest.name] ??
      declaredLicense
    const licenseNote =
      policy.licenseNotes?.[overrideKey] ?? policy.licenseNotes?.[manifest.name] ?? ''
    const license = resolveLicenseExpression(effectiveLicense, policy)
    const review = isReviewLicense(effectiveLicense, policy) && !license.ok
    const packageRoot = path.dirname(packageJsonPath)

    packages.push({
      name: manifest.name,
      version: manifest.version ?? '',
      declaredLicense,
      effectiveLicense,
      licenseNote,
      selectedLicense: license.selected,
      ok: license.ok && !review,
      review,
      repository: repositoryUrl(manifest.repository),
      homepage: manifest.homepage ?? '',
      packageRoot: path.relative(repoRoot, packageRoot),
      licenseFiles: readLicenseFiles(packageRoot),
    })
  }

  packages.sort(
    (a, b) =>
      a.name.localeCompare(b.name) ||
      a.version.localeCompare(b.version) ||
      a.packageRoot.localeCompare(b.packageRoot),
  )

  return { packages, missing: [...missingPackages].sort((a, b) => a.localeCompare(b)) }
}

function repositoryUrl(repository) {
  if (!repository) {
    return ''
  }
  if (typeof repository === 'string') {
    return repository
  }
  return repository.url ?? ''
}

function licenseSummary(packages) {
  const counts = new Map()
  for (const pkg of packages) {
    counts.set(pkg.selectedLicense, (counts.get(pkg.selectedLicense) ?? 0) + 1)
  }
  return [...counts.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
}

function renderReport({ packages, missing, source, policyPath }) {
  const lines = []
  lines.push('BMZ IR Web Dependency Licenses')
  lines.push('================================')
  lines.push('')
  lines.push('This report is generated from the Worker bundle metafile or installed')
  lines.push('node_modules package metadata. It is not legal advice.')
  lines.push('')
  lines.push(`Source: ${source}`)
  lines.push(`Policy: ${path.relative(repoRoot, policyPath)}`)
  lines.push(`Packages: ${packages.length}`)
  lines.push('')
  lines.push('License Summary')
  lines.push('---------------')
  lines.push('')
  for (const [license, count] of licenseSummary(packages)) {
    lines.push(`- ${license}: ${count}`)
  }
  lines.push('')
  lines.push('Packages')
  lines.push('--------')

  for (const pkg of packages) {
    lines.push('')
    lines.push(`${pkg.name} ${pkg.version}`)
    lines.push('-'.repeat(Math.min(`${pkg.name} ${pkg.version}`.length, 80)))
    lines.push(`Declared license: ${pkg.declaredLicense}`)
    if (pkg.effectiveLicense !== pkg.declaredLicense) {
      lines.push(`Policy-selected license: ${pkg.effectiveLicense}`)
    }
    lines.push(`Resolved license: ${pkg.selectedLicense}`)
    if (pkg.licenseNote) {
      lines.push(`License note: ${pkg.licenseNote}`)
    }
    if (pkg.repository) {
      lines.push(`Repository: ${pkg.repository}`)
    }
    if (pkg.homepage) {
      lines.push(`Homepage: ${pkg.homepage}`)
    }
    lines.push(`Installed package root: ${pkg.packageRoot}`)
    if (pkg.licenseFiles.length === 0) {
      lines.push('')
      lines.push('No license file was found in the installed package root.')
      continue
    }

    for (const file of pkg.licenseFiles) {
      lines.push('')
      lines.push(`--- ${file.name} ---`)
      lines.push(file.text)
    }
  }

  if (missing.length > 0) {
    lines.push('')
    lines.push('Packages referenced by the metafile but not found in node_modules')
    lines.push('---------------------------------------------------------------')
    for (const name of missing) {
      lines.push(`- ${name}`)
    }
  }

  lines.push('')
  return `${lines.join('\n')}\n`
}

function main() {
  try {
    const args = parseArgs(process.argv.slice(2))
    const policyPath = path.resolve(repoRoot, args.policy)
    const policy = readJson(policyPath)
    const packageSource = args.allInstalled
      ? packageJsonPathsFromInstalled()
      : packagesFromMetafile(path.resolve(args.metafile), args.sourcemapRoot)
    const source = args.allInstalled
      ? 'all installed node_modules packages'
      : `${path.relative(repoRoot, path.resolve(args.metafile))}${
          args.sourcemapRoot ? ` with sourcemaps from ${args.sourcemapRoot}` : ''
        }`
    const details = packageDetails(packageSource.packageJsonPaths, packageSource.missing, policy)
    const failed = details.packages.filter((pkg) => !pkg.ok)

    if (failed.length > 0 || details.missing.length > 0) {
      for (const pkg of failed) {
        console.error(
          `license review required: ${pkg.name}@${pkg.version} declared=${pkg.declaredLicense} selected=${pkg.selectedLicense}`,
        )
      }
      for (const name of details.missing) {
        console.error(
          `package referenced by bundle metafile but not found in node_modules: ${name}`,
        )
      }
      process.exitCode = 1
      return
    }

    const outputPath = path.resolve(repoRoot, args.output)
    fs.mkdirSync(path.dirname(outputPath), { recursive: true })
    fs.writeFileSync(outputPath, renderReport({ ...details, source, policyPath }))
    console.log(
      `wrote ${path.relative(repoRoot, outputPath)} (${details.packages.length} packages)`,
    )
  } catch (error) {
    console.error(`error: ${error.message}`)
    usage()
    process.exitCode = 1
  }
}

main()
