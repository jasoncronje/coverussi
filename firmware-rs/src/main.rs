//! Coverussi: follows Home Assistant for digital sources' art, listens to
//! analog sources (a turntable, a tape deck) and identifies them with Shazam,
//! and serves its own web app. The decisions are coverussi-app's (crates/app);
//! this connects them to the board, the network and the mics.

mod app;
mod audio;
mod board;
mod config;
mod ha;
mod heap;
mod http;
mod live;
mod mqtt;
mod net;
mod update;
mod web;
mod work;

use std::time::Duration;

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::gpio::{PinDriver, Pull};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::ota::EspOta;
use esp_idf_svc::sys;
use log::{error, info, warn};

fn main() {
    sys::link_patches();
    live::start();
    let p = Peripherals::take().expect("peripherals");
    let sysloop = EspSystemEventLoop::take().expect("event loop");
    let nvs = EspDefaultNvsPartition::take().expect("NVS");
    let store = config::Store::new(nvs.clone());
    config::console(store.clone()); // first: the QEMU build stores its Home Assistant here
    let creds = store.credentials();
    info!("Coverussi {}", env!("CARGO_PKG_VERSION"));

    let boot = PinDriver::input(p.pins.gpio0, Pull::Up).expect("BOOT button");

    if !board::panel_begin() {
        error!("the panel didn't start");
    }
    #[cfg(not(feature = "qemu"))]
    if !creds.wifi() {
        setup_needed();
    }

    net::dns_override(net::host_of(&creds.ha_url), &creds.connect_ip);
    let rx = app::init(creds.clone(), store);
    work::start();
    #[cfg(not(feature = "qemu"))]
    net::start(p.modem, sysloop, nvs, &creds);
    #[cfg(feature = "qemu")]
    net::start(p.mac, sysloop, nvs, &creds);
    net::start_clock();
    audio::start(app::state().settings.listening.mic_gain_db);
    ha::start(&creds);
    mqtt::start(&creds.mqtt_url);
    let server = web::start().expect("web server");
    std::mem::forget(server);
    app::run(rx, boot);
}

/// Starts a thread with its stack in PSRAM, leaving internal RAM to the panel's
/// DMA buffers and the threads that must stay there. Only for threads that
/// never write flash (NVS, OTA): a flash write turns the cache, and with it
/// PSRAM, off.
pub fn spawn_in_psram(name: &str, stack: usize, f: impl FnOnce() + Send + 'static) {
    spawn_in_psram_on(None, name, stack, f);
}

/// The same, kept on one core. Left to the scheduler, a thread stays on the
/// core where it first uses floating point, a different one from start to start.
pub fn spawn_in_psram_on(core: Option<i32>, name: &str, stack: usize, f: impl FnOnce() + Send + 'static) {
    unsafe {
        let mut cfg = sys::esp_pthread_get_default_config();
        cfg.stack_alloc_caps = sys::MALLOC_CAP_SPIRAM | sys::MALLOC_CAP_8BIT;
        if let Some(core) = core {
            cfg.pin_to_core = core;
        }
        sys::esp_pthread_set_cfg(&cfg);
    }
    let spawned = std::thread::Builder::new().name(name.into()).stack_size(stack).spawn(f);
    unsafe { sys::esp_pthread_set_cfg(&sys::esp_pthread_get_default_config()) };
    spawned.expect(name);
}

/// Once the network is up, this build can take the next update: keep it. Called
/// from the Wi-Fi thread, since internal RAM has no room for a thread of its own.
pub fn confirm_update() {
    if let Err(e) = EspOta::new().and_then(|mut o| o.mark_running_slot_valid()) {
        warn!("couldn't confirm this firmware: {e}");
    }
}

/// Until USB setup has run: a slow amber pulse in the middle of the panel.
#[cfg(not(feature = "qemu"))]
fn setup_needed() -> ! {
    warn!("USB setup required: use the flasher or scripts/provision.py");
    board::panel_brightness(60);
    let mut f = [0u16; coverussi_app::art::SIZE * coverussi_app::art::SIZE];
    for i in 0u32.. {
        let v = if i % 40 < 20 { i % 20 * 6 } else { (20 - i % 20) * 6 } as u16;
        let (r, g) = ((v * 2).min(255) >> 3, v >> 2);
        for y in 28..36 {
            for x in 28..36 {
                f[y * coverussi_app::art::SIZE + x] = r << 11 | g << 5;
            }
        }
        board::panel_draw(&f);
        std::thread::sleep(Duration::from_millis(50));
    }
    unreachable!()
}

/// QEMU has no ADC, and IDF calibrates it in a startup constructor that waits
/// forever for a reading. The QEMU build links this in its place
/// (`--wrap=adc_calc_hw_calibration_code`).
#[cfg(feature = "qemu")]
#[no_mangle]
pub extern "C" fn __wrap_adc_calc_hw_calibration_code(_unit: u32, _atten: u32) {}
