//! The preview HTTP server (spec Section 5, TL-09).
//!
//! Binds 127.0.0.1 on a random port (never file://, never the network),
//! resolves every request strictly inside the project root, serves
//! per-folder index.html files, applies `_headers` rules, disables caching
//! and injects the selection bridge plus element annotations into HTML.
//! The server stops when the project closes or the panel is closed.

use crate::headers::HeaderRules;
use hsw_edit::rewrite::annotate_and_inject;
use std::io::Write as _;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

/// Injected into every served HTML page. Reports clicks back to the parent
/// interface with the element index (the `data-hsw-e` annotation), the
/// selected text node's ordinal inside that element, and media references.
pub const BRIDGE_SCRIPT: &str = r#"<script data-hsw-bridge>(function () {
  if (window.__hswBridge) return;
  window.__hswBridge = true;
  function textOrdinal(root, node) {
    if (!node || node.nodeType !== 3) return null;
    var n = 0, found = false;
    (function walk(el) {
      for (var i = 0; i < el.childNodes.length && !found; i++) {
        var child = el.childNodes[i];
        if (child === node) { found = true; return; }
        if (child.nodeType === 3) n++;
        else if (child.nodeType === 1) walk(child);
      }
    })(root);
    return found ? n : null;
  }
  function mediaInfo(el) {
    var tag = el.tagName.toLowerCase();
    if (tag === 'img') return { kind: 'image', src: el.getAttribute('src') || '', srcset: el.getAttribute('srcset') || '' };
    if (tag === 'video') return { kind: 'video', src: el.currentSrc || el.getAttribute('src') || '', poster: el.getAttribute('poster') || '' };
    if (tag === 'source') return { kind: 'source', src: el.getAttribute('src') || '', srcset: el.getAttribute('srcset') || '' };
    var style = window.getComputedStyle(el).backgroundImage;
    if (style && style !== 'none') {
      var m = style.match(/url\(["']?([^"')]+)["']?\)/);
      if (m) return { kind: 'css-background', src: m[1] };
    }
    return null;
  }
  function send(payload) {
    payload.type = 'hsw-select';
    payload.page = location.pathname;
    window.parent.postMessage(payload, '*');
  }
  document.addEventListener('click', function (ev) {
    var annotated = ev.target.closest ? ev.target.closest('[data-hsw-e]') : null;
    if (!annotated) return;
    ev.preventDefault();
    ev.stopPropagation();
    var payload = {
      elementIndex: parseInt(annotated.getAttribute('data-hsw-e'), 10),
      tag: annotated.tagName.toLowerCase(),
      text: null,
      textOrdinal: null,
      media: mediaInfo(annotated)
    };
    if (document.caretRangeFromPoint) {
      var range = document.caretRangeFromPoint(ev.clientX, ev.clientY);
      if (range && range.startContainer.nodeType === 3) {
        var anchor = range.startContainer.parentElement
          ? range.startContainer.parentElement.closest('[data-hsw-e]')
          : null;
        if (anchor) {
          payload.elementIndex = parseInt(anchor.getAttribute('data-hsw-e'), 10);
          payload.tag = anchor.tagName.toLowerCase();
          payload.text = range.startContainer.textContent;
          payload.textOrdinal = textOrdinal(anchor, range.startContainer);
          payload.media = mediaInfo(anchor);
        }
      }
    }
    send(payload);
  }, true);
  window.addEventListener('message', function (event) {
    var data = event.data || {};
    if (data.type === 'hsw-navigate' && typeof data.url === 'string') { location.href = data.url; }
    if (data.type === 'hsw-scroll-to-element' && typeof data.elementIndex === 'number') {
      var el = document.querySelector('[data-hsw-e="' + data.elementIndex + '"]');
      if (el) {
        el.scrollIntoView({ behavior: 'smooth', block: 'center' });
        el.classList.add('__hsw-flash');
        setTimeout(function () { el.classList.remove('__hsw-flash'); }, 1200);
      }
    }
  });
})();</script>"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerInfo {
    pub port: u16,
    pub url: String,
}

pub struct PreviewServer {
    info: ServerInfo,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl PreviewServer {
    /// Starts the server for `project_root` on a random localhost port.
    pub fn start(project_root: &Path) -> std::io::Result<PreviewServer> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        let server = Arc::new(
            tiny_http::Server::from_listener(listener, None)
                .map_err(|error| std::io::Error::other(error.to_string()))?,
        );
        let stop = Arc::new(AtomicBool::new(false));
        let rules = HeaderRules::load(&project_root.join("_headers"));
        let root = project_root.to_path_buf();
        let stop_flag = stop.clone();
        let server_ref = server.clone();
        let thread = std::thread::spawn(move || {
            // stop() sends a dummy request so recv() returns and the flag
            // check ends the loop immediately.
            while let Ok(request) = server_ref.recv() {
                if stop_flag.load(Ordering::SeqCst) {
                    break;
                }
                handle_request(&root, &rules, request);
            }
        });
        Ok(PreviewServer {
            info: ServerInfo { port, url: format!("http://127.0.0.1:{port}") },
            stop,
            thread: Some(thread),
        })
    }

    pub fn info(&self) -> &ServerInfo {
        &self.info
    }

    /// Stops the server and joins its thread. A dummy self-connection
    /// unblocks the listening loop so shutdown is immediate.
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Ok(mut stream) = std::net::TcpStream::connect(("127.0.0.1", self.info.port)) {
            let _ = stream.write_all(
                b"GET /__hsw_stop HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
            );
            let _ = stream.flush();
        }
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("json" | "map") => "application/json",
        Some("webmanifest") => "application/manifest+json",
        Some("xml") => "application/xml",
        Some("txt" | "md") => "text/plain; charset=utf-8",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("avif") => "image/avif",
        Some("ico") => "image/x-icon",
        Some("bmp") => "image/bmp",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        Some("mov") => "video/quicktime",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        _ => "application/octet-stream",
    }
}

fn is_html(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(),
        Some("html" | "htm")
    )
}

/// Percent-decodes a request path. Invalid escapes decode to '?' which
/// cannot appear in a valid relative path, so malformed input fails
/// resolution instead of resolving somewhere unexpected.
fn percent_decode(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = &path[i + 1..i + 3];
            if let Ok(value) = u8::from_str_radix(hex, 16) {
                out.push(value);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn header(name: &str, value: &str) -> Option<tiny_http::Header> {
    tiny_http::Header::from_bytes(name.as_bytes(), value.as_bytes()).ok()
}

fn respond(
    request: tiny_http::Request,
    status: u16,
    body: Vec<u8>,
    content_type_value: &'static str,
    extra: Vec<(String, String)>,
) {
    let mut response = tiny_http::Response::from_data(body)
        .with_status_code(status);
    if let Some(header) = header("Content-Type", content_type_value) {
        response.add_header(header);
    }
    if let Some(header) = header("Cache-Control", "no-store") {
        response.add_header(header);
    }
    if let Some(header) = header("X-Content-Type-Options", "nosniff") {
        response.add_header(header);
    }
    for (name, value) in extra {
        if let Some(header) = header(&name, &value) {
            response.add_header(header);
        }
    }
    let _ = request.respond(response);
}

fn handle_request(root: &Path, rules: &HeaderRules, request: tiny_http::Request) {
    let method = request.method().clone();
    let raw_url = request.url().to_string();
    let path = raw_url.split('?').next().unwrap_or("/").to_string();

    if path == "/__hsw_stop" {
        respond(request, 200, b"stopping".to_vec(), "text/plain", vec![]);
        return;
    }
    if method != tiny_http::Method::Get && method != tiny_http::Method::Head {
        respond(request, 405, b"method not allowed".to_vec(), "text/plain", vec![]);
        return;
    }

    let decoded = percent_decode(&path);
    let rel_text = decoded.trim_start_matches('/').replace('\\', "/");
    let rel = PathBuf::from(&rel_text);

    let resolved = hsw_core::fsutil::safe_join(root, &rel);
    let target = match resolved {
        Ok(target) => target,
        Err(_) => {
            respond(
                request,
                400,
                b"invalid request path".to_vec(),
                "text/plain",
                vec![],
            );
            return;
        }
    };

    // Directories serve their index.html; files that do not exist fall
    // through to the project's 404 page (never a directory listing).
    let mut target = target;
    if target.is_dir() {
        target = target.join("index.html");
    }
    if !target.is_file() {
        let fallback = root.join("404.html");
        let extra = rules.matching(&path);
        if fallback.is_file() {
            if let Ok(body) = std::fs::read(&fallback) {
                respond(request, 404, body, "text/html; charset=utf-8", extra);
                return;
            }
        }
        respond(request, 404, b"404 - not found".to_vec(), "text/plain", extra);
        return;
    }

    let extra = rules.matching(&path);
    match std::fs::read(&target) {
        Ok(mut body) => {
            let content_type_value = content_type(&target);
            if is_html(&target) {
                match String::from_utf8(body) {
                    Ok(text) => {
                        let transformed = annotate_and_inject(&text, BRIDGE_SCRIPT);
                        body = transformed.into_bytes();
                    }
                    Err(error) => body = error.into_bytes(),
                }
            }
            respond(request, 200, body, content_type_value, extra);
        }
        Err(err) => {
            respond(
                request,
                500,
                format!("cannot read file: {err}").into_bytes(),
                "text/plain",
                extra,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    // `Read` only; `Write` already arrives through `use super::*`.
    use std::io::Read as _;

    fn fixture() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("index.html"),
            "<!doctype html><html><head><title>Home</title></head><body><h1>Welcome</h1><img src=\"image/logo.png\"></body></html>",
        )
        .unwrap();
        fs::create_dir_all(tmp.path().join("about")).unwrap();
        fs::write(tmp.path().join("about/index.html"), "<html><head></head><body><p>About</p></body></html>").unwrap();
        fs::write(tmp.path().join("404.html"), "<html><body><h1>Custom 404</h1></body></html>").unwrap();
        fs::create_dir_all(tmp.path().join("assets/css")).unwrap();
        fs::write(tmp.path().join("assets/css/style.css"), "body { color: #222; }").unwrap();
        fs::write(
            tmp.path().join("_headers"),
            "/assets/*\n  X-Preview: workshop\n",
        )
        .unwrap();
        fs::write(tmp.path().join("secret.txt"), "TOP SECRET").unwrap();
        tmp
    }

    fn http_get(port: u16, target: &str) -> (u16, String, String) {
        let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(
            stream,
            "GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        let mut raw = String::new();
        stream.read_to_string(&mut raw).unwrap();
        let mut parts = raw.splitn(2, "\r\n\r\n");
        let head = parts.next().unwrap_or("").to_string();
        let raw_body = parts.next().unwrap_or("").to_string();
        let status: u16 = head
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        // tiny_http may answer with chunked transfer encoding; de-chunk so
        // the assertions see the actual payload.
        let body = if head.to_ascii_lowercase().contains("transfer-encoding: chunked") {
            dechunk(&raw_body)
        } else {
            raw_body
        };
        (status, head, body)
    }

    fn dechunk(data: &str) -> String {
        let mut out = String::new();
        let mut cursor = 0usize;
        while let Some(line_end) = data[cursor..].find("\r\n") {
            let size_text = &data[cursor..cursor + line_end];
            let Ok(size) = usize::from_str_radix(size_text.trim().split(';').next().unwrap_or("0"), 16) else { break };
            if size == 0 {
                break;
            }
            let start = cursor + line_end + 2;
            let end = start + size;
            if end > data.len() {
                break;
            }
            out.push_str(&data[start..end]);
            cursor = end + 2; // trailing CRLF of the chunk
        }
        out
    }

    #[test]
    fn serves_annotated_index_with_bridge() {
        let tmp = fixture();
        let server = PreviewServer::start(tmp.path()).unwrap();
        let port = server.info().port;
        let (status, head, body) = http_get(port, "/");
        assert_eq!(status, 200);
        assert!(head.contains("text/html"));
        assert!(head.contains("no-store"), "caching must be disabled");
        assert!(body.contains("data-hsw-e=\""), "element annotation missing");
        assert!(body.contains("__hswBridge"), "bridge script missing");
        assert!(body.contains(">Welcome</h1>"), "annotated h1 must keep its text");
        server.stop();
    }

    #[test]
    fn directory_paths_serve_their_index() {
        let tmp = fixture();
        let server = PreviewServer::start(tmp.path()).unwrap();
        let (status, _, body) = http_get(server.info().port, "/about");
        assert_eq!(status, 200);
        assert!(body.contains(">About</p>"), "annotated p must keep its text");
        server.stop();
    }

    #[test]
    fn missing_paths_serve_the_project_404_page() {
        let tmp = fixture();
        let server = PreviewServer::start(tmp.path()).unwrap();
        let (status, _, body) = http_get(server.info().port, "/no/such/page");
        assert_eq!(status, 404);
        assert!(body.contains("Custom 404"));
        server.stop();
    }

    #[test]
    fn headers_rules_are_applied() {
        let tmp = fixture();
        let server = PreviewServer::start(tmp.path()).unwrap();
        let (status, head, _) = http_get(server.info().port, "/assets/css/style.css");
        assert_eq!(status, 200);
        assert!(head.contains("X-Preview: workshop"));
        assert!(head.contains("text/css"));
        server.stop();
    }

    #[test]
    fn traversal_is_refused_and_files_stay_root_locked() {
        let tmp = fixture();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("outside.txt"), "MUST NOT LEAK").unwrap();
        let server = PreviewServer::start(tmp.path()).unwrap();
        let port = server.info().port;
        let (status, _, body) = http_get(port, "/..%2f..%2foutside.txt");
        assert!(status == 400 || status == 404, "traversal must not serve: {status}");
        assert!(!body.contains("MUST NOT LEAK"));
        let (status, _, body) = http_get(port, "/../../outside.txt");
        assert!(status == 400 || status == 404);
        assert!(!body.contains("MUST NOT LEAK"));
        server.stop();
    }

    #[test]
    fn directories_without_index_are_never_listed() {
        let tmp = fixture();
        let server = PreviewServer::start(tmp.path()).unwrap();
        fs::create_dir_all(tmp.path().join("empty-dir")).unwrap();
        let (status, _, _) = http_get(server.info().port, "/empty-dir/");
        assert_eq!(status, 404);
        server.stop();
    }

    #[test]
    fn stop_terminates_the_listener() {
        let tmp = fixture();
        let server = PreviewServer::start(tmp.path()).unwrap();
        let port = server.info().port;
        let (status, _, _) = http_get(port, "/");
        assert_eq!(status, 200);
        server.stop();
        // A restart on the same port pattern is not required; just ensure
        // stop() returned cleanly (join finished).
    }
}
