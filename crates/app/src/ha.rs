//! Entity states from Home Assistant's `subscribe_entities` events: "a" adds a
//! whole state, "c" carries "+"/"-" diffs, "r" removes.

use std::collections::HashMap;

use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::model::Player;

struct Stored {
    state: String,
    attributes: Map<String, Value>,
}

#[derive(Default)]
pub struct HaStates {
    entities: HashMap<String, Stored>,
}

impl HaStates {
    /// Apply one event; returns the entities it touched.
    pub fn apply(&mut self, event: &Value) -> Vec<String> {
        let mut touched = Vec::new();
        let object = |k: &str| event.get(k).and_then(Value::as_object).into_iter().flatten();
        for (id, item) in object("a") {
            let state = item.get("s").and_then(Value::as_str).unwrap_or_default().to_string();
            let attributes = item.get("a").and_then(Value::as_object).cloned().unwrap_or_default();
            self.entities.insert(id.clone(), Stored { state, attributes });
            touched.push(id.clone());
        }
        for (id, item) in object("c") {
            let Some(e) = self.entities.get_mut(id) else { continue };
            if let Some(plus) = item.get("+") {
                if let Some(s) = plus.get("s").and_then(Value::as_str) {
                    e.state = s.to_string();
                }
                if let Some(a) = plus.get("a").and_then(Value::as_object) {
                    e.attributes.extend(a.iter().map(|(k, v)| (k.clone(), v.clone())));
                }
            }
            if let Some(gone) = item.pointer("/-/a").and_then(Value::as_array) {
                for key in gone.iter().filter_map(Value::as_str) {
                    e.attributes.remove(key);
                }
            }
            touched.push(id.clone());
        }
        for id in event.get("r").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
            self.entities.remove(id);
            touched.push(id.to_string());
        }
        touched
    }

    /// An entity's state and attributes, as Home Assistant sent them.
    pub fn get(&self, entity: &str) -> Option<(&str, &Map<String, Value>)> {
        self.entities.get(entity).map(|e| (e.state.as_str(), &e.attributes))
    }

    pub fn has(&self, entity: &str) -> bool {
        self.entities.contains_key(entity)
    }

    pub fn player(&self, entity: &str) -> Player {
        let mut p = Player { entity: entity.to_string(), ..Default::default() };
        let Some(e) = self.entities.get(entity) else { return p };
        let text = |k: &str| e.attributes.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
        p.state = e.state.clone();
        p.name = text("friendly_name");
        p.source = text("source");
        p.picture = text("entity_picture");
        p.title = text("media_title");
        p.artist = text("media_artist");
        p.album = text("media_album_name");
        p.app_name = text("app_name");
        p.app_id = text("app_id");
        p.attributes =
            e.attributes.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect();
        p
    }

    /// A player's apps or inputs and what's open now, for the app picker.
    pub fn apps(&self, entity: &str) -> Value {
        let p = self.player(entity);
        json!({
            "entity": entity,
            "followed": self.has(entity),
            "state": p.state,
            "app_name": p.app_name,
            "app_id": p.app_id,
            "source": p.source,
            "apps": self.strings(entity, "source_list"),
        })
    }

    /// A list attribute's strings, such as a player's source_list; empty if it has none.
    pub fn strings(&self, entity: &str, key: &str) -> Vec<String> {
        let list = self.entities.get(entity).and_then(|e| e.attributes.get(key)).and_then(Value::as_array);
        list.into_iter().flatten().filter_map(Value::as_str).map(String::from).collect()
    }
}

/// An entity for the web app's autocomplete.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Entity {
    pub entity: String,
    pub name: String,
    pub state: String,
}

/// Rows Home Assistant sends back for the autocomplete, at most this many.
const TEMPLATE_ROWS: usize = 200;

/// The POST /api/template body that lists entities in these domains (all when
/// empty) with the first word of `q` in the id or the name, as
/// `[[id, name, state], ...]`. Home Assistant filters, so the board never
/// downloads every state (megabytes on a big install); `search` ranks the rest.
pub fn entities_template(q: &str, domains: &[&str]) -> String {
    let word = q.split_whitespace().next().unwrap_or("").to_lowercase();
    let template = format!(
        "{{%- set d = {} -%}}{{%- set w = {} -%}}[{{%- for s in states if (not d or s.domain in d) \
         and w in (s.entity_id ~ ' ' ~ s.name) | lower -%}}{{%- if loop.index <= {TEMPLATE_ROWS} -%}}\
         {{{{ ',' if loop.index > 1 }}}}[{{{{ s.entity_id | tojson }}}},{{{{ s.name | tojson }}}},{{{{ s.state | tojson }}}}]\
         {{%- endif -%}}{{%- endfor -%}}]",
        json!(domains),
        json!(word),
    );
    json!({ "template": template }).to_string()
}

/// The template's answer, sorted by id.
pub fn parse_rows(body: &[u8]) -> Option<Vec<Entity>> {
    let rows: Vec<(String, String, String)> = serde_json::from_slice(body).ok()?;
    let mut all: Vec<Entity> = rows.into_iter().map(|(entity, name, state)| Entity { entity, name, state }).collect();
    all.sort_by(|a, b| a.entity.cmp(&b.entity));
    Some(all)
}

/// Entities in these domains (all when empty) with every word of `q` in the id or the name.
/// Ids and names that start with `q` come first.
pub fn search<'a>(all: &'a [Entity], q: &str, domains: &[&str], limit: usize) -> Vec<&'a Entity> {
    let q = q.to_lowercase();
    let words: Vec<&str> = q.split_whitespace().collect();
    let mut first = Vec::new();
    let mut rest = Vec::new();
    for e in all {
        let (domain, object) = e.entity.split_once('.').unwrap_or(("", &e.entity));
        if !domains.is_empty() && !domains.contains(&domain) {
            continue;
        }
        let text = format!("{} {}", e.entity, e.name).to_lowercase();
        if !words.iter().all(|w| text.contains(w)) {
            continue;
        }
        let starts = words.first().is_some_and(|w| object.starts_with(w) || e.name.to_lowercase().starts_with(w));
        if starts { first.push(e) } else { rest.push(e) }
    }
    first.into_iter().chain(rest).take(limit).collect()
}

/// The app picker's answer from Home Assistant's /api/states/<entity>, for a
/// player the device doesn't follow yet (just added, not saved).
pub fn apps_from_state(entity: &str, body: &[u8]) -> Option<Value> {
    let v: Value = serde_json::from_slice(body).ok()?;
    let mut ha = HaStates::default();
    ha.apply(&json!({"a": {entity: {"s": v.get("state")?, "a": v.get("attributes")?}}}));
    Some(ha.apps(entity))
}

/// What an analog source can match on, from Home Assistant's /api/states/<entity>:
/// the state and each text attribute, with the values each can take (an
/// attribute's `<name>_list`, or the `options` a select lists for its state).
/// Only text attributes, as only those are compared.
pub fn triggers_from_state(body: &[u8]) -> Option<Value> {
    let v: Value = serde_json::from_slice(body).ok()?;
    let state = v.get("state")?.as_str()?;
    let attrs = v.get("attributes")?.as_object()?;
    let texts = |key: &str| -> Vec<&str> {
        attrs.get(key).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).collect()
    };
    fn values<'a>(current: &'a str, list: Vec<&'a str>) -> Vec<&'a str> {
        let mut out = vec![current];
        out.extend(list.into_iter().filter(|x| *x != current));
        out.retain(|x| !x.is_empty());
        out
    }
    let mut names: Vec<&String> = attrs.iter().filter(|(_, v)| v.is_string()).map(|(k, _)| k).collect();
    names.sort();
    let attributes: Map<String, Value> = names
        .into_iter()
        .map(|k| (k.clone(), json!(values(attrs[k].as_str().unwrap_or(""), texts(&format!("{k}_list"))))))
        .collect();
    Some(json!({"states": values(state, texts("options")), "attributes": attributes}))
}

/// Entity ids are lowercase letters, digits, underscores and one dot.
pub fn valid_entity(entity: &str) -> bool {
    entity.split_once('.').is_some_and(|(d, o)| {
        let ok = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
        ok(d) && ok(o)
    })
}

/// The subscribe_entities request for these entities.
pub fn subscribe_request(id: u64, entities: &[String]) -> Value {
    json!({"id": id, "type": "subscribe_entities", "entity_ids": entities})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::AnalogSource;

    #[test]
    fn apps_come_from_one_state() {
        let body = br#"{"entity_id": "media_player.apple_tv", "state": "playing",
            "attributes": {"app_name": "Spotify", "app_id": "com.spotify.client", "source_list": ["Spotify", "Music"]}}"#;
        let a = apps_from_state("media_player.apple_tv", body).unwrap();
        assert_eq!(a["followed"], true);
        assert_eq!((a["state"].as_str(), a["app_name"].as_str()), (Some("playing"), Some("Spotify")));
        assert_eq!(a["apps"], json!(["Spotify", "Music"]));
        assert!(apps_from_state("media_player.apple_tv", b"{\"message\": \"Entity not found.\"}").is_none());
        assert!(valid_entity("media_player.apple_tv") && !valid_entity("media_player.x/../y") && !valid_entity("apple_tv"));
    }

    #[test]
    fn triggers_come_from_one_state() {
        let body = br#"{"entity_id": "media_player.wiim_ultra", "state": "playing",
            "attributes": {"source": "Network", "source_list": ["Network", "Phono In"], "volume_level": 0.4,
            "friendly_name": "WiiM Ultra"}}"#;
        let t = triggers_from_state(body).unwrap();
        assert_eq!(t["states"], json!(["playing"]));
        assert_eq!(t["attributes"], json!({"friendly_name": ["WiiM Ultra"], "source": ["Network", "Phono In"]}));
        let select = br#"{"state": "Vinyl", "attributes": {"options": ["Off", "Vinyl", "CD"]}}"#;
        assert_eq!(triggers_from_state(select).unwrap()["states"], json!(["Vinyl", "Off", "CD"]));
        assert!(triggers_from_state(b"{\"message\": \"Entity not found.\"}").is_none());
    }

    #[test]
    fn states_follow_subscribe_entities() {
        let mut ha = HaStates::default();
        let add = json!({"a": {"media_player.wiim_ultra_2": {"s": "playing", "a": {"source": "Network",
            "entity_picture": "/api/media_player_proxy/x?token=1", "media_title": "Peg", "media_artist": "Steely Dan",
            "media_album_name": "Aja", "friendly_name": "WiiM Ultra"}}}});
        assert_eq!(ha.apply(&add).len(), 1);
        let p = ha.player("media_player.wiim_ultra_2");
        assert_eq!((p.state.as_str(), p.source.as_str(), p.title.as_str()), ("playing", "Network", "Peg"));
        assert_eq!((p.album.as_str(), p.name.as_str()), ("Aja", "WiiM Ultra"));
        assert_eq!(p.picture, "/api/media_player_proxy/x?token=1");
        let change = json!({"c": {"media_player.wiim_ultra_2": {"+": {"s": "idle", "a": {"source": "Phono In"}},
            "-": {"a": ["entity_picture"]}}}});
        ha.apply(&change);
        let p = ha.player("media_player.wiim_ultra_2");
        assert_eq!((p.state.as_str(), p.source.as_str(), p.picture.as_str(), p.title.as_str()), ("idle", "Phono In", "", "Peg"));
        ha.apply(&json!({"r": ["media_player.wiim_ultra_2"]}));
        assert_eq!(ha.player("media_player.wiim_ultra_2").state, "");
        ha.apply(&json!({"a": {"media_player.apple_tv": {"s": "playing", "a": {"app_name": "Music", "app_id": "com.apple.TVMusic"}}}}));
        let p = ha.player("media_player.apple_tv");
        assert_eq!((p.app_name.as_str(), p.app_id.as_str()), ("Music", "com.apple.TVMusic"));
    }

    #[test]
    fn analog_attribute_comes_from_home_assistant() {
        let mut ha = HaStates::default();
        ha.apply(&json!({"a": {"media_player.receiver": {"s": "on", "a": {"sound_mode": "PHONO", "volume_level": 0.4}}}}));
        let p = ha.player("media_player.receiver");
        assert_eq!(p.attribute("sound_mode"), "PHONO");
        assert_eq!(p.attribute("volume_level"), ""); // strings only
        let a = AnalogSource {
            name: "Turntable".into(),
            entity: "media_player.receiver".into(),
            attribute: "sound_mode".into(),
            value: "PHONO".into(),
        };
        assert!(a.on(&p));
        ha.apply(&json!({"c": {"media_player.receiver": {"+": {"a": {"sound_mode": "STEREO"}}}}}));
        assert!(!a.on(&ha.player("media_player.receiver")));
    }

    #[test]
    fn entity_search() {
        let body = br#"[["switch.turntable","Turntable","off"],["media_player.wiim_ultra_2","WiiM Ultra","playing"],
            ["media_player.apple_tv","","idle"],["sensor.t","Turntable temperature","21.5"]]"#;
        let all = parse_rows(body).unwrap();
        assert_eq!(all[0].entity, "media_player.apple_tv");
        assert_eq!(all[3], Entity { entity: "switch.turntable".into(), name: "Turntable".into(), state: "off".into() });
        let ids = |v: Vec<&Entity>| v.into_iter().map(|e| e.entity.clone()).collect::<Vec<_>>();
        assert_eq!(ids(search(&all, "turn", &[], 10)), ["sensor.t", "switch.turntable"]);
        assert_eq!(ids(search(&all, "turntable", &["switch"], 10)), ["switch.turntable"]);
        assert_eq!(ids(search(&all, "ultra wiim", &["media_player"], 10)), ["media_player.wiim_ultra_2"]);
        assert_eq!(search(&all, "", &["media_player"], 1).len(), 1);
        assert_eq!(all[2].state, "21.5");
        assert!(parse_rows(b"{}").is_none());
    }

    #[test]
    fn entity_template_filters_in_home_assistant() {
        let body: Value = serde_json::from_str(&entities_template("WiiM ultra", &["media_player"])).unwrap();
        let t = body["template"].as_str().unwrap();
        assert!(t.starts_with(r#"{%- set d = ["media_player"] -%}{%- set w = "wiim" -%}["#), "{t}");
        assert!(t.contains("loop.index <= 200") && t.contains("{{ s.state | tojson }}]"), "{t}");
        assert!(!t.contains('\n'));
        // The query lands as a JSON string, so a quote can't end it early.
        let t = entities_template(r#"a"}}{{ x"#, &[]);
        assert!(t.contains(r#"set d = [] -%}{%- set w = \"a\\\"}}{{\" -%}"#), "{t}");
    }

    #[test]
    fn subscribe() {
        let q = subscribe_request(3, &["media_player.a".into()]);
        assert_eq!(q["type"], "subscribe_entities");
        assert_eq!(q["entity_ids"][0], "media_player.a");
    }
}
