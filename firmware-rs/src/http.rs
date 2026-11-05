//! HTTP(S) requests with whole bodies: art, Shazam, iTunes, Home Assistant.

use std::time::Duration;

use esp_idf_svc::http::client::{Configuration, EspHttpConnection};
use esp_idf_svc::http::Method;
use esp_idf_svc::sys;

pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}

fn connect(url: &str, timeout: Duration) -> Result<EspHttpConnection, sys::EspError> {
    EspHttpConnection::new(&Configuration {
        timeout: Some(timeout),
        buffer_size: Some(4096),
        buffer_size_tx: Some(2048),
        crt_bundle_attach: url.starts_with("https://").then_some(sys::esp_crt_bundle_attach as _),
        ..Default::default()
    })
}

/// `limit`: the largest body accepted; a bigger one is an error.
pub fn request(
    method: Method,
    url: &str,
    headers: &[(&str, &str)],
    body: &[u8],
    limit: usize,
    timeout: Duration,
) -> Result<Response, String> {
    let mut c = connect(url, timeout).map_err(|e| e.to_string())?;
    let length = body.len().to_string();
    let mut all: Vec<(&str, &str)> = headers.to_vec();
    if method != Method::Get {
        all.push(("Content-Length", &length));
    }
    c.initiate_request(method, url, &all).map_err(|e| e.to_string())?;
    if !body.is_empty() {
        c.write_all(body).map_err(|e| e.to_string())?;
    }
    c.initiate_response().map_err(|e| e.to_string())?;
    let status = c.status();
    let size = c.header("Content-Length").and_then(|n| n.parse::<usize>().ok());
    if size.is_some_and(|n| n > limit) {
        return Err(format!("{} bytes is too big", size.unwrap()));
    }
    let mut out = Vec::with_capacity(size.unwrap_or(16 * 1024));
    let mut chunk = [0u8; 2048];
    loop {
        let n = c.read(&mut chunk).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        if out.len() + n > limit {
            return Err("response too big".into());
        }
        out.extend_from_slice(&chunk[..n]);
    }
    Ok(Response { status, body: out })
}

pub fn get(url: &str, headers: &[(&str, &str)], limit: usize) -> Result<Response, String> {
    request(Method::Get, url, headers, &[], limit, Duration::from_secs(15))
}
