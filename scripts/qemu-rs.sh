#!/bin/bash
# Boot the QEMU build of firmware-rs in Espressif's QEMU and print its console.
#   scripts/esp32-rs.sh --qemu build --release && scripts/qemu-rs.sh [seconds]
# The web app is on http://127.0.0.1:8088 while it runs (QEMU_PORT to change).
# 16 MB of PSRAM, like the N32R16 board (more leaves no room to map flash).
# Install QEMU once with: idf_tools.py install qemu-xtensa
# The QEMU build's "microphone" is build/qemu/clip.raw: 12 s of 16 kHz mono s16le,
# e.g. afconvert -f WAVE -d LEI16@16000 -c 1 song.m4a, minus the WAV header.
set -euo pipefail
cd "$(dirname "$0")/.."
SECONDS_TO_RUN="${1:-60}"
PORT="${QEMU_PORT:-8088}"
if [[ -n "${ESP_TOOLS:-}" ]]; then  # a folder with esp-idf/, toolchain/ and venv/
  export IDF_TOOLS_PATH="$ESP_TOOLS/toolchain"
  export PATH="$ESP_TOOLS/venv/bin:$PATH"
  source "$ESP_TOOLS/esp-idf/export.sh" >/dev/null
elif [[ -n "${IDF_PATH:-}" ]]; then
  command -v idf.py >/dev/null || source "$IDF_PATH/export.sh" >/dev/null
else
  echo "ESP-IDF 5.5.2 not found: set IDF_PATH (or ESP_TOOLS)." >&2
  exit 1
fi
T=firmware-rs/target/qemu/xtensa-esp32s3-espidf/release
B=$(ls -td "$T"/build/esp-idf-sys-*/out/build | head -1)  # the newest, after a flags change
O=build/qemu
mkdir -p "$O"

esptool.py --chip esp32s3 elf2image --flash_mode dio --flash_size 16MB -o "$O/app.bin" "$T/coverussi" >/dev/null
python "$IDF_PATH/components/partition_table/gen_esp32part.py" firmware-rs/partitions.csv "$O/ptable.bin" >/dev/null
esptool.py --chip esp32s3 merge_bin --fill-flash-size 16MB -o "$O/flash.bin" \
  0x0 "$B/bootloader/bootloader.bin" 0x8000 "$O/ptable.bin" 0x20000 "$O/app.bin" >/dev/null
# idf.py qemu's eFuses (chip rev 0.3); without them the ADC calibration at boot never ends.
python - "$IDF_PATH" "$O/efuse.bin" <<'PY'
import sys
sys.path.insert(0, f"{sys.argv[1]}/tools/idf_py_actions")
sys.path.insert(0, f"{sys.argv[1]}/tools")
from qemu_ext import QEMU_TARGETS
open(sys.argv[2], "wb").write(QEMU_TARGETS["esp32s3"].default_efuse)
PY

rm -f "$O/console.log"
qemu-system-xtensa -M esp32s3 -m 16M -display none -monitor none \
  -drive "file=$O/flash.bin,if=mtd,format=raw" \
  -drive "file=$O/efuse.bin,if=none,format=raw,id=efuse" \
  -global driver=nvram.esp32s3.efuse,property=drive,value=efuse \
  -global driver=timer.esp32s3.timg,property=wdt_disable,value=true \
  -nic "user,model=open_eth,hostfwd=tcp:127.0.0.1:$PORT-:80" \
  -serial "file:$O/console.log" >"$O/qemu.out" 2>&1 &
pid=$!
trap 'kill $pid 2>/dev/null || true' EXIT
for _ in $(seq "$SECONDS_TO_RUN"); do
  sleep 1
  grep -aqE 'Returned from app_main|panic|abort\(\)' "$O/console.log" 2>/dev/null && break
done
grep -av '^\s*$' "$O/console.log"
