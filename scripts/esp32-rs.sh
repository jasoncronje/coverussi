#!/bin/bash
# Cargo for firmware-rs/, with ESP-IDF 5.5.2 (IDF_PATH) and the Xtensa Rust toolchain from espup (`espup install -t esp32s3`).
#   scripts/esp32-rs.sh build --release
#   scripts/esp32-rs.sh --qemu build --release   (for Espressif's QEMU, in target/qemu)
set -euo pipefail
cd "$(dirname "$0")/.."
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
# Only espup's libclang, for bindgen. Its GCC is newer than IDF 5.5.2 accepts,
# so IDF's own stays first on PATH.
export LIBCLANG_PATH="$(sed -n 's/^export LIBCLANG_PATH="\(.*\)"$/\1/p' "$HOME/export-esp.sh")"
if [[ "${1:-}" == flash ]]; then
  # The board's images, as the browser flasher writes them; NVS (setup) is kept.
  O=build/firmware-rs
  python3 scripts/firmware_manifest.py firmware-rs/target/xtensa-esp32s3-espidf/release "$O" >/dev/null
  args=()
  while read -r offset path; do args+=("$offset" "$O/$path"); done < <(
    python3 -c 'import json,sys; [print(hex(p["offset"]), p["path"]) for p in json.load(open(sys.argv[1]))["parts"]]' "$O/manifest.json")
  exec esptool.py --chip esp32s3 ${2:+--port "$2"} write_flash --flash_mode dio --flash_freq 80m --flash_size 32MB "${args[@]}"
fi
cd firmware-rs
if [[ "${1:-}" == --qemu ]]; then
  shift
  export CARGO_TARGET_DIR="$PWD/target/qemu"
  export ESP_IDF_SDKCONFIG_DEFAULTS="$PWD/sdkconfig.defaults;$PWD/sdkconfig.qemu"
  # Joined with the rustflags in .cargo/config.toml; see `qemu` in src/main.rs.
  export CARGO_TARGET_XTENSA_ESP32S3_ESPIDF_RUSTFLAGS="-C link-arg=-Wl,--wrap=adc_calc_hw_calibration_code"
  exec cargo "$@" --features qemu
fi
exec cargo "$@"
