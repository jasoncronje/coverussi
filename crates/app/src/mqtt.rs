//! Home Assistant entities through MQTT discovery: a now-playing sensor and an
//! image entity for what an analog source plays, a light for the panel (on/off
//! and brightness) and an online sensor.

use serde_json::{json, Value};

pub const BASE: &str = "coverussi";

pub struct Topics {
    pub availability: String,
    pub now_playing: String,
    pub artwork: String,
    pub light_state: String,
    pub light_set: String,
}

/// `device`: the display's id in topics and unique ids (from its MAC).
pub fn topics(device: &str) -> Topics {
    let t = format!("{BASE}/{device}");
    Topics {
        availability: format!("{t}/online"),
        now_playing: format!("{t}/analog/now_playing"),
        artwork: format!("{t}/analog/artwork_url"),
        light_state: format!("{t}/panel/state"),
        light_set: format!("{t}/panel/set"),
    }
}

/// (topic, config) for each entity.
pub fn discovery(prefix: &str, device: &str, name: &str) -> Vec<(String, Value)> {
    let t = topics(device);
    let uid = format!("coverussi_{device}");
    let dev = json!({"identifiers": [uid], "name": name, "manufacturer": "DIY", "model": "64×64 matrix"});
    vec![
        (
            format!("{prefix}/sensor/{uid}/analog_now_playing/config"),
            json!({
                "name": "Analog now playing", "unique_id": format!("{uid}_analog_now_playing"), "device": dev,
                "state_topic": t.now_playing, "value_template": "{{ value_json.title | default('') }}",
                "json_attributes_topic": t.now_playing, "icon": "mdi:music-circle",
            }),
        ),
        (
            format!("{prefix}/image/{uid}/analog_artwork/config"),
            json!({"name": "Analog artwork", "unique_id": format!("{uid}_analog_artwork"), "device": dev, "url_topic": t.artwork}),
        ),
        (
            format!("{prefix}/light/{uid}/panel/config"),
            json!({
                "name": "Panel", "unique_id": format!("{uid}_panel"), "device": dev,
                "availability_topic": t.availability, "payload_available": "online", "payload_not_available": "offline",
                "schema": "json", "state_topic": t.light_state, "command_topic": t.light_set,
                "brightness": true, "brightness_scale": 255, "supported_color_modes": ["brightness"],
            }),
        ),
        (
            format!("{prefix}/binary_sensor/{uid}/online/config"),
            json!({
                "name": "Online", "unique_id": format!("{uid}_online"), "device": dev, "device_class": "connectivity",
                "state_topic": t.availability, "payload_on": "online", "payload_off": "offline",
            }),
        ),
    ]
}

/// A light command from Home Assistant as the panel command's args.
pub fn panel_command(payload: &[u8]) -> Option<Value> {
    let cmd: Value = serde_json::from_slice(payload).ok()?;
    let mut args = json!({"on": cmd["state"] == "ON"});
    if let Some(b) = cmd["brightness"].as_f64() {
        args["brightness"] = json!(b as i64);
    }
    Some(args)
}

pub fn panel_state(on: bool, brightness: i64) -> Value {
    json!({"state": if on { "ON" } else { "OFF" }, "brightness": brightness})
}

/// `mqtt://user:pass@host:port`, from USB setup or the web app.
pub struct Broker {
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

pub fn broker(url: &str) -> Option<Broker> {
    let rest = url.strip_prefix("mqtt://").or_else(|| url.strip_prefix("mqtts://"))?;
    let scheme = &url[..url.len() - rest.len()];
    let (auth, host) = match rest.rsplit_once('@') {
        Some((a, h)) => (Some(a), h),
        None => (None, rest),
    };
    if host.is_empty() {
        return None;
    }
    let (username, password) = match auth.map(|a| a.split_once(':').unwrap_or((a, ""))) {
        Some((u, p)) => (Some(u.to_string()).filter(|u| !u.is_empty()), Some(p.to_string()).filter(|p| !p.is_empty())),
        None => (None, None),
    };
    let host = if host.contains(':') { host.to_string() } else { format!("{host}:1883") };
    Some(Broker { url: format!("{scheme}{host}"), username, password })
}

/// The broker as stored, from the web app's address, username and password.
/// A bare host gets `mqtt://`; an empty address means no broker.
pub fn broker_url(address: &str, username: &str, password: &str) -> Option<String> {
    let address = address.trim().trim_end_matches('/');
    if address.is_empty() {
        return Some(String::new());
    }
    let address = if address.contains("://") { address.to_string() } else { format!("mqtt://{address}") };
    let rest = address.strip_prefix("mqtt://").or_else(|| address.strip_prefix("mqtts://"))?;
    if rest.contains('@') || username.contains(':') {
        return None;
    }
    let scheme = &address[..address.len() - rest.len()];
    let url = match (username.is_empty(), password.is_empty()) {
        (true, true) => address.clone(),
        (false, true) => format!("{scheme}{username}@{rest}"),
        _ => format!("{scheme}{username}:{password}@{rest}"),
    };
    broker(&url).map(|_| url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entities_point_at_the_topics() {
        let d = discovery("homeassistant", "a1b2c3", "Living room");
        assert_eq!(d.len(), 4);
        assert_eq!(d[2].0, "homeassistant/light/coverussi_a1b2c3/panel/config");
        assert_eq!(d[2].1["command_topic"], "coverussi/a1b2c3/panel/set");
        assert_eq!(d[0].1["device"]["name"], "Living room");
        assert_eq!(d[3].1["state_topic"], topics("a1b2c3").availability);
    }

    #[test]
    fn light_commands() {
        assert_eq!(panel_command(br#"{"state":"ON","brightness":120}"#), Some(json!({"on": true, "brightness": 120})));
        assert_eq!(panel_command(br#"{"state":"OFF"}"#), Some(json!({"on": false})));
        assert_eq!(panel_command(b"nope"), None);
        assert_eq!(panel_state(true, 80), json!({"state": "ON", "brightness": 80}));
    }

    #[test]
    fn broker_urls() {
        let b = broker("mqtt://ha:secret@192.168.1.5").unwrap();
        assert_eq!((b.url.as_str(), b.username.as_deref(), b.password.as_deref()), ("mqtt://192.168.1.5:1883", Some("ha"), Some("secret")));
        let b = broker("mqtt://broker.local:1884").unwrap();
        assert_eq!((b.url.as_str(), b.username, b.password), ("mqtt://broker.local:1884", None, None));
        assert!(broker("http://x").is_none() && broker("mqtt://").is_none());
    }

    #[test]
    fn broker_from_the_web_app() {
        assert_eq!(broker_url("10.0.0.2", "ha", "p:w@d").as_deref(), Some("mqtt://ha:p:w@d@10.0.0.2"));
        let b = broker(&broker_url("10.0.0.2", "ha", "p:w@d").unwrap()).unwrap();
        assert_eq!((b.url.as_str(), b.username.as_deref(), b.password.as_deref()), ("mqtt://10.0.0.2:1883", Some("ha"), Some("p:w@d")));
        assert_eq!(broker_url("mqtts://broker.local:8883/", "", "").as_deref(), Some("mqtts://broker.local:8883"));
        assert_eq!(broker_url("  ", "ha", "x").as_deref(), Some(""));
        assert!(broker_url("http://x", "", "").is_none() && broker_url("u@x", "", "").is_none() && broker_url("x", "a:b", "").is_none());
    }
}
