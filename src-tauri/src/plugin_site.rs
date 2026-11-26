//! Tiny loopback HTTP server for bundled plugin sites.
//!
//! A Tauri asset-protocol URL encodes the whole Windows path into one URL
//! segment, so `css/style.css` and `assets/terrain/*` resolve against the
//! origin root instead of the site directory. Serving the plugin root over a
//! loopback HTTP listener keeps relative paths intact and gives the iframe a
//! real origin. The listener is bound to 127.0.0.1 only and serves files below
//! the plugin directory; no external interface is exposed.

use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

pub struct SiteServer {
    addr: SocketAddr,
    root: PathBuf,
}

impl SiteServer {
    pub fn start(root: PathBuf) -> Result<Arc<Self>, String> {
        fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|error| error.to_string())?;
        let addr = listener.local_addr().map_err(|error| error.to_string())?;
        let server = Arc::new(Self { addr, root });
        let worker = server.clone();
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let server = worker.clone();
                thread::spawn(move || {
                    let _ = server.serve(stream);
                });
            }
        });
        Ok(server)
    }

    pub fn url_for(&self, id: &str, local: &str) -> Option<String> {
        if !safe_segment(id)
            || local.trim().is_empty()
            || local.starts_with('/')
            || local.contains('\\')
            || local.split('/').any(|part| part == ".." || part == ".")
        {
            return None;
        }
        let path = self.root.join(id).join("site").join(local);
        if !path.is_file() {
            return None;
        }
        Some(format!(
            "http://127.0.0.1:{}/{}/site/{}",
            self.addr.port(),
            id,
            local
        ))
    }

    fn serve(&self, mut stream: TcpStream) -> Result<(), String> {
        let mut request = [0u8; 8192];
        let read = stream.read(&mut request).map_err(|error| error.to_string())?;
        if read == 0 {
            return Ok(());
        }
        let line = String::from_utf8_lossy(&request[..read]);
        let raw_path = line.split_whitespace().nth(1).unwrap_or("/");
        let path = percent_decode(raw_path.split('?').next().unwrap_or("/"));
        let relative = path.trim_start_matches('/');
        if relative.is_empty()
            || relative.split('/').any(|part| part == ".." || part == ".")
            || relative.contains('\\')
        {
            return write_response(&mut stream, 404, "text/plain", b"not found");
        }
        let target = self.root.join(relative);
        if !target.is_file() {
            return write_response(&mut stream, 404, "text/plain", b"not found");
        }
        let bytes = fs::read(&target).map_err(|error| error.to_string())?;
        write_response(&mut stream, 200, content_type(&target), &bytes)
    }
}

fn write_response(stream: &mut TcpStream, status: u16, mime: &str, body: &[u8]) -> Result<(), String> {
    let reason = if status == 200 { "OK" } else { "Not Found" };
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nCache-Control: max-age=300\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(header.as_bytes())
        .and_then(|_| stream.write_all(body))
        .map_err(|error| error.to_string())
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|value| value.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" | "webmanifest" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "webp" => "image/webp",
        "jpg" | "jpeg" => "image/jpeg",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn safe_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-'))
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let high = hex(bytes[index + 1]);
            let low = hex(bytes[index + 2]);
            if let (Some(high), Some(low)) = (high, low) {
                out.push(high * 16 + low);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_site_files_and_rejects_traversal() {
        let root = std::env::temp_dir().join(format!("yebian-site-server-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("demo.plugin/site/css")).unwrap();
        fs::write(root.join("demo.plugin/site/index.html"), "<html>ok</html>").unwrap();
        fs::write(root.join("demo.plugin/site/css/app.css"), "body{}").unwrap();
        let server = SiteServer::start(root.clone()).unwrap();
        let base = server.url_for("demo.plugin", "index.html").unwrap();
        let body = get(&base);
        assert!(body.contains("<html>ok</html>"));
        let css = get(&format!("{}css/app.css", base.trim_end_matches("index.html")));
        assert!(css.contains("body{}"));
        let traversal = format!("http://127.0.0.1:{}/../server.sqlite", server.addr.port());
        assert!(!get(&traversal).contains("sqlite"));
        let _ = fs::remove_dir_all(root);
    }

    fn get(url: &str) -> String {
        let address = url
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap_or_default();
        let path = format!("/{}", url.splitn(4, '/').nth(3).unwrap_or_default());
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .write_all(format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").as_bytes())
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response.split("\r\n\r\n").nth(1).unwrap_or("").to_string()
    }
}
