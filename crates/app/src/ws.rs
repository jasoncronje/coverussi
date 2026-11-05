//! A WebSocket client's framing (RFC 6455), for Home Assistant's API. The
//! board reads bytes off TCP or TLS and feeds them here; whole messages come
//! out however the server or the network split them.

pub const TEXT: u8 = 1;
pub const BINARY: u8 = 2;
pub const CLOSE: u8 = 8;
pub const PING: u8 = 9;
pub const PONG: u8 = 10;

/// The opening request. `key`: 16 random bytes, base64.
pub fn handshake(host: &str, path: &str, key: &str) -> String {
    format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
         Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\nUser-Agent: Coverussi\r\n\r\n"
    )
}

/// Where the response head ends (after its blank line), once it has.
pub fn head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4)
}

/// The server agreed to switch protocols.
pub fn accepted(head: &[u8]) -> bool {
    head.starts_with(b"HTTP/1.1 101")
}

/// A client frame (always masked).
pub fn frame(opcode: u8, payload: &[u8], mask: [u8; 4]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 14);
    out.push(0x80 | opcode);
    match payload.len() {
        n if n < 126 => out.push(0x80 | n as u8),
        n if n <= 0xFFFF => {
            out.push(0x80 | 126);
            out.extend_from_slice(&(n as u16).to_be_bytes());
        }
        n => {
            out.push(0x80 | 127);
            out.extend_from_slice(&(n as u64).to_be_bytes());
        }
    }
    out.extend_from_slice(&mask);
    out.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
    out
}

#[derive(Debug, PartialEq)]
pub enum Message {
    Text(String),
    Binary(Vec<u8>),
    /// Answer with a pong carrying the same payload.
    Ping(Vec<u8>),
    Close,
}

#[derive(Debug, PartialEq)]
pub enum Error {
    /// A frame or message over the limit.
    TooBig,
    /// Masked, reserved bits, a bad opcode, text that isn't UTF-8.
    Protocol,
}

/// Reassembles server frames into messages.
pub struct Decoder {
    buf: Vec<u8>,
    /// A fragmented message so far: its opcode and payload.
    partial: Option<(u8, Vec<u8>)>,
    limit: usize,
}

impl Decoder {
    /// `limit`: the largest message accepted.
    pub fn new(limit: usize) -> Self {
        Self { buf: Vec::new(), partial: None, limit }
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// The next whole message, if the bytes so far hold one.
    pub fn message(&mut self) -> Result<Option<Message>, Error> {
        loop {
            let Some((fin, opcode, payload)) = self.frame()? else { return Ok(None) };
            match opcode {
                PING => return Ok(Some(Message::Ping(payload))),
                PONG => continue,
                CLOSE => return Ok(Some(Message::Close)),
                0 => {
                    let Some((_, so_far)) = self.partial.as_mut() else { return Err(Error::Protocol) };
                    if so_far.len() + payload.len() > self.limit {
                        return Err(Error::TooBig);
                    }
                    so_far.extend_from_slice(&payload);
                    if fin {
                        let (op, all) = self.partial.take().unwrap();
                        return message(op, all).map(Some);
                    }
                }
                TEXT | BINARY if fin => return message(opcode, payload).map(Some),
                TEXT | BINARY => self.partial = Some((opcode, payload)),
                _ => return Err(Error::Protocol),
            }
        }
    }

    fn frame(&mut self) -> Result<Option<(bool, u8, Vec<u8>)>, Error> {
        let b = &self.buf;
        if b.len() < 2 {
            return Ok(None);
        }
        if b[0] & 0x70 != 0 || b[1] & 0x80 != 0 {
            return Err(Error::Protocol);
        }
        let (len, at) = match b[1] & 0x7F {
            126 if b.len() >= 4 => (u16::from_be_bytes([b[2], b[3]]) as u64, 4),
            127 if b.len() >= 10 => (u64::from_be_bytes(b[2..10].try_into().unwrap()), 10),
            126 | 127 => return Ok(None),
            n => (n as u64, 2),
        };
        if len > self.limit as u64 {
            return Err(Error::TooBig);
        }
        let end = at + len as usize;
        if b.len() < end {
            return Ok(None);
        }
        let (fin, opcode) = (b[0] & 0x80 != 0, b[0] & 0x0F);
        let payload = b[at..end].to_vec();
        self.buf.drain(..end);
        Ok(Some((fin, opcode, payload)))
    }
}

fn message(opcode: u8, payload: Vec<u8>) -> Result<Message, Error> {
    if opcode == TEXT {
        String::from_utf8(payload).map(Message::Text).map_err(|_| Error::Protocol)
    } else {
        Ok(Message::Binary(payload))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A server frame: unmasked.
    fn server(fin: bool, opcode: u8, payload: &[u8]) -> Vec<u8> {
        let mut f = frame(opcode, payload, [0; 4]);
        f[0] = if fin { 0x80 | opcode } else { opcode };
        f[1] &= 0x7F;
        let at = match f[1] {
            126 => 4,
            127 => 10,
            _ => 2,
        };
        f.drain(at..at + 4);
        f
    }

    #[test]
    fn frames_are_masked_with_the_right_length() {
        let f = frame(TEXT, b"hi", [1, 2, 3, 4]);
        assert_eq!(f, [0x81, 0x82, 1, 2, 3, 4, b'h' ^ 1, b'i' ^ 2]);
        assert_eq!(&frame(TEXT, &[0; 300], [0; 4])[..4], [0x81, 0xFE, 1, 44]);
        assert_eq!(&frame(BINARY, &vec![0; 70_000], [0; 4])[..10], [0x82, 0xFF, 0, 0, 0, 0, 0, 1, 0x11, 0x70]);
    }

    #[test]
    fn messages_come_out_whole_however_they_arrive() {
        let big = "x".repeat(70_000);
        let mut bytes = server(true, TEXT, br#"{"type":"auth_required"}"#);
        bytes.extend(server(true, TEXT, big.as_bytes()));
        bytes.extend(server(false, TEXT, b"{\"a\":"));
        bytes.extend(server(true, PING, b"p"));
        bytes.extend(server(true, 0, b"1}"));
        let mut d = Decoder::new(1 << 20);
        let mut got = Vec::new();
        for chunk in bytes.chunks(1000) {
            d.feed(chunk);
            while let Some(m) = d.message().unwrap() {
                got.push(m);
            }
        }
        assert_eq!(got.len(), 4);
        assert_eq!(got[0], Message::Text(r#"{"type":"auth_required"}"#.into()));
        assert_eq!(got[1], Message::Text(big));
        assert_eq!(got[2], Message::Ping(b"p".to_vec())); // control frames may come mid-message
        assert_eq!(got[3], Message::Text(r#"{"a":1}"#.into()));
    }

    #[test]
    fn refuses_what_it_should() {
        let mut d = Decoder::new(100);
        d.feed(&server(true, TEXT, &[b'x'; 200]));
        assert_eq!(d.message(), Err(Error::TooBig));
        let mut d = Decoder::new(100);
        d.feed(&frame(TEXT, b"masked", [1, 2, 3, 4]));
        assert_eq!(d.message(), Err(Error::Protocol));
        let mut d = Decoder::new(100);
        d.feed(&server(true, CLOSE, &[3, 232]));
        assert_eq!(d.message(), Ok(Some(Message::Close)));
    }

    #[test]
    fn handshake() {
        let h = super::handshake("ha.local:8123", "/api/websocket", "dGhlIHNhbXBsZSBub25jZQ==");
        assert!(h.starts_with("GET /api/websocket HTTP/1.1\r\nHost: ha.local:8123\r\n") && h.ends_with("\r\n\r\n"));
        let resp = b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\n\r\n\x81\x02hi";
        let end = head_end(resp).unwrap();
        assert!(accepted(&resp[..end]));
        assert_eq!(&resp[end..], b"\x81\x02hi");
        assert!(!accepted(b"HTTP/1.1 401 Unauthorized\r\n\r\n"));
    }
}
