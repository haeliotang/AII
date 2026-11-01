//! Minimal HTTP/1.1 GET client over `std::net::TcpStream`.
//!
//! Only what the read-only adapter needs: plain `http://host:port`, a single
//! `GET` with `Connection: close`, `Content-Length`, chunked, or read-to-EOF
//! bodies. No TLS, no redirects, no authentication.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
}

impl Endpoint {
    /// Parses `http://host:port` (an optional trailing `/` is accepted).
    pub fn parse(url: &str) -> Result<Endpoint, String> {
        let rest = url.strip_prefix("http://").ok_or_else(|| format!("only http:// URLs are supported: {url:?}"))?;
        let rest = rest.trim_end_matches('/');
        if rest.contains('/') {
            return Err(format!("base URL must not contain a path: {url:?}"));
        }
        let (host, port) = rest.rsplit_once(':').ok_or_else(|| format!("base URL needs an explicit port: {url:?}"))?;
        let port: u16 = port.parse().map_err(|_| format!("invalid port in {url:?}"))?;
        if host.is_empty() {
            return Err(format!("empty host in {url:?}"));
        }
        Ok(Endpoint { host: host.to_string(), port })
    }
}

#[derive(Debug)]
pub enum HttpError {
    /// The TCP connection could not be established (refused, unreachable, timeout).
    Connect(String),
    /// The connection broke or closed before a complete response arrived.
    Transport(String),
    /// A response arrived but it is not valid HTTP/1.x.
    Malformed(String),
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpError::Connect(detail) => write!(f, "connect failed: {detail}"),
            HttpError::Transport(detail) => write!(f, "transport failed: {detail}"),
            HttpError::Malformed(detail) => write!(f, "malformed response: {detail}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str())
    }
}

pub struct Timeouts {
    pub connect: Duration,
    pub io: Duration,
}

impl Default for Timeouts {
    fn default() -> Timeouts {
        Timeouts { connect: Duration::from_secs(2), io: Duration::from_secs(5) }
    }
}

pub fn get(endpoint: &Endpoint, path: &str, timeouts: &Timeouts) -> Result<Response, HttpError> {
    let addrs: Vec<SocketAddr> = (endpoint.host.as_str(), endpoint.port)
        .to_socket_addrs()
        .map_err(|error| HttpError::Connect(format!("resolve {}: {error}", endpoint.host)))?
        .collect();
    let mut last_error = None;
    let mut stream = None;
    for addr in &addrs {
        match TcpStream::connect_timeout(addr, timeouts.connect) {
            Ok(connected) => {
                stream = Some(connected);
                break;
            }
            Err(error) => last_error = Some(format!("{addr}: {error} ({:?})", error.kind())),
        }
    }
    let mut stream =
        stream.ok_or_else(|| HttpError::Connect(last_error.unwrap_or_else(|| "no address".to_string())))?;
    let transport = |error: std::io::Error| HttpError::Transport(format!("{error} ({:?})", error.kind()));
    stream.set_read_timeout(Some(timeouts.io)).map_err(transport)?;
    stream.set_write_timeout(Some(timeouts.io)).map_err(transport)?;
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {}:{}\r\nAccept: application/json\r\nUser-Agent: poie-basyx-shadow/0.1\r\nConnection: close\r\n\r\n",
        endpoint.host, endpoint.port
    );
    stream.write_all(request.as_bytes()).map_err(transport)?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).map_err(transport)?;
    parse_response(&raw)
}

/// Parses a complete HTTP/1.x response read until the server closed the
/// connection. An empty read means the peer closed without answering.
pub fn parse_response(raw: &[u8]) -> Result<Response, HttpError> {
    if raw.is_empty() {
        return Err(HttpError::Transport("connection closed without a response (0 bytes)".to_string()));
    }
    let split = find(raw, b"\r\n\r\n").ok_or_else(|| {
        HttpError::Transport(format!("connection closed inside the response head ({} bytes)", raw.len()))
    })?;
    let head = std::str::from_utf8(&raw[..split]).map_err(|_| HttpError::Malformed("non-UTF-8 head".into()))?;
    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap_or_default();
    let mut parts = status_line.splitn(3, ' ');
    let version = parts.next().unwrap_or_default();
    if !version.starts_with("HTTP/1.") {
        return Err(HttpError::Malformed(format!("bad status line {status_line:?}")));
    }
    let status: u16 = parts
        .next()
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| HttpError::Malformed(format!("bad status code in {status_line:?}")))?;
    let mut headers = Vec::new();
    for line in lines {
        let (name, value) =
            line.split_once(':').ok_or_else(|| HttpError::Malformed(format!("bad header line {line:?}")))?;
        headers.push((name.trim().to_string(), value.trim().to_string()));
    }
    let mut response = Response { status, headers, body: Vec::new() };
    let rest = &raw[split + 4..];
    let chunked = response.header("transfer-encoding").map(|value| value.eq_ignore_ascii_case("chunked")) == Some(true);
    response.body = if chunked {
        dechunk(rest)?
    } else if let Some(length) = response.header("content-length") {
        let length: usize =
            length.parse().map_err(|_| HttpError::Malformed(format!("bad content-length {length:?}")))?;
        if rest.len() < length {
            return Err(HttpError::Transport(format!("body truncated: {} of {length} bytes", rest.len())));
        }
        rest[..length].to_vec()
    } else {
        rest.to_vec()
    };
    Ok(response)
}

fn dechunk(mut data: &[u8]) -> Result<Vec<u8>, HttpError> {
    let mut body = Vec::new();
    loop {
        let line_end = find(data, b"\r\n").ok_or_else(|| HttpError::Transport("chunked body truncated".into()))?;
        let size_text =
            std::str::from_utf8(&data[..line_end]).map_err(|_| HttpError::Malformed("chunk size".into()))?;
        let size_text = size_text.split(';').next().unwrap_or_default().trim();
        let size = usize::from_str_radix(size_text, 16)
            .map_err(|_| HttpError::Malformed(format!("bad chunk size {size_text:?}")))?;
        data = &data[line_end + 2..];
        if size == 0 {
            return Ok(body);
        }
        if data.len() < size + 2 {
            return Err(HttpError::Transport("chunk truncated".into()));
        }
        body.extend_from_slice(&data[..size]);
        data = &data[size + 2..];
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn endpoints_parse() {
        assert_eq!(Endpoint::parse("http://127.0.0.1:8081/").unwrap().port, 8081);
        assert!(Endpoint::parse("https://x:1").is_err());
        assert!(Endpoint::parse("http://x").is_err());
        assert!(Endpoint::parse("http://x:1/api").is_err());
    }

    #[test]
    fn content_length_and_chunked_bodies_parse() {
        let plain = parse_response(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nhi").unwrap();
        assert_eq!((plain.status, plain.body.as_slice()), (200, &b"hi"[..]));
        let chunked = parse_response(
            b"HTTP/1.1 404 Not Found\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n2;x=y\r\nde\r\n0\r\n\r\n",
        )
        .unwrap();
        assert_eq!((chunked.status, chunked.body.as_slice()), (404, &b"abcde"[..]));
        assert!(matches!(parse_response(b""), Err(HttpError::Transport(_))));
        assert!(matches!(parse_response(b"garbage\r\n\r\n"), Err(HttpError::Malformed(_))));
        assert!(matches!(
            parse_response(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\nhi"),
            Err(HttpError::Transport(_))
        ));
    }

    #[test]
    fn a_local_server_is_read_and_a_closed_port_is_a_connect_error() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 1024];
            let read = socket.read(&mut buffer).unwrap();
            assert!(std::str::from_utf8(&buffer[..read]).unwrap().starts_with("GET /shells HTTP/1.1\r\n"));
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").unwrap();
        });
        let endpoint = Endpoint { host: "127.0.0.1".into(), port };
        let response = get(&endpoint, "/shells", &Timeouts::default()).unwrap();
        assert_eq!(response.body, b"{}");
        server.join().unwrap();

        // The listener is gone now: the port refuses connections.
        let error = get(&endpoint, "/shells", &Timeouts::default()).unwrap_err();
        assert!(matches!(error, HttpError::Connect(_)), "{error}");
    }
}
