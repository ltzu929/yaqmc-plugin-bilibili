"""Build the YAQMC provider component and package it for local import."""

import json
from pathlib import Path
import subprocess
from tempfile import TemporaryDirectory
import zipfile


def main():
    root = Path(__file__).resolve().parent
    manifest = json.loads((root / "manifest.json").read_text(encoding="utf-8"))
    target = "wasm32-wasip2"
    with TemporaryDirectory(dir=root) as staging:
        metadata = json.loads(subprocess.check_output(
            ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
            cwd=root, encoding="utf-8",
        ))
        package = metadata["packages"][0]
        if package["version"] != manifest["version"]:
            raise ValueError("Cargo and plugin manifest versions differ")
        subprocess.run(["cargo", "build", "--locked", "--release", "--target", target],
                       cwd=root, check=True)
        library = next(t for t in package["targets"] if "cdylib" in t["crate_types"])
        wasm = Path(metadata["target_directory"]) / target / "release" / (library["name"] + ".wasm")
        filename = f'{manifest["id"]}-{manifest["version"]}.yaqmc-plugin'
        staged = Path(staging) / filename
        with zipfile.ZipFile(staged, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            archive.write(root / "manifest.json", "manifest.json")
            archive.write(wasm, manifest["entrypoints"]["component"])
            archive.write(root / "LICENSE", "LICENSE")
            archive.write(root / "NOTICE.md", "NOTICE.md")
        dist = root / "dist"
        dist.mkdir(exist_ok=True)
        print(staged.replace(dist / filename))


if __name__ == "__main__":
    main()
