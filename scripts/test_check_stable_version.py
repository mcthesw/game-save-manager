import unittest

from check_stable_version import numeric_version


class StableVersionTests(unittest.TestCase):
    def test_numeric_stable_version(self):
        self.assertLess(numeric_version("1.9.1"), numeric_version("1.9.2"))

    def test_prerelease_and_nightly_tags_are_rejected(self):
        for value in ("1.9.1-rc.1", "nightly", "v1.9.1", "1.9"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                numeric_version(value)


if __name__ == "__main__":
    unittest.main()
