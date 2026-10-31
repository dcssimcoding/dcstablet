import importlib.util
import tempfile
import unittest
import zipfile
from pathlib import Path


spec = importlib.util.spec_from_file_location(
    "release_privacy", Path(__file__).resolve().parents[1] / "scripts/Check-ReleasePrivacy.py"
)
privacy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(privacy)


class ReleasePrivacyTests(unittest.TestCase):
    def test_private_paths_in_binary_encodings(self):
        for value in (
            r"C:\Users\example\.cargo\registry\src\dependency.rs",
            r"C:\\Users\\example\\project",
            "C:/Users/example/project",
            "/home/example/project",
            "/Users/example/project",
            "/mnt/c/Users/example/project",
        ):
            for encoding in ("utf-8", "utf-16le", "utf-16be"):
                for prefix in (b"", b"\x00"):
                    with self.subTest(value=value, encoding=encoding, prefix=prefix):
                        self.assertTrue(privacy.contains_user_path(prefix + value.encode(encoding)))

    def test_neutral_build_paths_and_urls_are_allowed(self):
        self.assertFalse(privacy.contains_user_path(
            b"/build/.cargo/registry/src/dependency.rs https://github.com/example/project "
            b"C:\\Program Files\\Application\\app.exe src/main.rs"
        ))

    def test_zip_checks_every_member_and_debug_symbols(self):
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / "release.zip"
            for filename, data in (
                ("app.exe", b"prefix\x00" + "/home/example/project".encode("utf-16le")),
                ("BUILD-INFO.json", b'{"path":"C:\\\\Users\\\\example"}'),
                ("app.pdb", b"symbols"),
                ("C:/Users/example/file.txt", b"neutral contents"),
            ):
                with self.subTest(filename=filename):
                    with zipfile.ZipFile(archive, "w") as zipped:
                        zipped.writestr("README.md", b"neutral contents")
                        zipped.writestr(filename, data)
                    with self.assertRaises(ValueError):
                        privacy.check_file(archive)
            with zipfile.ZipFile(archive, "w") as zipped:
                zipped.writestr("app.exe", b"/build/src/main.rs")
            privacy.check_file(archive)


if __name__ == "__main__":
    unittest.main()
