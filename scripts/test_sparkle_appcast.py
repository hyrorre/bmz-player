import base64
import importlib.util
from pathlib import Path
import tempfile
import unittest
import sys

sys.dont_write_bytecode = True

spec = importlib.util.spec_from_file_location("appcast", Path(__file__).with_name("generate-sparkle-appcast.py"))
appcast = importlib.util.module_from_spec(spec)
spec.loader.exec_module(appcast)


class AppcastTests(unittest.TestCase):
    def test_version_maps_prereleases_to_apple_bundle_versions(self):
        self.assertEqual(appcast.bundle_version("0.5.0-rc.2"), "0.5.0fc2")
        self.assertEqual(appcast.bundle_version("0.5.0"), "0.5.0")
        with self.assertRaises(ValueError):
            appcast.bundle_version("../bad")

    def test_feed_preserves_history_and_replaces_only_same_version(self):
        signature = base64.b64encode(bytes(64)).decode()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "feed.xml"
            appcast.generate("0.5.0", "x64", signature, 10).write(path)
            appcast.generate("0.6.0", "x64", signature, 20, path).write(path)
            feed = appcast.generate("0.5.0", "x64", signature, 30, path)
            items = feed.getroot().findall("channel/item")
            self.assertEqual(len(items), 2)
            self.assertEqual(items[-1].find("enclosure").get("length"), "30")
            self.assertEqual(items[-1].findtext(f"{{{appcast.NAMESPACE}}}minimumSystemVersion"), "10.13")


if __name__ == "__main__":
    unittest.main()
