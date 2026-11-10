//! Home Assistant entities through MQTT discovery (coverussi_app::mqtt), when
//! a broker is set (USB setup or the web app): what an analog source plays, and the panel as a light.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;
use std::time::Duration;

use coverussi_app::mqtt::{self as topics, broker, discovery, panel_command, panel_state};
use esp_idf_svc::mqtt::client::{EspMqttClient, EventPayload, LwtConfiguration, MqttClientConfiguration, QoS};
use log::{info, warn};
use serde_json::{json, Value};

use crate::app::{self, Msg};

enum Ev {
    Connected,
    Disconnected,
    Received(String, Vec<u8>),
    NowPlaying(Value),
    Panel(bool, i64),
    Stop,
}

static TX: Mutex<Option<Sender<Ev>>> = Mutex::new(None);
static CONNECTED: AtomicBool = AtomicBool::new(false);
/// What an analog source plays now, for a client started with a new broker.
static PLAYING: Mutex<Option<Value>> = Mutex::new(None);

fn post(e: Ev) {
    if let Some(tx) = TX.lock().unwrap().as_ref() {
        let _ = tx.send(e);
    }
}

pub fn connected() -> bool {
    CONNECTED.load(Ordering::Relaxed)
}

pub fn now_playing(v: &Value) {
    *PLAYING.lock().unwrap() = Some(v.clone()).filter(|v| !v.is_null());
    post(Ev::NowPlaying(v.clone()));
}

pub fn panel(on: bool, brightness: i64) {
    post(Ev::Panel(on, brightness));
}

/// Connects to this broker, dropping the one before; "" just drops it.
pub fn start(url: &str) {
    if let Some(old) = TX.lock().unwrap().take() {
        let _ = old.send(Ev::Stop);
    }
    CONNECTED.store(false, Ordering::Relaxed);
    if url.is_empty() {
        return;
    }
    let Some(b) = broker(url) else {
        warn!("MQTT: {url:?} isn't an mqtt:// address");
        return;
    };
    let device = crate::net::mac().replace(':', "");
    let t = topics::topics(&device);
    let (tx, rx) = mpsc::channel();
    let tx_keep = tx.clone();
    let client_id = format!("coverussi-{device}");
    let conf = MqttClientConfiguration {
        client_id: Some(&client_id),
        username: b.username.as_deref(),
        password: b.password.as_deref(),
        keep_alive_interval: Some(Duration::from_secs(30)),
        reconnect_timeout: Some(Duration::from_secs(10)),
        // Internal RAM is scarce: buffers over 4 KB go to PSRAM (CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL; one of
        // exactly 4096 bytes stays internal), and the task, whose stack can only be internal, runs little
        // more than the callback below.
        buffer_size: 4096 + 1,
        out_buffer_size: 4096 + 1,
        task_stack: 4096,
        lwt: Some(LwtConfiguration { topic: &t.availability, payload: b"offline", qos: QoS::AtLeastOnce, retain: true }),
        ..Default::default()
    };
    let client = EspMqttClient::new_cb(&b.url, &conf, move |event| match event.payload() {
        EventPayload::Connected(_) => {
            let _ = tx.send(Ev::Connected);
        }
        EventPayload::Disconnected => {
            let _ = tx.send(Ev::Disconnected);
        }
        EventPayload::Received { topic, data, .. } => {
            let _ = tx.send(Ev::Received(topic.unwrap_or("").to_string(), data.to_vec()));
        }
        _ => {}
    });
    match client {
        Ok(client) => {
            *TX.lock().unwrap() = Some(tx_keep);
            crate::spawn_in_psram("mqtt", 6144, move || run(client, rx, device));
        }
        Err(e) => warn!("MQTT: {e}"),
    }
}

fn run(mut client: EspMqttClient<'static>, rx: Receiver<Ev>, device: String) {
    let t = topics::topics(&device);
    let pub_ = |client: &mut EspMqttClient, topic: &str, body: &str| {
        let _ = client.enqueue(topic, QoS::AtLeastOnce, true, body.as_bytes());
    };
    let mut last_panel = None;
    let mut last_playing = PLAYING.lock().unwrap().clone().unwrap_or_else(|| json!({"title": ""}));
    for e in rx {
        match e {
            Ev::Stop => {
                // A clean disconnect skips the last will.
                let _ = client.publish(&t.availability, QoS::AtLeastOnce, true, b"offline");
                return; // drops the client, which disconnects
            }
            Ev::Disconnected => CONNECTED.store(false, Ordering::Relaxed),
            Ev::Connected => {
                info!("MQTT connected");
                CONNECTED.store(true, Ordering::Relaxed);
                let ha = app::state().settings.home_assistant.clone();
                if ha.mqtt_discovery {
                    for (topic, config) in discovery("homeassistant", &device, &ha.name) {
                        pub_(&mut client, &topic, &config.to_string());
                    }
                }
                pub_(&mut client, &t.availability, "online");
                let _ = client.subscribe(&t.light_set, QoS::AtLeastOnce);
                pub_(&mut client, &t.now_playing, &last_playing.to_string());
                pub_(&mut client, &t.artwork, last_playing["art_url"].as_str().unwrap_or(""));
                if let Some((on, b)) = last_panel {
                    pub_(&mut client, &t.light_state, &panel_state(on, b).to_string());
                }
            }
            Ev::Received(topic, data) if topic == t.light_set => {
                if let Some(args) = panel_command(&data) {
                    app::post(Msg::Command("panel".into(), args));
                }
            }
            Ev::Received(..) => {}
            Ev::NowPlaying(v) => {
                last_playing = if v.is_null() { json!({"title": ""}) } else { v };
                pub_(&mut client, &t.now_playing, &last_playing.to_string());
                pub_(&mut client, &t.artwork, last_playing["art_url"].as_str().unwrap_or(""));
            }
            Ev::Panel(on, b) => {
                if last_panel != Some((on, b)) {
                    last_panel = Some((on, b));
                    pub_(&mut client, &t.light_state, &panel_state(on, b).to_string());
                }
            }
        }
    }
}
