"""Download the exact Ubuntu sources for the libraries in the package."""

from collections import defaultdict
import subprocess


def download_sources(packages, source_root):
    sources = defaultdict(list)
    versions = defaultdict(set)
    for package in packages:
        source = package["source_package"]
        version = package["source_version"]
        sources[(source, version)].append(package)
        versions[source].add(version)
        print(
            f"Ubuntu library {package['library']}: "
            f"{package['binary_package']}={package['binary_version']} -> {source}={version}",
            flush=True,
        )

    # The archive and its verifier expect exactly one .dsc per source name.
    for source, found in sorted(versions.items()):
        if len(found) > 1:
            details = "; ".join(
                f"{p['library']} ({p['binary_package']}={p['binary_version']}) -> "
                f"{p['source_version']}"
                for version in sorted(found) for p in sources[(source, version)]
            )
            raise ValueError(f"Multiple versions of Ubuntu source {source}: {details}")

    for (source, version), libraries in sorted(sources.items()):
        directory = source_root / "ubuntu" / source
        directory.mkdir(parents=True)
        try:
            subprocess.run(
                ["apt-get", "source", "--download-only", f"{source}={version}"],
                cwd=directory, check=True,
            )
        except (subprocess.CalledProcessError, OSError) as error:
            details = ", ".join(
                f"{p['library']} ({p['binary_package']}={p['binary_version']})"
                for p in libraries
            )
            raise RuntimeError(
                f"Failed to download exact Ubuntu source {source}={version} "
                f"for {details}: {error}"
            ) from error
