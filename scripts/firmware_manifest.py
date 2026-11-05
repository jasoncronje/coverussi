"""Collect a firmware-rs build for the browser flasher and the Releases page.

    python3 scripts/firmware_manifest.py firmware-rs/target/xtensa-esp32s3-espidf/release out/firmware --commit abc1234

Run it with ESP-IDF's environment (IDF_PATH, esptool). Writes the images a
fresh board needs (bootloader, partition table, blank OTA data, app) and
manifest.json, which the flasher page and `scripts/esp32-rs.sh flash` read.
They're kept as separate parts, not one merged image, so flashing an update
leaves the NVS partition (Wi-Fi, Home Assistant, settings) alone.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import os
import re
import subprocess
import sys
from datetime import UTC, datetime
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CRATE = ROOT / "firmware-rs"
FLASH = ["--flash_mode", "dio", "--flash_freq", "80m", "--flash_size", "32MB"]  # QIO switches on after boot


def partitions() -> dict[str, int]:
    rows = (r for r in csv.reader((CRATE / "partitions.csv").read_text().splitlines()) if r and not r[0].startswith("#"))
    return {r[0].strip(): int(r[3].strip(), 16) for r in rows}


def version() -> str:
    m = re.search(r'^CONFIG_APP_PROJECT_VER="(.+)"$', (CRATE / "sdkconfig.defaults").read_text(), re.M)
    return m.group(1) if m else "0.0.0"


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("build", type=Path, help="cargo's release directory for the board")
    p.add_argument("out", type=Path, help="where the images and manifest.json go")
    p.add_argument("--commit", default="", help="the git commit, shown beside the version")
    a = p.parse_args()
    idf = Path(os.environ.get("IDF_PATH") or sys.exit("IDF_PATH isn't set: run this with ESP-IDF's environment."))

    # The newest esp-idf-sys build, after a flags change leaves the old one behind.
    idf_builds = sorted(a.build.glob("build/esp-idf-sys-*/out/build"), key=lambda d: d.stat().st_mtime)
    if not idf_builds:
        sys.exit(f"No esp-idf-sys build in {a.build}; build firmware-rs first.")
    a.out.mkdir(parents=True, exist_ok=True)
    table = partitions()

    app = a.out / "coverussi.bin"
    subprocess.run(["esptool.py", "--chip", "esp32s3", "elf2image", *FLASH, "-o", app, a.build / "coverussi"],
                   check=True, stdout=subprocess.DEVNULL)
    subprocess.run([sys.executable, idf / "components/partition_table/gen_esp32part.py", "--flash-size", "32MB",
                    CRATE / "partitions.csv", a.out / "partition-table.bin"], check=True, stdout=subprocess.DEVNULL)
    (a.out / "bootloader.bin").write_bytes((idf_builds[-1] / "bootloader/bootloader.bin").read_bytes())
    # Blank OTA data boots ota_0, even if an earlier install had moved to ota_1.
    (a.out / "ota_data_initial.bin").write_bytes(b"\xff" * 0x2000)

    parts = []
    for name, offset in [("bootloader.bin", 0x0), ("partition-table.bin", 0x8000),
                         ("ota_data_initial.bin", table["otadata"]), (app.name, table["ota_0"])]:
        data = (a.out / name).read_bytes()
        parts.append({"path": name, "offset": offset, "size": len(data), "sha256": hashlib.sha256(data).hexdigest()})

    manifest = {
        "name": "Coverussi",
        "version": version(),
        "commit": a.commit[:7],
        "built": datetime.now(UTC).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "chip": "esp32s3",
        "app": app.name,  # the image the web app's firmware update takes
        "parts": parts,
    }
    (a.out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps(manifest, indent=2))


if __name__ == "__main__":
    main()
