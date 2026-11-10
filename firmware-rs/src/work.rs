//! Slow network jobs, each kind on its own thread so a recognition never holds
//! up the art: fetch and decode artwork; identify a clip (Shazam, then iTunes
//! for the track's length and album); look up a streamed song's album.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use coverussi_app::album_lock::{AlbumLock, Correction};
use coverussi_app::plays::Play;
use coverussi_app::throttle::Throttle;
use coverussi_app::{itunes, recent, shazam, Match};
use esp_idf_svc::http::Method;
use esp_idf_svc::sys;
use log::{info, warn};
use serde_json::{json, Value};
use signature::Fingerprint;

use crate::app::{self, Msg};
use crate::{board, ha, http, live, net};

const MAX_IMAGE: usize = 4 << 20;
const MAX_ANSWER: usize = 256 << 10;

/// Shazam is asked once this much of a clip is in; only a miss waits for the rest.
const FIRST_TRY_MS: u32 = 5000;

enum Job {
    /// A clip still recording: its blocks, until the sender hangs up.
    Stream { blocks: Receiver<Vec<i16>>, clip_ms: u32, source: String },
    Play(Play),
}

static ART: OnceLock<SyncSender<String>> = OnceLock::new();
static JOBS: OnceLock<SyncSender<Job>> = OnceLock::new();

/// Shazam's pacing and the listening session's album, shared with the web app.
pub struct Recognizer {
    pub throttle: Throttle,
    pub lock: AlbumLock,
    prefer_album: bool,
    /// The Apple Music store to ask.
    country: String,
    tracks: HashMap<String, Vec<String>>,
}

pub static RECOGNIZER: Mutex<Option<Recognizer>> = Mutex::new(None);

fn steady() -> f64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64()
}

pub fn rate_limit_wait_ms() -> u32 {
    RECOGNIZER.lock().unwrap().as_ref().map_or(0, |r| r.throttle.wait_ms(steady()))
}

/// The engine's Shazam spacing and album lock follow the settings.
pub fn apply_settings(settings: &coverussi_app::settings::Settings, corrections: &HashMap<String, Correction>) {
    let s = &settings.recognition;
    let mut g = RECOGNIZER.lock().unwrap();
    let r = g.get_or_insert_with(|| Recognizer {
        throttle: Throttle::default(),
        lock: AlbumLock::default(),
        prefer_album: true,
        country: String::new(),
        tracks: HashMap::new(),
    });
    r.throttle.min_interval_s = s.min_interval_s as f64;
    r.lock.replace_after = s.replace_after.max(1) as u32;
    r.lock.timeout_s = s.session_timeout_s as f64;
    r.prefer_album = s.prefer_album;
    r.country = settings.store_country();
    r.lock.corrections = corrections.clone();
}

/// The listening session is over (the analog source stopped).
pub fn end_session() {
    if let Some(r) = RECOGNIZER.lock().unwrap().as_mut() {
        r.lock.end();
    }
}

/// Keep this session's album whatever plays next. False with no session.
pub fn pin() -> bool {
    RECOGNIZER.lock().unwrap().as_mut().is_some_and(|r| r.lock.pin())
}

pub fn start() {
    let (art_tx, art_rx) = mpsc::sync_channel(8);
    let (job_tx, job_rx) = mpsc::sync_channel(4);
    let _ = ART.set(art_tx);
    let _ = JOBS.set(job_tx);
    crate::spawn_in_psram("art", 10 * 1024, move || art_loop(art_rx));
    // The signature's FFT and the TLS handshake both want stack. Nothing here
    // writes flash: the app task saves the recent songs. On core 1: on the
    // app task's core, fingerprinting a clip starves it.
    crate::spawn_in_psram_on(Some(1), "recognize", 32 * 1024, move || job_loop(job_rx));
}

/// A Home Assistant path (/api/media_player_proxy/...) or a URL.
pub fn fetch_art(key: &str) {
    if ART.get().is_none_or(|tx| tx.try_send(key.to_string()).is_err()) {
        app::post(Msg::ImageFailed);
    }
}

/// A clip as it records; the engine hears ClipFailed if it can't be queued.
pub fn identify_stream(blocks: Receiver<Vec<i16>>, clip_ms: u32, source: String) {
    if JOBS.get().is_none_or(|tx| tx.try_send(Job::Stream { blocks, clip_ms, source }).is_err()) {
        app::post(Msg::ClipFailed);
    }
}

pub fn played(p: Play) {
    if let Some(tx) = JOBS.get() {
        let _ = tx.try_send(Job::Play(p));
    }
}

fn art_loop(rx: Receiver<String>) {
    for key in rx {
        let c = &app::shared().creds;
        let msg = match ha::fetch(c, &itunes::panel_art_url(&key), MAX_IMAGE) {
            Err(e) => {
                warn!("art: {e}");
                Msg::ImageFailed
            }
            Ok(r) if r.status != 200 => {
                warn!("art: HTTP {} for {:.80}", r.status, key);
                Msg::ImageFailed
            }
            Ok(r) if !r.body.starts_with(&[0xFF, 0xD8]) => {
                warn!("art: not a JPEG ({} bytes); only JPEG artwork is supported", r.body.len());
                Msg::ImageFailed
            }
            Ok(r) => match board::decode_jpeg(&r.body, coverussi_app::art::SIZE as i32) {
                Some((rgb, w, h)) => {
                    info!("art {w}x{h} from {} bytes", r.body.len());
                    Msg::Image { key, rgb, w, h }
                }
                None => {
                    warn!("art: JPEG decode failed (progressive JPEGs aren't supported)");
                    Msg::ImageFailed
                }
            },
        };
        app::deliver(msg);
    }
}

fn job_loop(rx: Receiver<Job>) {
    for job in rx {
        match job {
            Job::Stream { blocks, clip_ms, source } => {
                let started = Instant::now();
                let answer = recognize_stream(blocks, clip_ms, &source);
                answered(answer, &source, started);
            }
            Job::Play(p) => {
                let (album, art) = song_art(&p.artist, &p.title).unwrap_or((None, None));
                app::add_recent(|r| r.streamed(&p, album, art));
            }
        }
    }
}

fn answered(answer: Value, source: &str, started: Instant) {
    info!(
        "recognition: {} {} ms after recording started",
        answer["reason"].as_str().unwrap_or(""),
        started.elapsed().as_millis()
    );
    live::send(&json!({"type": "recognition", "answer": answer, "source": source, "at": net::unix()}));
    app::deliver(Msg::Answer(answer));
}

/// Fingerprints the clip as it records. Shazam is asked at FIRST_TRY_MS; only
/// a miss waits for the whole clip and asks again.
fn recognize_stream(blocks: Receiver<Vec<i16>>, clip_ms: u32, source: &str) -> Value {
    let mut f = Fingerprint::default();
    let first_try = FIRST_TRY_MS.min(clip_ms);
    let mut tried = false;
    for block in blocks.iter() {
        f.feed(&block);
        if !tried && first_try < clip_ms && f.ms() >= first_try {
            tried = true;
            let answer = recognize(&mut f, false, source);
            if answer["reason"] != "no_match" {
                return answer; // dropping the blocks ends the recording
            }
        }
    }
    recognize(&mut f, tried, source)
}

fn uuid() -> String {
    let mut b = [0u8; 16];
    unsafe { sys::esp_fill_random(b.as_mut_ptr().cast(), 16) };
    b[6] = b[6] & 0x0F | 0x40;
    b[8] = b[8] & 0x3F | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02X}")).collect();
    format!("{}-{}-{}-{}-{}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..])
}

fn country() -> String {
    RECOGNIZER.lock().unwrap().as_ref().map(|r| r.country.clone()).unwrap_or_default()
}

fn json_get(url: &str) -> Option<Value> {
    let r = http::get(url, &[], MAX_ANSWER).ok().filter(|r| r.status == 200)?;
    serde_json::from_slice(&r.body).ok()
}

/// `follow_up`: the same clip again, longer, after a miss; it doesn't wait out the spacing.
fn recognize(f: &mut Fingerprint, follow_up: bool, source: &str) -> Value {
    if !follow_up {
        if let Some(Err(wait)) = RECOGNIZER.lock().unwrap().as_mut().map(|r| r.throttle.begin(steady())) {
            return recent::rate_limited(wait);
        }
    }
    let t = Instant::now();
    let sig = f.signature();
    let clip_ms = sig.sample_ms;
    let signed = t.elapsed().as_millis();
    let url = shazam::tag_url(&uuid(), &uuid());
    let body = shazam::tag_body(&sig.uri, sig.sample_ms, (net::unix() * 1000.0) as u64);
    let r = http::request(Method::Post, &url, &shazam::HEADERS, body.as_bytes(), MAX_ANSWER, Duration::from_secs(20));
    let raw: Value = match r {
        Ok(r) if r.status == 429 => {
            let wait = RECOGNIZER.lock().unwrap().as_mut().map_or(60_000, |g| g.throttle.refused(steady()));
            return recent::rate_limited(wait);
        }
        Ok(r) if r.status == 200 => match serde_json::from_slice(&r.body) {
            Ok(v) => v,
            Err(_) => return recent::failed(),
        },
        Ok(r) => {
            warn!("Shazam: HTTP {}", r.status);
            return recent::failed();
        }
        Err(e) => {
            warn!("Shazam: {e}");
            return recent::failed();
        }
    };
    if let Some(g) = RECOGNIZER.lock().unwrap().as_mut() {
        g.throttle.answered();
    }
    let shazamed = t.elapsed().as_millis();
    let Some(m) = shazam::parse(&raw) else { return recent::no_match() };
    drop(raw);
    // A match without a length still shows the art; it just can't predict the end.
    let (song, by_id) = itunes_song(&m);
    let looked_up = t.elapsed().as_millis();
    let m = match song.as_ref() {
        // Shazam's own release: keep its album and art.
        Some(song) if by_id => itunes::enrich(&m, Some(song)),
        // Shazam's release isn't in this store: show the store's.
        Some(song) => itunes::use_album_version(&itunes::enrich(&m, Some(song)), song),
        None => m.clone(),
    };
    let m = if wants_album_version(&m, song.as_ref()) { album_version(&m).unwrap_or(m) } else { m };
    let versioned = t.elapsed().as_millis();
    let tracks = album_tracks(m.collection_id.as_deref());
    info!(
        "timing: {clip_ms} ms clip, signature {signed} ms, Shazam +{} ms, iTunes song +{} ms, album version +{} ms, album tracks +{} ms",
        shazamed - signed,
        looked_up - shazamed,
        versioned - looked_up,
        t.elapsed().as_millis() - versioned
    );
    let decision = {
        let mut g = RECOGNIZER.lock().unwrap();
        let Some(g) = g.as_mut() else { return recent::failed() };
        g.lock.apply(&m, net::unix(), &tracks)
    };
    let answer = recent::matched(&m, &decision, clip_ms);
    app::now_playing(json!({
        "title": m.title, "artist": m.artist, "album": decision.album, "art_url": decision.art_url,
        "duration_ms": m.duration_ms, "offset_ms": m.offset_ms, "matched_at": net::unix(),
    }));
    app::add_recent(|r| r.heard(net::unix(), source, &m, &decision));
    answer
}

/// The match's song in the store, and whether it's Shazam's own release (found by its ID).
fn itunes_song(m: &Match) -> (Option<Value>, bool) {
    let country = country();
    if let Some(id) = m.apple_id.as_deref() {
        let song = json_get(&itunes::lookup_url(id, &country)).and_then(|a| itunes::looked_up(&a).cloned());
        if song.is_some() {
            return (song, true);
        }
    }
    let song = json_get(&itunes::search_url(&m.artist, &m.title, &country)).and_then(|a| itunes::found(&a, &m.artist, &m.title).cloned());
    (song, false)
}

/// The setting is on, the match isn't on a studio album, and no correction names its album.
fn wants_album_version(m: &Match, song: Option<&Value>) -> bool {
    let g = RECOGNIZER.lock().unwrap();
    let Some(r) = g.as_ref() else { return false };
    let corrected = r.lock.corrections.contains_key(&coverussi_app::keys::album_key(&m.artist, m.album.as_deref()));
    r.prefer_album && !corrected && !song.is_some_and(itunes::on_album)
}

fn album_version(m: &Match) -> Option<Match> {
    let answer = json_get(&itunes::versions_url(&m.artist, &m.title, &country()))?;
    let song = itunes::album_version(&answer, &m.artist, &m.title)?;
    let v = itunes::use_album_version(m, song);
    info!("album version: {:?} instead of {:?}", v.album.as_deref().unwrap_or(""), m.album.as_deref().unwrap_or(""));
    Some(v)
}

fn album_tracks(collection_id: Option<&str>) -> Vec<String> {
    let Some(id) = collection_id else { return Vec::new() };
    if let Some(t) = RECOGNIZER.lock().unwrap().as_ref().and_then(|r| r.tracks.get(id)) {
        return t.clone();
    }
    let Some(answer) = json_get(&itunes::album_url(id, &country())) else { return Vec::new() };
    let tracks = itunes::album_tracks(&answer);
    if let Some(r) = RECOGNIZER.lock().unwrap().as_mut() {
        if r.tracks.len() >= 16 {
            r.tracks.clear();
        }
        r.tracks.insert(id.to_string(), tracks.clone());
    }
    tracks
}

fn song_art(artist: &str, title: &str) -> Option<(Option<String>, Option<String>)> {
    if artist.is_empty() {
        return None;
    }
    let answer = json_get(&itunes::search_url(artist, title, &country()))?;
    itunes::song_art(&answer, artist, title)
}
