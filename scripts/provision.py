# /// script
# requires-python = ">=3.12"
# dependencies = ["pyserial>=3.5"]
# ///
"""Set up the display over USB without printing its secrets.

Reads WIFI_SSID, WIFI_PASSWORD, HA_URL, HA_TOKEN, HA_CONNECT_IP and MQTT_URL
(mqtt://user:password@host) from the environment, for example:

    op run --env-file config/device.env.refs -- uv run scripts/provision.py

Only what's set is replaced, so `MQTT_URL=... uv run scripts/provision.py`
changes just the MQTT broker. The display restarts to use it.

    uv run scripts/provision.py --info     what the display has (no secrets)
    uv run scripts/provision.py --erase    forget everything
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time

import serial
from serial.tools import list_ports

ESPRESSIF_VID = 0x303A
FIELDS = {  # env var: what the firmware calls it
    "WIFI_SSID": "ssid",
    "WIFI_PASSWORD": "password",
    "HA_URL": "ha_url",
    "HA_TOKEN": "ha_token",
    "HA_CONNECT_IP": "connect_ip",
    "MQTT_URL": "mqtt_url",
}


def find_port() -> str:
    ports = [p.device for p in list_ports.comports() if p.vid == ESPRESSIF_VID]
    if not ports:
        raise SystemExit("No ESP32-S3 on USB. Plug the board's USB port (not POWER) in, or pass --port.")
    if len(ports) > 1:
        raise SystemExit(f"More than one ESP32 connected ({', '.join(ports)}); pass --port.")
    return ports[0]


def values_from_env() -> dict[str, str]:
    values = {}
    for env, key in FIELDS.items():
        value = os.environ.get(env)
        if value is None or (value == "" and env != "HA_CONNECT_IP"):
            continue
        if value.startswith("op://"):
            raise SystemExit(f"{env} is still a 1Password reference; run this through op run.")
        if env == "HA_URL":
            value = value.rstrip("/")
            if value.startswith("http://"):
                print("Note: plain http sends the Home Assistant token unencrypted on your network.")
        values[key] = value
    if "ssid" in values and "password" not in values:
        values["password"] = ""
    return values


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--port", help="serial port (default: the one Espressif device on USB)")
    p.add_argument("--info", action="store_true", help="print what the display reports and exit")
    p.add_argument("--erase", action="store_true", help="erase everything the display has stored")
    a = p.parse_args()

    values = {} if a.info or a.erase else values_from_env()
    if not (a.info or a.erase or values):
        raise SystemExit("Nothing to send: set some of " + ", ".join(FIELDS) + ".")

    port = serial.Serial()
    port.port = a.port or find_port()
    port.baudrate = 115200
    port.timeout = 1
    port.dtr, port.rts = True, False  # keeps the ESP32-S3 from resetting when the port opens
    port.open()
    with port:
        port.reset_input_buffer()
        if a.info:
            port.write(b"\nINFO\n")
        elif a.erase:
            port.write(b"\nERASE\n")
        else:
            port.write(b"\nPROVISION " + json.dumps(values, ensure_ascii=True).encode() + b"\n")
        port.flush()
        deadline = time.monotonic() + 12
        while time.monotonic() < deadline:
            line = port.readline()
            if a.info and line.startswith(b"INFO "):
                print(json.dumps(json.loads(line[5:]), indent=2))
                return
            if b"ERASE_OK" in line:
                print("Erased. The display is restarting into USB setup.")
                return
            if b"PROVISION_OK" in line:
                print("Stored " + ", ".join(values) + ". The display is restarting.")
                return
            if b"PROVISION_REJECTED" in line:
                reason = line.split(b"PROVISION_REJECTED", 1)[1].strip().decode(errors="replace")[:32]
                raise SystemExit(f"The display rejected the setup ({reason}).")
    sys.exit("No answer from the display. Nothing secret was printed.")


if __name__ == "__main__":
    main()
