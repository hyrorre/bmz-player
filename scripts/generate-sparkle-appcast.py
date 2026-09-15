"""Generate/merge static Sparkle feeds after signed, notarized archives are uploaded."""
import argparse
import base64
import re
from pathlib import Path
import xml.etree.ElementTree as ET

NAMESPACE = "http://www.andymatuschak.org/xml-namespaces/sparkle"
ET.register_namespace("sparkle", NAMESPACE)


def bundle_version(version):
    match = re.fullmatch(r"(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-(alpha|beta|rc)\.([1-9]\d*))?", version)
    if not match:
        raise ValueError("Unsupported release version")
    major, minor, patch, kind, number = match.groups()
    core = f"{major}.{minor}.{patch}"
    if kind:
        if int(number) > 255:
            raise ValueError("Apple bundle prerelease number must be <= 255")
        return core + {"alpha": "a", "beta": "b", "rc": "fc"}[kind] + number
    return core


def generate(version, arch, signature, size, previous=None):
    internal = bundle_version(version)
    if arch not in ("x64", "arm64") or len(base64.b64decode(signature, validate=True)) != 64 or size <= 0:
        raise ValueError("Invalid signed archive metadata")
    if previous and Path(previous).exists():
        tree = ET.parse(previous)
        channel = tree.getroot().find("channel")
        if channel is None:
            raise ValueError("Invalid previous feed")
    else:
        tree = ET.ElementTree(ET.Element("rss", {"version": "2.0"}))
        channel = ET.SubElement(tree.getroot(), "channel")
        ET.SubElement(channel, "title").text = "BMZ Player"
    # Preserve older compatible versions for OS floors and delayed release jobs.
    for item in list(channel.findall("item")):
        if item.findtext(f"{{{NAMESPACE}}}version") == internal:
            channel.remove(item)
    item = ET.SubElement(channel, "item")
    ET.SubElement(item, "title").text = f"BMZ Player {version}"
    ET.SubElement(item, f"{{{NAMESPACE}}}version").text = internal
    ET.SubElement(item, f"{{{NAMESPACE}}}shortVersionString").text = version
    ET.SubElement(item, f"{{{NAMESPACE}}}minimumSystemVersion").text = "11.0" if arch == "arm64" else "10.13"
    ET.SubElement(item, "link").text = f"https://github.com/hyrorre/bmz-player/releases/tag/v{version}"
    ET.SubElement(item, "enclosure", {
        "url": f"https://github.com/hyrorre/bmz-player/releases/download/v{version}/bmz-player-v{version}-macos-{arch}.app.zip",
        "length": str(size), "type": "application/octet-stream", f"{{{NAMESPACE}}}edSignature": signature,
    })
    return tree


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", required=True)
    parser.add_argument("--bundle-version-only", action="store_true")
    parser.add_argument("--arch")
    parser.add_argument("--signature")
    parser.add_argument("--size", type=int)
    parser.add_argument("--previous")
    parser.add_argument("--output")
    args = parser.parse_args()
    if args.bundle_version_only:
        print(bundle_version(args.version))
    else:
        generate(args.version, args.arch, args.signature, args.size, args.previous).write(args.output, encoding="utf-8", xml_declaration=True)
