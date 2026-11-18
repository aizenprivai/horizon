#!/usr/bin/env python3
"""Build Horizon as a UWP app and lay out (and, on Windows, pack and sign) its package, for the
Xbox's Dev Mode. See docs/xbox-uwp.md.

    python3 tools/uwp_package.py                  # build, lay out .local/uwp/layout, pack if it can
    python3 tools/uwp_package.py --no-build       # lay out an exe built before
    python3 tools/uwp_package.py --pfx dev.pfx    # also sign the package with that certificate
    python3 tools/uwp_package.py --no-resources   # a package with no game files: a first test
    python3 tools/uwp_package.py --zip-resources  # .local/uwp/resources.zip, to upload on its own

On Windows it packs with the SDK's makeappx and signs with signtool. Elsewhere it packs with
Microsoft's open-source makemsix (github.com/microsoft/msix-packaging, `--makemsix`) and signs with
osslsigncode.

Unless `--no-resources`, the layout holds the game's prepared resources (`--resources`, by default
`.local/horizon-client/26.3`), which come from the player's own Minecraft install: the package is
for your own console only. Never share it or commit it (`.local/` is ignored).
"""

from __future__ import annotations

import argparse
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TARGET = "x86_64-uwp-windows-msvc"
TEMPLATE = ROOT / "tools" / "uwp" / "AppxManifest.xml"
ICON = ROOT / "docs" / "media" / "brand" / "icon-1024.png"
# The package's images: name, width, height.
ASSETS = [
    ("Square44x44Logo.png", 44, 44),
    ("Square150x150Logo.png", 150, 150),
    ("Wide310x150Logo.png", 310, 150),
    ("StoreLogo.png", 50, 50),
    ("SplashScreen.png", 620, 300),
]


def render_manifest(template: str, publisher: str, version: str) -> str:
    """The manifest with its publisher and version. The version is four numbers (a.b.c.d, each up
    to 65535), as the package format requires; the publisher a certificate subject (`CN=...`)."""
    parts = version.split(".")
    if len(parts) != 4 or not all(p.isdigit() and int(p) <= 65535 for p in parts):
        raise ValueError(f"package version {version!r} is not four numbers a.b.c.d")
    if not publisher.startswith("CN="):
        raise ValueError(f"publisher {publisher!r} is not a certificate subject (CN=...)")
    escaped = (
        publisher.replace("&", "&amp;").replace('"', "&quot;").replace("<", "&lt;")
    )
    return template.replace("{publisher}", escaped).replace("{version}", version)


def build(profile: str) -> Path:
    """Builds the game for UWP and returns the executable. Needs nightly Rust with `rust-src`
    (the target has no prebuilt standard library) and, for the link, Visual Studio's UWP tools."""
    command = [
        "cargo", "+nightly", "build", "-Zbuild-std=std,panic_abort",
        "--target", TARGET, "--profile", profile,
        "-p", "horizon-client", "--bin", "horizon-client",
    ]
    print("+", " ".join(command), flush=True)
    subprocess.run(command, cwd=ROOT, check=True)
    return exe_path(profile)


def exe_path(profile: str) -> Path:
    target_dir = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    folder = "debug" if profile == "dev" else profile
    return target_dir / TARGET / folder / "horizon-client.exe"


def make_assets(icon: Path, dest: Path) -> None:
    """The tile, logo and splash images from Horizon's icon, centred on black."""
    from PIL import Image  # Pillow: only this step needs it.

    dest.mkdir(parents=True, exist_ok=True)
    source = Image.open(icon).convert("RGBA")
    for name, width, height in ASSETS:
        side = min(width, height)
        mark = source.resize((side, side), Image.LANCZOS)
        image = Image.new("RGBA", (width, height), (0, 0, 0, 255))
        image.alpha_composite(mark, ((width - side) // 2, (height - side) // 2))
        image.save(dest / name)


def lay_out(
    exe: Path, resources: Path | None, layout: Path, manifest: str, dxc: Path | None
) -> None:
    """The package's folder: the executable, its resources beside it (where the game looks first,
    launch_options.rs), the images and the manifest. With no resources the game opens its
    no-resources screen, which still starts the window, the renderer and the input."""
    if layout.exists():
        shutil.rmtree(layout)
    layout.mkdir(parents=True)
    shutil.copy2(exe, layout / exe.name)
    if dxc is not None:
        for dll in ("dxcompiler.dll", "dxil.dll"):
            if (dxc / dll).is_file():
                shutil.copy2(dxc / dll, layout / dll)
    if resources is not None:
        if not (resources / "horizon-resources.json").is_file():
            sys.exit(
                f"{resources} holds no prepared resources (horizon-resources.json); "
                "run horizon-prepare first (docs/native-client.md), or pass --no-resources"
            )
        shutil.copytree(resources, layout / "resources")
    make_assets(ICON, layout / "Assets")
    (layout / "AppxManifest.xml").write_text(manifest, encoding="utf-8")


def zip_resources(resources: Path, out: Path) -> None:
    """The prepared resources as one zip with `horizon-resources.json` at its root, for the
    game's LocalState (crates/horizon-client/src/resources_zip.rs): Device Portal uploads one
    file at a time. Like the package, it is for your own console only."""
    import zipfile

    if not (resources / "horizon-resources.json").is_file():
        sys.exit(f"{resources} holds no prepared resources (horizon-resources.json)")
    out.parent.mkdir(parents=True, exist_ok=True)
    partial = out.with_name(out.name + ".partial")
    with zipfile.ZipFile(partial, "w", zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(resources.rglob("*")):
            if path.is_file():
                archive.write(path, path.relative_to(resources).as_posix())
    partial.replace(out)
    print(f"Wrote {out}: upload it to the app's LocalState folder with Device Portal")


def windows_sdk_tool(name: str) -> Path | None:
    """`makeappx.exe` or `signtool.exe` from the newest Windows 10/11 SDK, or the PATH's."""
    found = shutil.which(name)
    if found:
        return Path(found)
    kits = Path(os.environ.get("ProgramFiles(x86)", r"C:\Program Files (x86)")) / "Windows Kits/10/bin"
    if not kits.is_dir():
        return None
    versions = sorted(
        (p for p in kits.iterdir() if re.fullmatch(r"10\.0\.\d+\.\d+", p.name)),
        key=lambda p: tuple(int(n) for n in p.name.split(".")),
    )
    for version in reversed(versions):
        tool = version / "x64" / name
        if tool.is_file():
            return tool
    return None


def pack(
    layout: Path,
    package: Path,
    pfx: Path | None,
    password: str | None,
    makemsix: Path | None = None,
) -> bool:
    makeappx = windows_sdk_tool("makeappx.exe")
    if makeappx is None:
        return pack_elsewhere(layout, package, pfx, password, makemsix)
    subprocess.run([str(makeappx), "pack", "/d", str(layout), "/p", str(package), "/o"], check=True)
    if pfx is None:
        print(f"Packed {package}, unsigned: Dev Mode installs only signed packages (--pfx).")
        return True
    signtool = windows_sdk_tool("signtool.exe")
    if signtool is None:
        sys.exit("No signtool.exe (Windows SDK) to sign the package with")
    command = [str(signtool), "sign", "/fd", "SHA256", "/f", str(pfx)]
    if password:
        command += ["/p", password]
    subprocess.run(command + [str(package)], check=True)
    print(f"Packed and signed {package}")
    return True


def pack_elsewhere(
    layout: Path, package: Path, pfx: Path | None, password: str | None, makemsix: Path | None
) -> bool:
    """Packs with makemsix and signs with osslsigncode, for a machine without the Windows SDK."""
    tool = makemsix or (Path(found) if (found := shutil.which("makemsix")) else None)
    if tool is None:
        print(
            "No makeappx.exe (Windows SDK) or makemsix (--makemsix): the layout is ready; pack it\n"
            f"on Windows with  makeappx pack /d {layout} /p {package} /o"
        )
        return False
    unsigned = package.with_name(package.stem + ".unsigned.appx") if pfx else package
    unsigned.unlink(missing_ok=True)
    subprocess.run([str(tool), "pack", "-d", str(layout), "-p", str(unsigned)], check=True)
    if pfx is None:
        print(f"Packed {package}, unsigned: Dev Mode installs only signed packages (--pfx).")
        return True
    signer = shutil.which("osslsigncode")
    if signer is None:
        sys.exit("No osslsigncode to sign the package with")
    command = [signer, "sign", "-pkcs12", str(pfx), "-h", "sha256"]
    if password:
        command += ["-pass", password]
    package.unlink(missing_ok=True)
    subprocess.run(command + ["-in", str(unsigned), "-out", str(package)], check=True)
    unsigned.unlink()
    print(f"Packed and signed {package}")
    return True


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--profile", default="fast", help="cargo profile (default: fast)")
    parser.add_argument("--no-build", action="store_true", help="use the exe already built")
    parser.add_argument(
        "--resources",
        type=Path,
        default=ROOT / ".local" / "horizon-client" / "26.3",
        help="prepared resources (default: .local/horizon-client/26.3)",
    )
    parser.add_argument("--out", type=Path, default=ROOT / ".local" / "uwp", help="output folder")
    parser.add_argument(
        "--publisher",
        default="CN=HorizonDev",
        help="the signing certificate's subject (default: CN=HorizonDev)",
    )
    parser.add_argument("--version", default="0.1.0.0", help="package version a.b.c.d")
    parser.add_argument("--dxc", type=Path, help="folder with dxcompiler.dll and dxil.dll to ship")
    parser.add_argument("--pfx", type=Path, help="certificate to sign the package with")
    parser.add_argument("--pfx-password", help="the certificate's password")
    parser.add_argument(
        "--no-resources", action="store_true", help="leave the game's prepared resources out"
    )
    parser.add_argument("--makemsix", type=Path, help="makemsix, where the Windows SDK is missing")
    parser.add_argument(
        "--zip-resources",
        action="store_true",
        help="only write <out>/resources.zip from --resources, for the game's LocalState",
    )
    args = parser.parse_args()

    if args.zip_resources:
        zip_resources(args.resources, args.out / "resources.zip")
        return

    exe = exe_path(args.profile) if args.no_build else build(args.profile)
    if not exe.is_file():
        sys.exit(f"no executable at {exe}")
    manifest = render_manifest(TEMPLATE.read_text(encoding="utf-8"), args.publisher, args.version)
    layout = args.out / "layout"
    resources = None if args.no_resources else args.resources
    lay_out(exe, resources, layout, manifest, args.dxc)
    print(f"Laid out {layout}")
    package = args.out / f"Horizon_{args.version}_x64.appx"
    pack(layout, package, args.pfx, args.pfx_password, args.makemsix)


if __name__ == "__main__":
    main()
