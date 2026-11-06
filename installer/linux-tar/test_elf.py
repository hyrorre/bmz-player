"""Negative controls for ELF version requirements and provider parsing."""

import unittest

from elf import definitions, glibc_max, version_needs


class ElfTests(unittest.TestCase):
    def test_needs_excludes_definitions_and_retains_providers(self):
        text = """Version definition section '.gnu.version_d' contains 1 entry:
  Name: GLIBCXX_3.4.99
Version needs section '.gnu.version_r' contains 2 entries:
  File: libc.so.6  Cnt: 2
    Name: GLIBC_2.28
    Name: GLIBC_2.2.5
  File: libstdc++.so.6  Cnt: 2
    Name: GLIBCXX_3.4.25
    Name: CXXABI_1.3.11
"""
        needs = version_needs(text)
        self.assertEqual(needs["libstdc++.so.6"], {"GLIBCXX_3.4.25", "CXXABI_1.3.11"})
        self.assertEqual(glibc_max(needs), (2, 28))
        self.assertEqual(definitions(text), {"GLIBCXX_3.4.99"})

    def test_later_or_private_glibc_fails(self):
        for version in ("2.29", "2.35", "2.100", "PRIVATE", "ABI_DT_RELR"):
            with self.subTest(version=version), self.assertRaises(ValueError):
                glibc_max({"libc.so.6": {"GLIBC_" + version}})


if __name__ == "__main__":
    unittest.main()
