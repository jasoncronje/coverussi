//! What's kept in NVS: what USB setup stores (Wi-Fi, Home Assistant), the
//! settings, the recent songs, album corrections and the web app's password.
//! Also the USB setup console the browser flasher and scripts/provision.py talk to.

use std::sync::{Arc, Mutex};

use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};
use esp_idf_svc::sys;
use log::warn;
use serde_json::{json, Value};

/// The namespace the C++ firmware used, so a board set up with it keeps its setup.
const SPACE: &str = "aad";

#[derive(Clone, Debug, Default)]
pub struct Credentials {
    pub ssid: String,
    #[cfg_attr(feature = "qemu", allow(dead_code))]
    pub password: String,
    pub ha_url: String,
    pub ha_token: String,
    /// Resolve the Home Assistant host to this LAN address.
    pub connect_ip: String,
    /// mqtt://user:pass@host:port, for Home Assistant's MQTT discovery.
    pub mqtt_url: String,
}

impl Credentials {
    pub fn wifi(&self) -> bool {
        !self.ssid.is_empty()
    }
    pub fn home_assistant(&self) -> bool {
        !self.ha_url.is_empty() && !self.ha_token.is_empty()
    }
}

#[derive(Clone)]
pub struct Store(Arc<Mutex<EspNvs<NvsDefault>>>);

impl Store {
    pub fn new(partition: EspDefaultNvsPartition) -> Self {
        Self(Arc::new(Mutex::new(EspNvs::new(partition, SPACE, true).expect("NVS namespace"))))
    }

    pub fn get(&self, key: &str) -> Option<String> {
        self.blob(key).and_then(|b| String::from_utf8(b).ok())
    }

    /// Strings and blobs both: the C++ firmware stored strings, this stores blobs (no 4000-byte limit).
    pub fn blob(&self, key: &str) -> Option<Vec<u8>> {
        let nvs = self.0.lock().unwrap();
        if let Ok(Some(n)) = nvs.blob_len(key) {
            let mut buf = vec![0; n];
            return nvs.get_blob(key, &mut buf).ok().flatten().map(|b| b.to_vec());
        }
        let n = nvs.str_len(key).ok().flatten()?;
        let mut buf = vec![0; n + 1];
        nvs.get_str(key, &mut buf).ok().flatten().map(|s| s.as_bytes().to_vec())
    }

    pub fn set(&self, key: &str, value: &[u8]) -> bool {
        let nvs = self.0.lock().unwrap();
        let _ = nvs.remove(key); // it may be a string from the C++ firmware
        let ok = nvs.set_blob(key, value).is_ok();
        if !ok {
            warn!("couldn't save {key} ({} bytes)", value.len());
        }
        ok
    }

    pub fn u32(&self, key: &str) -> u32 {
        let nvs = self.0.lock().unwrap();
        nvs.get_u32(key).ok().flatten().or_else(|| nvs.get_i32(key).ok().flatten().map(|v| v.max(0) as u32)).unwrap_or(0)
    }

    pub fn set_u32(&self, key: &str, v: u32) {
        let nvs = self.0.lock().unwrap();
        let _ = nvs.remove(key);
        let _ = nvs.set_u32(key, v);
    }

    pub fn credentials(&self) -> Credentials {
        let text = |k| self.get(k).unwrap_or_default();
        Credentials {
            ssid: text("ssid"),
            password: text("password"),
            ha_url: text("ha_url"),
            ha_token: text("ha_token"),
            connect_ip: text("connect_ip"),
            mqtt_url: text("mqtt_url"),
        }
    }

    fn erase(&self) {
        let _ = self.0.lock().unwrap().erase_all();
    }

    /// A random secret, made once, for signing web app sessions.
    pub fn secret(&self) -> Vec<u8> {
        if let Some(s) = self.blob("secret").filter(|s| s.len() == 32) {
            return s;
        }
        let mut s = vec![0u8; 32];
        unsafe { sys::esp_fill_random(s.as_mut_ptr().cast(), s.len()) };
        self.set("secret", &s);
        s
    }
}

const KEYS: [&str; 6] = ["ssid", "password", "ha_url", "ha_token", "connect_ip", "mqtt_url"];

fn url_ok(u: &str) -> bool {
    u.starts_with("https://") || u.starts_with("http://")
}

fn token_ok(t: &str) -> bool {
    (16..2000).contains(&t.len()) && !t.contains([' ', '\t', '\r', '\n', '"'])
}

/// `PROVISION {"ssid","password","ha_url","ha_token","connect_ip","mqtt_url"}`: each key is
/// optional and only what's sent is replaced. Secrets are never echoed.
fn provision(store: &Store, body: &str) -> Result<(), &'static str> {
    let j: Value = serde_json::from_str(body).map_err(|_| "format")?;
    let j = j.as_object().ok_or("format")?;
    let text = |k: &str| j.get(k).and_then(Value::as_str);
    if let Some(ssid) = text("ssid") {
        let pass = text("password").unwrap_or("");
        if ssid.is_empty() || ssid.len() > 32 || (!pass.is_empty() && !(8..=63).contains(&pass.len())) {
            return Err("wifi");
        }
    }
    if text("ha_url").is_some_and(|u| !url_ok(u)) {
        return Err("url");
    }
    if text("mqtt_url").is_some_and(|u| !u.is_empty() && coverussi_app::mqtt::broker(u).is_none()) {
        return Err("mqtt");
    }
    if text("ha_token").is_some_and(|t| !token_ok(t)) {
        return Err("token");
    }
    for k in KEYS {
        if let Some(v) = text(k) {
            let v = if k == "ha_url" { v.trim_end_matches('/') } else { v };
            if !store.set(k, v.as_bytes()) {
                return Err("storage");
            }
        }
    }
    Ok(())
}

fn info(store: &Store) -> Value {
    let c = store.credentials();
    let d = unsafe { &*sys::esp_app_get_description() };
    let text = |s: &[core::ffi::c_char]| unsafe { core::ffi::CStr::from_ptr(s.as_ptr()) }.to_string_lossy().into_owned();
    json!({
        "project": text(&d.project_name),
        "version": text(&d.version),
        "mac": crate::net::mac(),
        "wifi": c.wifi(),
        "ha_url": c.ha_url,
        "ha_token": !c.ha_token.is_empty(),
        "mqtt": !c.mqtt_url.is_empty(),
        "ip": crate::net::ip(),
    })
}

fn restart() -> ! {
    std::thread::sleep(std::time::Duration::from_millis(300));
    unsafe { sys::esp_restart() }
}

fn line(store: &Store, line: &str) {
    if let Some(body) = line.strip_prefix("PROVISION ") {
        match provision(store, body) {
            Ok(()) => {
                println!("PROVISION_OK");
                restart();
            }
            Err(problem) => println!("PROVISION_REJECTED {problem}"),
        }
    } else if line == "INFO" {
        println!("INFO {}", info(store));
    } else if line == "ERASE" {
        store.erase();
        println!("ERASE_OK");
        restart();
    } else if line == "RESTART" {
        restart();
    }
}

/// The USB setup console, on its own thread.
#[cfg(not(feature = "qemu"))]
pub fn console(store: Store) {
    unsafe {
        let mut cfg = sys::usb_serial_jtag_driver_config_t { tx_buffer_size: 4096, rx_buffer_size: 4096 };
        sys::esp!(sys::usb_serial_jtag_driver_install(&mut cfg)).expect("USB serial driver");
        sys::esp_vfs_usb_serial_jtag_use_driver();
    }
    std::thread::Builder::new()
        .name("console".into())
        .stack_size(8192)
        .spawn(move || {
            let mut text = Vec::new();
            let mut overflow = false;
            let mut bytes = [0u8; 128];
            loop {
                let n = unsafe { sys::usb_serial_jtag_read_bytes(bytes.as_mut_ptr().cast(), bytes.len() as u32, 1000) };
                for &b in &bytes[..n.max(0) as usize] {
                    match b {
                        b'\n' => {
                            if !overflow {
                                line(&store, String::from_utf8_lossy(&text).trim_end_matches('\r'));
                            }
                            text.clear();
                            overflow = false;
                        }
                        _ if text.len() < 4096 => text.push(b),
                        _ => overflow = true,
                    }
                }
            }
        })
        .expect("console thread");
    log::info!("USB setup console ready");
}

/// QEMU's console is a UART that the setup protocol doesn't need: the QEMU build
/// takes Home Assistant from the environment it was built in.
#[cfg(feature = "qemu")]
pub fn console(store: Store) {
    for (k, v) in [("ha_url", option_env!("COVERUSSI_HA_URL")), ("ha_token", option_env!("COVERUSSI_HA_TOKEN"))] {
        if let Some(v) = v.filter(|v| !v.is_empty()) {
            store.set(k, v.as_bytes());
        }
    }
    let _ = line;
}
