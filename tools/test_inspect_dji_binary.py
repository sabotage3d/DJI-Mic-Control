"""Deterministic tests using synthetic data, not proprietary DJI firmware."""
import io
from pathlib import Path
import tempfile
import unittest
import zipfile
from contextlib import redirect_stdout
from importlib.machinery import SourceFileLoader

scanner = SourceFileLoader("dji_fw_inspector", str(
    Path(__file__).resolve().parent / "inspect_dji_binary.py"
)).load_module()


class FirmwareInspectionTests(unittest.TestCase):
    def test_magic(self):
        self.assertIn("xV4", scanner.file_kind(b"xV4\x00\x00"))
        self.assertIn("ELF", scanner.file_kind(b"\x7fELF"))
        self.assertIn("ZIP", scanner.file_kind(b"PK\x03\x04"))

    def test_matches_ascii_and_utf16le_with_byte_offsets(self):
        raw = b"aaaWirelessbbb" + "standby".encode("utf-16le")
        result = scanner.scan_tokens(raw)
        self.assertEqual(result["wireless"], [3])
        self.assertEqual(result["standby"], [14])
        self.assertNotIn("shutdown", result)

    def test_binary_works_without_usb(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "test.bin"
            p.write_bytes(b"IM*H" + b"test-rf_disable-foo")
            output = io.StringIO()
            with redirect_stdout(output):
                self.assertEqual(scanner.main([str(p)]), 0)
            self.assertIn("rf_disable@", output.getvalue())
            self.assertIn("No USB communication", output.getvalue())

    def test_zip_finds_strings_in_member(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "test.apk"
            with zipfile.ZipFile(p, "w") as z:
                z.writestr("lib/arm64-v8a/libexample.so", b"\x7fELF" + b"standby")
                z.writestr("res/noise.bin", b"\x01\x02\x03")
            output = io.StringIO()
            with redirect_stdout(output):
                self.assertEqual(scanner.main([str(p)]), 0)
            self.assertIn("libexample.so", output.getvalue())
            self.assertIn("standby@", output.getvalue())


if __name__ == "__main__":
    unittest.main()
