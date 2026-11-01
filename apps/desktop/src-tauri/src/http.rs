//! Small HTTP helper for remote inference.
//!
//! We deliberately do not bundle an HTTP/TLS stack in the host: the desktop app
//! uses the system `curl` (present on macOS and Windows 10+). The API key is
//! written to a 0600 temporary config file, never to argv, and the file is
//! removed before the call returns. Tests use the real curl binary against a
//! loopback server; a future iteration can replace this with rustls.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

pub trait HttpTransport: Send + Sync {
    fn post_json(&self, url: &str, api_key: &str, body: &str) -> Result<HttpResponse, String>;
}

pub struct CurlTransport;

struct TempWorkspace {
    path: PathBuf,
}

impl TempWorkspace {
    fn new() -> Result<TempWorkspace, String> {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let path = std::env::temp_dir().join(format!("poie-inference-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&path).map_err(|error| format!("temp dir: {error}"))?;
        restrict(&path);
        Ok(TempWorkspace { path })
    }

    fn file(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[cfg(unix)]
fn restrict(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mode = if path.is_dir() { 0o700 } else { 0o600 };
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
}

#[cfg(not(unix))]
fn restrict(_path: &Path) {}

fn config_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

impl HttpTransport for CurlTransport {
    fn post_json(&self, url: &str, api_key: &str, body: &str) -> Result<HttpResponse, String> {
        let workspace = TempWorkspace::new()?;
        let body_path = workspace.file("request.json");
        let response_path = workspace.file("response.json");
        let config_path = workspace.file("curl.conf");
        std::fs::write(&body_path, body).map_err(|error| format!("request body: {error}"))?;
        restrict(&body_path);
        let config = format!(
            "silent\nshow-error\nmax-time = 30\noutput = {}\nwrite-out = {}\nurl = {}\nheader = {}\nheader = {}\ndata-binary = {}\n",
            config_quote(&response_path.display().to_string()),
            config_quote("%{http_code}"),
            config_quote(url),
            config_quote("Content-Type: application/json"),
            config_quote(&format!("Authorization: Bearer {api_key}")),
            config_quote(&format!("@{}", body_path.display())),
        );
        std::fs::write(&config_path, config).map_err(|error| format!("curl config: {error}"))?;
        restrict(&config_path);

        let output = Command::new("curl")
            .arg("--config")
            .arg(&config_path)
            .env("NO_PROXY", "127.0.0.1,localhost")
            .env("no_proxy", "127.0.0.1,localhost")
            .output()
            .map_err(|error| format!("system curl is unavailable: {error}"))?;
        if !output.status.success() {
            return Err(format!("curl failed: {}", String::from_utf8_lossy(&output.stderr).trim()));
        }
        let status = String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<u16>()
            .map_err(|_| "curl returned no HTTP status".to_string())?;
        let body = std::fs::read_to_string(&response_path).unwrap_or_default();
        Ok(HttpResponse { status, body })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn read_request(stream: &mut std::net::TcpStream) -> String {
        let mut data = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            let count = stream.read(&mut buffer).unwrap_or(0);
            if count == 0 {
                break;
            }
            data.extend_from_slice(&buffer[..count]);
            if let Some(head_end) = data.windows(4).position(|window| window == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&data[..head_end + 4]).to_lowercase();
                let content_length = head
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .and_then(|value| value.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                if data.len() >= head_end + 4 + content_length {
                    break;
                }
            }
        }
        String::from_utf8_lossy(&data).to_string()
    }

    #[test]
    fn curl_posts_json_with_bearer_header() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            assert!(request.starts_with("POST /v1/chat/completions HTTP/1.1"), "{request}");
            assert!(request.to_lowercase().contains("authorization: bearer test-key"), "{request}");
            assert!(request.contains("\"model\":\"test-model\""), "{request}");
            let body = r#"{"choices":[{"message":{"content":"pong"}}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        let transport = CurlTransport;
        let response = transport
            .post_json(
                &format!("http://{address}/v1/chat/completions"),
                "test-key",
                r#"{"model":"test-model","messages":[{"role":"user","content":"ping"}]}"#,
            )
            .unwrap();
        server.join().unwrap();
        assert_eq!(response.status, 200);
        assert!(response.body.contains("pong"));
    }
}
