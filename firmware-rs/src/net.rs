//! Wi-Fi (Ethernet in QEMU), the clock and the time zone.

use std::ffi::{c_char, c_int, CStr, CString};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use coverussi_app::idle::LocalTime;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sntp::{EspSntp, SntpConf};
use esp_idf_svc::sys;
use log::info;

use crate::config::Credentials;

static UP: AtomicBool = AtomicBool::new(false);
/// Whether this build has been kept after its first connection.
static CONFIRMED: AtomicBool = AtomicBool::new(false);

pub fn up() -> bool {
    UP.load(Ordering::Relaxed)
}

#[cfg(not(feature = "qemu"))]
const IFKEY: &CStr = c"WIFI_STA_DEF";
#[cfg(feature = "qemu")]
const IFKEY: &CStr = c"ETH_DEF";

/// Connect and stay connected, on a thread of its own.
#[cfg(not(feature = "qemu"))]
pub fn start(modem: esp_idf_svc::hal::modem::Modem<'static>, sysloop: EspSystemEventLoop, nvs: EspDefaultNvsPartition, c: &Credentials) {
    use esp_idf_svc::wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi};
    use log::warn;
    use std::time::Duration;
    let mut wifi = BlockingWifi::wrap(EspWifi::new(modem, sysloop.clone(), Some(nvs)).expect("Wi-Fi"), sysloop)
        .expect("Wi-Fi");
    let conf = ClientConfiguration {
        ssid: c.ssid.as_str().try_into().unwrap_or_default(),
        password: c.password.as_str().try_into().unwrap_or_default(),
        auth_method: if c.password.is_empty() { AuthMethod::None } else { AuthMethod::WPA2Personal },
        ..Default::default()
    };
    wifi.set_configuration(&Configuration::Client(conf)).expect("Wi-Fi configuration");
    wifi.start().expect("Wi-Fi start");
    // Home Assistant events and the web app's live view want low latency.
    unsafe { sys::esp_wifi_set_ps(sys::wifi_ps_type_t_WIFI_PS_NONE) };
    std::thread::Builder::new()
        .name("wifi".into())
        .stack_size(6144)
        .spawn(move || loop {
            if !wifi.is_connected().unwrap_or(false) {
                if UP.swap(false, Ordering::Relaxed) {
                    warn!("Wi-Fi lost; reconnecting");
                }
                if wifi.connect().is_ok() && wifi.wait_netif_up().is_ok() {
                    UP.store(true, Ordering::Relaxed);
                    info!("Wi-Fi connected, {}", ip());
                    if !CONFIRMED.swap(true, Ordering::Relaxed) {
                        crate::confirm_update();
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(5));
        })
        .expect("wifi thread");
}

/// QEMU's network card (`-nic user,model=open_eth`).
#[cfg(feature = "qemu")]
pub fn start(mac: esp_idf_svc::hal::mac::MAC<'static>, sysloop: EspSystemEventLoop, _: EspDefaultNvsPartition, _: &Credentials) {
    use esp_idf_svc::eth::{BlockingEth, EspEth, EthDriver};
    let driver = EthDriver::new_openeth(mac, sysloop.clone()).expect("Ethernet");
    let mut eth = BlockingEth::wrap(EspEth::wrap(driver).expect("Ethernet"), sysloop).expect("Ethernet");
    eth.start().expect("Ethernet start");
    eth.wait_netif_up().expect("Ethernet up");
    UP.store(true, Ordering::Relaxed);
    info!("Ethernet up, {}", ip());
    crate::confirm_update();
    std::mem::forget(eth);
}

/// SNTP, kept running.
pub fn start_clock() {
    let mut conf = SntpConf::default();
    conf.servers[0] = "time.cloudflare.com";
    std::mem::forget(EspSntp::new(&conf).expect("SNTP"));
}

pub fn ip() -> String {
    unsafe {
        let netif = sys::esp_netif_get_handle_from_ifkey(IFKEY.as_ptr());
        let mut info = sys::esp_netif_ip_info_t::default();
        if netif.is_null() || sys::esp_netif_get_ip_info(netif, &mut info) != sys::ESP_OK {
            return String::new();
        }
        let a = info.ip.addr.to_le_bytes();
        if a == [0; 4] {
            return String::new();
        }
        format!("{}.{}.{}.{}", a[0], a[1], a[2], a[3])
    }
}

pub fn mac() -> String {
    let mut m = [0u8; 6];
    unsafe { sys::esp_read_mac(m.as_mut_ptr(), sys::esp_mac_type_t_ESP_MAC_WIFI_STA) };
    m.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(":")
}

/// 0 while down.
pub fn rssi() -> i32 {
    #[cfg(not(feature = "qemu"))]
    {
        let mut ap = sys::wifi_ap_record_t::default();
        if up() && unsafe { sys::esp_wifi_sta_get_ap_info(&mut ap) } == sys::ESP_OK {
            return ap.rssi as i32;
        }
    }
    0
}

/// Unix seconds; small until SNTP has the time.
pub fn unix() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

pub fn clock_set() -> bool {
    unix() > 1_700_000_000.0
}

pub fn set_time_zone(posix: &str) {
    if posix.is_empty() {
        return;
    }
    std::env::set_var("TZ", posix);
    unsafe { sys::tzset() };
}

/// The local time; None until the clock is set.
pub fn local_time() -> Option<LocalTime> {
    if !clock_set() {
        return None;
    }
    unsafe {
        let t: sys::time_t = sys::time(core::ptr::null_mut());
        let mut tm = sys::tm::default();
        sys::localtime_r(&t, &mut tm);
        Some(LocalTime {
            minute: (tm.tm_hour * 60 + tm.tm_min) as u32,
            weekday: tm.tm_wday as u32,
            day: tm.tm_mday as u32,
            month: tm.tm_mon as u32 + 1,
            unix: t as u64,
        })
    }
}

// USB setup's optional connect_ip: resolve the Home Assistant host to a fixed LAN
// address while TLS and HTTP still use the hostname. lwip_getaddrinfo is wrapped
// (`--wrap=lwip_getaddrinfo` in .cargo/config.toml).

static OVERRIDE: Mutex<Option<(CString, CString)>> = Mutex::new(None);

pub fn dns_override(host: &str, ip: &str) {
    if host.is_empty() || ip.is_empty() {
        return;
    }
    if let (Ok(h), Ok(i)) = (CString::new(host), CString::new(ip)) {
        *OVERRIDE.lock().unwrap() = Some((h, i));
    }
}

/// The host of a URL.
pub fn host_of(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    rest.split(['/', ':']).next().unwrap_or("")
}

extern "C" {
    fn __real_lwip_getaddrinfo(
        node: *const c_char,
        service: *const c_char,
        hints: *const sys::addrinfo,
        res: *mut *mut sys::addrinfo,
    ) -> c_int;
}

#[no_mangle]
unsafe extern "C" fn __wrap_lwip_getaddrinfo(
    node: *const c_char,
    service: *const c_char,
    hints: *const sys::addrinfo,
    res: *mut *mut sys::addrinfo,
) -> c_int {
    let mut node = node;
    if !node.is_null() {
        if let Some((host, ip)) = OVERRIDE.lock().unwrap().as_ref() {
            if CStr::from_ptr(node).to_bytes().eq_ignore_ascii_case(host.to_bytes()) {
                node = ip.as_ptr(); // the CString lives in the static
            }
        }
    }
    __real_lwip_getaddrinfo(node, service, hints, res)
}
