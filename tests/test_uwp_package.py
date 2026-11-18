import tempfile
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path

from tools import uwp_package

NS = {"m": "http://schemas.microsoft.com/appx/manifest/foundation/windows10"}


class UwpPackageTests(unittest.TestCase):
    def test_manifest_names_the_signer_and_version(self):
        text = uwp_package.render_manifest(
            uwp_package.TEMPLATE.read_text(encoding="utf-8"), 'CN=A "B" & C', "1.2.3.4"
        )
        identity = ET.fromstring(text).find("m:Identity", NS)
        self.assertEqual(identity.get("Publisher"), 'CN=A "B" & C')
        self.assertEqual(identity.get("Version"), "1.2.3.4")
        for bad in ("1.2.3", "1.2.3.70000", "1.2.x.4"):
            with self.assertRaises(ValueError):
                uwp_package.render_manifest("", "CN=A", bad)
        with self.assertRaises(ValueError):
            uwp_package.render_manifest("", "Someone", "1.0.0.0")

    def test_layout_holds_what_the_manifest_names(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            exe = tmp / "horizon-client.exe"
            exe.write_bytes(b"MZ")
            resources = tmp / "prepared"
            resources.mkdir()
            (resources / "horizon-resources.json").write_text("{}")
            manifest = uwp_package.render_manifest(
                uwp_package.TEMPLATE.read_text(encoding="utf-8"), "CN=Test", "0.1.0.0"
            )
            layout = tmp / "layout"
            uwp_package.lay_out(exe, resources, layout, manifest, None)
            root = ET.fromstring((layout / "AppxManifest.xml").read_text(encoding="utf-8"))
            named = [root.find("m:Properties/m:Logo", NS).text]
            app = root.find("m:Applications/m:Application", NS)
            self.assertTrue((layout / app.get("Executable")).is_file())
            for element in app.iter():
                named += [v for k, v in element.attrib.items() if v.endswith(".png")]
            for path in named:
                self.assertTrue((layout / path.replace("\\", "/")).is_file(), path)
            self.assertTrue((layout / "resources" / "horizon-resources.json").is_file())

    def test_resources_zip_has_the_manifest_at_its_root(self):
        import zipfile

        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            resources = tmp / "26.3"
            (resources / "assets").mkdir(parents=True)
            (resources / "horizon-resources.json").write_text("{}")
            (resources / "assets" / "lang.json").write_text("lang")
            out = tmp / "out" / "resources.zip"
            uwp_package.zip_resources(resources, out)
            names = zipfile.ZipFile(out).namelist()
            self.assertEqual(sorted(names), ["assets/lang.json", "horizon-resources.json"])


if __name__ == "__main__":
    unittest.main()
