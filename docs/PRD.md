# Coverussi: design

A 64 × 64 LED matrix that shows the artwork of whatever is playing. **Digital sources** are smart devices that tell Home Assistant what they play (a streamer, a TV box, a smart speaker), and the artwork comes from Home Assistant. **Analog sources** can't (a turntable, a tape deck, a CD player), so the display listens with its own microphones, identifies the song with Shazam, and shows that album. The board does all of it itself and serves a small web app that monitors it and holds its settings.

## Decisions

| Topic | Decision | Why |
| --- | --- | --- |
| Device role | Home Assistant client. No direct streaming-service APIs. | One integration point. |
| Everything on the board | No server: recognition, settings, the web app, MQTT and firmware updates all run on the ESP32-S3 | One device to set up. 16 MB of PSRAM and 32 MB of flash have room for it. |
| Language | Rust: `crates/app` (the decisions, host-tested) and `firmware-rs` (esp-idf-svc on ESP-IDF 5.5.2), plus a small C++ component for the panel, mic codec and JPEG decoder | One language for the engine, recognition and the web server; the engine's tests run on a computer. |
| Recognition | Shazam's tag endpoint, with the signature code from **shazamio-core** (MIT, Rust), called from the board | Free and accurate. It's reverse-engineered and can break, so it sits behind one function (`identify(pcm) → answer`). |
| Web app | Svelte 5 + Vite + TypeScript, built to 54 KB gzipped and embedded in the firmware | Small enough to serve from flash. |
| Storage | NVS: setup, settings, the last 10 songs, album corrections, the web app's password | No history; the recent songs are a buffer. |
| Digital sources | Any number of media players; the one that started playing last wins | What you started last is what you're looking at. |
| Analog sources | A named list, each played while an HA entity's attribute or state has a value (such as a receiver's source `Phono`) | A device that isn't smart can't be integrated; something in HA still knows when it plays. |
| No entity at all | Optional **Listen for music** (off by default): with nothing else playing, the room's sound is the analog source | A turntable into a plain amp has nothing in HA to watch. |
| Nothing playing | Panel off | Calm and saves power. |
| Panel and mics | `esphome/esp-hub75` driver, `esp_codec_dev` for the ES7210, `esp_jpeg` | Waveshare's examples use them. |

## Hardware

| Part | Detail |
| --- | --- |
| Controller | Waveshare ESP32-S3-RGB-Matrix: ESP32-S3-N32R16 (32 MB flash, 16 MB octal PSRAM), ES7210 4-channel ADC with 2 mics, ES8311 codec, NS4150 amp, QMI8658 IMU, SHTC3, PCF85063 RTC, TF slot |
| Panel | Waveshare 64 × 64 P2.5 HUB75, 160 × 160 mm, 1/32 scan |
| Power | A USB-C charger that supplies 5 V ⎓ 3 A (a 20 W PD charger), plugged into the board's **POWER** port. The panel's power lead goes to the board's two screw posts (5V and GND). Firmware limits the current (see Power below). |

Pin map (from Waveshare's BSP, `example/idf_v5.5.2/components/bsp/esp32_s3_matrix/include/bsp/config.h`):

| Function | GPIO |
| --- | --- |
| HUB75 R1 G1 B1 / R2 G2 B2 | 4 5 6 / 7 15 16 |
| HUB75 A B C D E | 18 8 3 42 9 |
| HUB75 LAT OE CLK | 40 2 41 |
| I2C SDA / SCL (codecs, sensors) | 47 / 48 |
| I2S MCLK BCLK WS DOUT DIN | 12 43 38 21 39 |
| Speaker amp enable | 11 |
| TF card CLK CMD D0 (1-bit SDMMC) | 1 44 17 |
| BOOT button | 0 |

GPIO43/44 are UART0's pins, so the console uses the USB Serial/JTAG port.

## Power

From Waveshare's schematic:

- The **POWER** USB-C port feeds the 5 V rail directly. It has only the 5.1 kΩ CC resistors and no PD chip, so it gets 5 V at whatever the charger offers a plain USB-C device, which is at most 3 A.
- The two screw posts (H2 = USB_5V, H3 = GND) are the panel's power outputs. Panel current flows through the board's 5 V rail.
- The data **USB** port reaches the same rail through a P-MOSFET (AO4407A) switch. A charger on POWER and a computer on USB can be connected together for flashing.
- The 4-pin **5V IN** header (U8) is on the same rail. It's for a bigger supply later.

Plan:

- **Current limit in firmware:** before drawing a frame, estimate its current from the sum of its gamma-corrected channel values × brightness, and scale brightness down so the estimate stays under the limit (default 2.4 A, set in the web app). Album art rarely reaches it, but a white cover would.
- If the panel ever needs more than about 2.5 A, move to a 5 V 4 A+ supply on the 5V IN header and leave POWER empty, so two supplies never feed the rail at once.

## Architecture

```text
                    ┌────────────────── Home Assistant ──────────────────┐
Streamer ──────────►│ media_player.streamer     (source, state, art)     │
TV box ────────────►│ media_player.tv_box       (state, art)             │
                    │ image.coverussi_analog_artwork   ◄── MQTT ─┐       │
                    │ sensor.coverussi_analog_…        ◄── MQTT ─┤       │
                    │ light.coverussi_panel            ◄── MQTT ─┤       │
                    └────────────────▲─────────────────────────┼─────────┘
                                     │ WebSocket + art fetch   │
                               ┌─────┴─────────────────────────┴──┐
                               │        Matrix (ESP32-S3)         │──► Shazam
                               │   engine, recognition, web app   │──► iTunes
                               └─────────────────▲────────────────┘
                                                 │ HTTP + WebSocket (LAN)
                                      Browser: the web app
```

- **Home Assistant:** the board subscribes to the followed entities over HA's WebSocket (`subscribe_entities`). It fetches the `entity_picture` of whatever it should show, decodes the JPEG, area-averages it to 64 × 64 and draws it with CIE 1931 gamma.
- **Recognition:** for an analog source the board records a clip and makes the Shazam signature as it records. It asks Shazam once 5 s are in; only a miss waits for the whole clip and asks again. Then it looks up the track's length and album on iTunes, applies the album lock and fetches the art (at 128 px, the size the panel needs).
- **MQTT** (optional, when USB setup gave a broker): Home Assistant gets a now-playing sensor and an `image` entity for analog sources, and the panel as a light.
- **The web app** is served by the board on port 80, LAN only.

## What the matrix shows

Checked in order on every state change:

1. **An analog source is playing** (its entity's attribute or state has its value: a receiver's source is `Phono In`, or a `switch.tape_deck` is `on`) → listening (below). The first on the list wins; switching to another analog source starts a new listening session. Digital sources are ignored.
2. **A digital source is `playing`** (in an app or input on its list, if it has one) → its artwork. If several are playing, one naming a song beats one reporting none (a WiiM on its HDMI input plays "Unknown"), then one with artwork beats one without, then the one that started most recently wins.
3. **Paused** → keep the artwork for the pause grace (default 30 s), then off.
4. **Listen for music** is on and no digital source plays in any app (a TV on Netflix counts, if it's a digital source) → listening to the room: music heard for `music_start_s` (default 5 s) starts an identify; after a short gap (the next track) 1 s is enough. When the retries after misses run out, it gives up, clears the art and ignores gaps until a silence long enough to end a side.
5. Otherwise **off**.

Changes of artwork use a transition (default a 400 ms crossfade; also fade through black, slide, wipe, dissolve or cut), previewed on the Settings page. The web app can turn the panel off, which overrides all of this.

## Analog sources

### Listening

The matrix only reads the mics while an analog source plays, or while it listens to the room for music. It computes the level continuously and records a clip only when it needs to identify something. Clips are never stored.

States:

| State | Panel | Leaves when |
| --- | --- | --- |
| **Quiet** | off, or the last album dimmed | level above the music threshold for 1 s → Identify |
| **Identify** | unchanged | Records a clip (default 10 s, 16 kHz mono 16-bit, 320 KB in PSRAM) and identifies it. Match → Playing. No match → retry after 10 s, 20 s, 40 s, then wait for the next gap. |
| **Playing** | album art | Predicted end reached + 1 s → Identify. Gap detected → Identify. Long silence → Quiet. |

The web app's **Identify now** button forces Identify from any state.

### Detecting a new song

- **Predicted end:** Shazam returns the clip's offset into the track. The board looks up the track's length (iTunes, by the Apple Music ID in Shazam's response, or a search) and works out `ends_in_ms`. The matrix listens again about 1 s after that.
- **Gaps:** on a record, tracks are separated by 1–3 s of near silence (surface noise only). A drop below the gap threshold for at least 0.8 s, then music again, triggers Identify early. This catches skipped tracks, a moved needle and wrong length data.
- **End of side:** below the gap threshold for more than 30 s (run-out groove crackle counts as silence) → Quiet. The album stays dimmed for 2 min so flipping the record doesn't flash the panel off.
- **Pauses:** a scheduled listen (predicted end or a retry) waits while it's quiet, so a paused record is never sent to Shazam. When music returns after at least 0.8 s, the gap rule identifies again, which also corrects the predicted end for the pause. A pause over 30 s looks like a side end: dimmed, then cleared after 2 min.
- Thresholds are set in the web app. Its **Calibrate** helper records the room quiet, then a record playing, and suggests the music and gap thresholds from the two levels.

### Album lock

Song recognition often returns a compilation or single instead of the LP. The board keeps the current session's album:

- The first match sets the album, from Shazam's `Album` metadata. With **Prefer the album version** on (the default), a match whose iTunes release is a single, EP, compilation or live record is swapped for the earliest studio album with the same artist and title, unless a correction names its album.
- A later match on the same album keeps it. A match on a different release of the same song (same artist and title, another album) keeps the locked album's art.
- A song by the same artist that isn't on the locked album's track list replaces the lock at once: another record is on. Without a track list, two matches in a row on another album replace it.
- The session ends after the side-end silence plus 10 minutes.
- From the web app you can **pin** the session's album, or save a **correction** (this Shazam album always means that album and art) that applies from then on.

Artwork: a correction first, then Shazam's cover art, then iTunes (`artworkUrl100` rewritten to 600 × 600).

## On the board

### Threads

| Thread | Does |
| --- | --- |
| app | Owns the engine (`crates/app/src/engine.rs`): source choice, the listener, transitions, the current limit. Draws about 30 frames a second. Everything else talks to it through a queue. |
| ha | Home Assistant's WebSocket: auth, subscribe, ping after 30 s quiet, reconnect |
| mic | 50 ms levels to the engine while it wants them, and clips |
| art / recognize | Art download and decode; Shazam, iTunes, album lock |
| live | Sends the web app's live feed (a WebSocket send waits on the HTTP server's task, so handlers never send directly) |
| mqtt | Discovery, now playing, the panel light |
| console | USB setup: `PROVISION`, `INFO`, `ERASE`, `RESTART` |

### Web API

With a password set, all of it is behind the login except the page itself and `/api/session` / `/api/login`.

| Endpoint | Does |
| --- | --- |
| `GET /api/session`, `POST /api/login`, `POST /api/logout`, `POST /api/password` | No password by default: everyone is logged in. Settings sets, changes or removes one (8 to 128 characters, stored as a salted, iterated HMAC-SHA256). The session cookie is signed with a secret kept in NVS and the password's hash, so a new password logs other browsers out; it lasts 30 days, and one made before the clock is set lasts until the board restarts. |
| `GET /api/state` | Telemetry, now playing, the recent songs, the rate-limit wait, Home Assistant's address (for streamed covers) |
| `GET`, `PUT /api/settings` | The settings document, its defaults and version |
| `GET /api/time_zones` | The time zone names the board knows, for the picker |
| `POST /api/commands` | `identify`, `panel` (on, brightness), `test_pattern`, `restart`, `calibrate` (seconds), `apps` (entity) |
| `POST /api/pin` | Pin the session's album |
| `GET`, `POST /api/corrections`, `DELETE /api/corrections/*` | Album corrections |
| `GET /api/ha/entities` | Entity search for the Sources settings |
| `GET /api/ha/apps?entity=` | A player's apps or inputs and what's open now, read from Home Assistant (works before the player is saved) |
| `POST /api/firmware` | A new app image: written to the other OTA slot, checked, then the board restarts into it and confirms it once it's back on the network; otherwise the bootloader rolls back |
| `/api/live` (WebSocket) | `snapshot` on connect, then `telemetry` (every 2 s and on change), `level` (10 a second), `now_playing`, `recent`, `recognition`, `event` (`analog_stopped`, `calibrate_result`, `apps_result`), `log`, `settings`, and the panel as binary RGB565 LE frames (8 KB, at most 5 a second) |

### Web app

| Page | Shows | Does |
| --- | --- | --- |
| **Now** | A live mirror of the panel; mode, source and track; the listener state with countdowns; a scrolling mic level graph with the music and gap thresholds; device health (Wi-Fi, HA, heap, PSRAM, uptime, current estimate) | Identify now, panel on/off, test pattern |
| **Recent** | The last 10 songs, heard and streamed | Fix a heard song's album (a correction); the corrections list |
| **Settings** | A sidebar of areas: General (the password, Prefer the album version), Panel, Sources, Microphone, Home Assistant and Device; an area with only advanced settings hides until Show advanced is on; changes apply on save from a bar pinned to the bottom | Save, revert, Calibrate, set or remove the password |
| **Settings → Device** | Firmware version, IP, clock, live log; the test pattern beside a preview of how it should look | Upload a firmware image, restart |

Settings (all with defaults; the display works before anyone opens the web app):

| Group | Settings |
| --- | --- |
| Display | brightness (day / night), night hours and time zone (a picker, from the board's list); rotation (0, 90, 180 or 270° clockwise, for how the panel is mounted; the web app's mirror stays upright); current limit; transition and its length; pause grace; side-end dim time |
| Sources | digital: media players, each with an optional list of apps or inputs to include or exclude (matched against HA's `app_name`, `app_id` or `source`; empty for all). Analog: a list of named sources, each an entity and the attribute (or `state`) and value that mean it's playing. Listen for music |
| Listening | music threshold; gap threshold and minimum gap; side-end silence; clip length; retry backoff; re-listen delay after the predicted end; mic gain |
| Recognition | rate limit; prefer the album version; album-lock rules (matches needed to replace a lock, session timeout) |
| Home Assistant | MQTT discovery on/off, the device name |

Secrets stay off the web app: Wi-Fi, the HA URL and token, and the MQTT broker go over USB, from the browser flasher or `scripts/provision.py`.

### Home Assistant entities (MQTT discovery)

Named after the device name setting (default Coverussi).


| Entity | Purpose |
| --- | --- |
| `sensor.coverussi_analog_now_playing` | What an analog source plays: state = title; attributes artist, album, art, offset, duration, matched_at |
| `image.coverussi_analog_artwork` | That album's art (`url_topic`) |
| `light.coverussi_panel` | Panel on/off and brightness, so automations can turn it off at night |
| `binary_sensor.coverussi_online` | The board connected to the broker (MQTT last will) |

### Other details

| Item | Plan |
| --- | --- |
| Rate limit | At most 1 recognition per 20 s (Shazam answers 429 for bursts), backoff 60 s doubling to 10 min on a 429 |
| Flash | 32 MB: NVS, OTA data, two 4 MB app slots. The app is 1.9 MB. |
| Time | SNTP; the time zone setting becomes a POSIX TZ rule from `crates/app/src/zones.rs` (made by `scripts/zones.py`) |
| Install | The browser flasher (GitHub Pages) writes the bootloader, partition table, blank OTA data and app, leaving NVS alone unless asked to erase; `scripts/esp32-rs.sh flash` does the same |

## Without the board

`scripts/esp32-rs.sh --qemu build --release && scripts/qemu-rs.sh` runs the firmware in Espressif's QEMU with an emulated network card: the web app is on `http://127.0.0.1:8088`, and the "microphone" is a recording (`build/qemu/clip.raw`), so Listen for music identifies it. Built with `COVERUSSI_HA_URL` and `COVERUSSI_HA_TOKEN` set, it follows Home Assistant too. The panel and mics do nothing there; the web app's mirror shows what the panel would.

## Repository layout

| Path | Role |
| --- | --- |
| `crates/app/` | The decisions, host-tested (`cargo test`): settings, model (source choice, level detector, listener), art (scale, transitions, current limit), engine, album lock, iTunes, Shazam request and answer, recent songs, auth, MQTT topics, Home Assistant and WebSocket framing |
| `crates/signature/` | The Shazam signature (from shazamio-core) |
| `firmware-rs/src/` | The board: `app` (engine task, BOOT button), `ha`, `work` (art, recognition), `audio`, `live`, `web`, `mqtt`, `net`, `config` (NVS, USB setup), `http`, `board` |
| `firmware-rs/components/board/` | C++: panel, mic codec, JPEG decoder |
| `portal/` | The web app and the browser flasher (Svelte 5) |
| `scripts/` | `esp32-rs.sh` (cargo with ESP-IDF; `flash`), `qemu-rs.sh`, `provision.py` (USB setup), `firmware_manifest.py` (flasher images), `zones.py`, `logo.py` |

## Milestones

| # | Milestone | Needs hardware |
| --- | --- | --- |
| 1 | **Recognizer** (done): identify WAV files; offset, length, album and art checked on 8 songs, including compilation sources | No |
| 2 | **Engine** (done): source choice, listener state machine, gap detector, transitions, current limit, album lock; host tests | No |
| 3 | **Web app** (done): Now, Recent, Settings (with Calibrate), Device (with firmware update) | No |
| 4 | **Rust on the board** (done, November 2, 2026): everything on the ESP32-S3, the hub removed | No |
| 5 | **QEMU end to end** (done): login, settings, Listen for music → Shazam match, art, recent songs, live feed with frames, firmware update into the other slot, Home Assistant players | No |
| 6 | **Hardware bring-up** (done, November 3, 2026): flash; test pattern (hold BOOT 2 s); mic levels in the web app from a real record; Calibrate | Yes |
| 7 | **On the board, end to end**: digital sources' art, listening to the turntable, the panel mirror, firmware update from the web app, MQTT entities | Yes |

## Findings

Recognizer tests, November 1, 2026: 8 songs from Apple's 30 s previews, each cut into two clean 10 s clips 10 s apart and one "room" clip (6 kHz low-pass, echo, pink noise, as a rough stand-in for the board's mics).

| Finding | Consequence |
| --- | --- |
| 24 of 24 clips matched, room clips included | shazamio is good enough to build on |
| `offset` is the track position at the clip's start: clips 10 s apart differed by 10.0 s | `ends_in_ms = duration − offset − clip length` |
| One room clip of Get Lucky matched at 119 s instead of 53 s: the song repeats | Predicted end can be wrong on repetitive songs; gap detection has to back it up |
| Dreams and Money came from compilation previews but matched Rumours and The Dark Side of the Moon | The album lock matters less than feared, but stays |
| So What matched "BD Music Presents Kind of Blue", a reissue | Album keys drop "… Presents" and edition tags; corrections cover the rest |
| 6 s room clips matched 3 of 3; 4 s matched 2 of 3 | 10 s default has margin; 8 s would do if latency matters |
| A burst got HTTP 429 for about 150 s; then one request every 30 s was fine | Requests are spaced (20 s) and back off (`crates/app/src/throttle.rs`) |
| Shazam's Apple Music ID gives the iTunes track directly, with its length and album | No fuzzy search needed in the normal case |
| A WiiM shows up twice in Home Assistant: once from the LinkPlay integration (source "Phono") and once from the WiiM integration (source "Phono In"). Neither reports audio levels, and on Phono both say `playing` whether or not the record turns | The board's mics are the only way to hear pauses, gaps and side ends |
| The WiiM integration reported the source as "Network" and a stale position while LinkPlay had "Spotify" and the right timing (November 2, 2026) | Follow the LinkPlay entity |
| Players pass through `buffering` / `idle` and drop their picture between tracks | A stopped player or a missing picture keeps its art 5 s before the panel changes |
| In QEMU (November 2, 2026): the board identified a 12 s room clip of Peg (Steely Dan) through Shazam, with TLS, the signature and iTunes all on the board | Recognition doesn't need a server |
| esp-idf-hal links ESP-IDF's legacy I2C driver, whose boot check aborts next to the new one the codec uses | `CONFIG_I2C_SKIP_LEGACY_CONFLICT_CHECK` |
| esp-idf-svc ignores a WebSocket handler's error, so a page that vanished without closing was called forever | The handler closes such sessions itself |

## Open questions and live tests

| # | Question | Answered by |
| --- | --- | --- |
| 1 | Is the panel's scan wiring standard, or does it need a different driver mode or shift driver (FM6126A)? | Answered: standard, with esp-hub75 pinned to 0.3.5 (0.3.6 garbles the picture) |
| 2 | Mic gain and the levels of music, gaps and room noise at the panel's spot | Calibrate in the web app (milestone 7) |
| 3 | Does a WiiM report `playing` on `Phono`, or only the source? | Answered: `playing`, whether or not the record turns |
| 4 | How short can a clip be for reliable Shazam matches? | Answered: 6 s worked on room clips, 4 s didn't always; default stays 10 s |
| 5 | Is the iTunes length good enough for predicted end, given vinyl speed drift? | Milestone 7 |
| 6 | Actual current with real album art at the chosen brightness | The web app's current estimate, checked once with an inline USB-C meter |
| 7 | Is the board's heap enough with TLS to Shazam, iTunes and HA at once, plus the web app? QEMU had 239 KB of internal RAM free while idle | Watching free heap in the web app on the board |
