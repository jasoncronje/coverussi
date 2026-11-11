# Set up Coverussi

## 1. Install and connect the display

Open **[jasoncronje.github.io/coverussi](https://jasoncronje.github.io/coverussi/)** in Chrome or Edge on a computer.

1. Plug the board's **USB** port (not POWER) into the computer.
2. **Install**, and pick the port named USB JTAG/serial debug unit. With nothing set up yet, the panel shows a pulsing amber square.
3. Fill in your Wi-Fi, Home Assistant's address and a long-lived access token (your Home Assistant profile, Security tab). For Home Assistant entities (the panel as a light, what analog sources play), add the MQTT broker Home Assistant uses (or later, in the web app's Settings → General). **Send to display**. They go straight to the board over USB.

**Check what it has** shows the web app's address. It has no password until you set one in Settings → General.

To update later, install again with **Erase the board first** left off, which keeps the board's settings. Or upload `coverussi.bin` from a [release](https://github.com/jasoncronje/coverussi/releases) in the web app's Settings → Controller, or press Update there.

<details>
<summary>From a terminal instead</summary>

The firmware is Rust on ESP-IDF 5.5.2. Install the Xtensa toolchain with `espup install -t esp32s3`, set `IDF_PATH` to your ESP-IDF install, then:

```sh
cd portal && npm ci && npm run build && cd ..   # the web app the board serves
scripts/esp32-rs.sh build --release
scripts/esp32-rs.sh flash
```

Connect it over USB. The secrets go to the board and are never printed:

```sh
WIFI_SSID=... WIFI_PASSWORD=... \
HA_URL=http://homeassistant.local:8123 HA_TOKEN=... \
MQTT_URL=mqtt://user:password@homeassistant.local \
uv run scripts/provision.py
```

`uv run scripts/provision.py --info` shows what the board has, without secrets; `--erase` clears it.

</details>

## 2. Tell it what to follow

Open the web app. The first time, it walks you through setup: the time zone, which way up the panel is, your charger, what to follow, the mic levels and a password. To go through it again, open `#/setup`.

Sources can be changed later in Settings, Sources:

- **Add source:** pick any Home Assistant entity. The highest one on the list that's playing is shown; drag to reorder.
- **A media player:** choose what it shows (Cover, Progress or Off), then any app or input that differs, such as HDMI set to Off. Set an input to Analog, such as Phono, and the display listens while the player is on it.
- **Anything else:** a switch or helper is analog while its state matches, such as a plug that's on.
- **The room:** with nothing above playing, listen for any music.

Then run **Calibrate**. It listens to the room while it's quiet, then with music playing, and sets the levels the display uses to tell the two apart.
