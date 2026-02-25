//! Sunshine stream connection detection.
//!
//! Detects whether top/bottom Sunshine instances have active streaming clients.
//! Replaces the curl/ss/proc checks from screen-tool-visibility.sh.

use std::io::Read;
use std::net::TcpStream;
use std::time::Duration;

/// Sunshine ports for the dual-screen setup.
const TOP_SERVERINFO_PORT: u16 = 47989;
const BOTTOM_SERVERINFO_PORT: u16 = 48089;

/// Connection timeout for HTTP checks (keep very short to avoid blocking the main loop).
const HTTP_TIMEOUT: Duration = Duration::from_millis(250);

pub struct StreamDetector {
    #[allow(dead_code)]
    top_port: u16,
    bottom_port: u16,
}

impl StreamDetector {
    pub fn new() -> Self {
        Self {
            top_port: TOP_SERVERINFO_PORT,
            bottom_port: BOTTOM_SERVERINFO_PORT,
        }
    }

    /// Check if the top Sunshine instance has an active stream.
    #[allow(dead_code)]
    pub fn is_top_stream_connected(&self) -> bool {
        is_stream_active(self.top_port)
    }

    /// Check if the bottom Sunshine instance has an active stream.
    pub fn is_bottom_stream_connected(&self) -> bool {
        is_stream_active(self.bottom_port)
    }
}

/// Check if a Sunshine instance at the given port has an active stream.
/// Tries HTTP serverinfo API first, falls back to TCP socket check, then /proc/net/tcp.
fn is_stream_active(port: u16) -> bool {
    // Method 1: HTTP serverinfo API (most reliable)
    if let Some(active) = check_sunshine_serverinfo(port) {
        return active;
    }

    // Method 2: Check for established TCP connections on the port
    if check_tcp_connections(port) {
        return true;
    }

    false
}

/// Query Sunshine's /serverinfo endpoint for stream state.
/// Returns None if the request fails (Sunshine not running, etc.).
fn check_sunshine_serverinfo(port: u16) -> Option<bool> {
    let addr = format!("127.0.0.1:{}", port);
    let mut stream = TcpStream::connect_timeout(&addr.parse().ok()?, HTTP_TIMEOUT).ok()?;
    stream
        .set_read_timeout(Some(HTTP_TIMEOUT))
        .ok()?;
    stream
        .set_write_timeout(Some(HTTP_TIMEOUT))
        .ok()?;

    // Send a minimal HTTP request
    let request = format!("GET /serverinfo HTTP/1.0\r\nHost: 127.0.0.1:{}\r\n\r\n", port);
    std::io::Write::write_all(&mut stream, request.as_bytes()).ok()?;

    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;

    // Check for active game session: <currentgame>N</currentgame> where N > 0
    if let Some(start) = response.find("<currentgame>") {
        let after = &response[start + 13..];
        if let Some(end) = after.find("</currentgame>") {
            let val = after[..end].trim();
            if val != "0" && !val.is_empty() {
                return Some(true);
            }
        }
    }

    // Check state field
    if let Some(start) = response.find("<state>") {
        let after = &response[start + 7..];
        if let Some(end) = after.find("</state>") {
            let state = after[..end].trim();
            if state.contains("BUSY") || state.contains("STREAM") || state.contains("IN_GAME") {
                return Some(true);
            }
        }
    }

    Some(false)
}

/// Check /proc/net/tcp for established connections on the given port.
/// Works in minimal containers without `ss` or `netstat`.
fn check_tcp_connections(port: u16) -> bool {
    let port_hex = format!("{:04X}", port);

    for path in &["/proc/net/tcp", "/proc/net/tcp6"] {
        if let Ok(content) = std::fs::read_to_string(path) {
            for line in content.lines().skip(1) {
                let fields: Vec<&str> = line.split_whitespace().collect();
                if fields.len() < 4 {
                    continue;
                }
                // Field 1 is local address (hex_ip:hex_port), field 3 is state
                let local = fields[1];
                let state = fields[3];
                // State "01" = ESTABLISHED
                if state == "01" {
                    if let Some(lport) = local.rsplit(':').next() {
                        if lport.eq_ignore_ascii_case(&port_hex) {
                            return true;
                        }
                    }
                }
            }
        }
    }

    false
}
