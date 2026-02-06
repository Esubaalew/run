//! Dev Server

use crate::v2::Result;
use serde::Serialize;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone)]
pub struct DevServerConfig {
    pub port: u16,

    pub host: String,

    pub websocket: bool,

    pub dashboard: bool,

    pub project_name: String,
}

impl Default for DevServerConfig {
    fn default() -> Self {
        Self {
            port: 3000,
            host: "127.0.0.1".to_string(),
            websocket: true,
            dashboard: true,
            project_name: "run".to_string(),
        }
    }
}

pub struct DevServer {
    config: DevServerConfig,
    running: Arc<AtomicBool>,
    status_provider: Arc<dyn Fn() -> Vec<ComponentStatus> + Send + Sync>,
    last_reload: Arc<Mutex<Option<String>>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl DevServer {
    pub fn new(
        config: DevServerConfig,
        status_provider: Arc<dyn Fn() -> Vec<ComponentStatus> + Send + Sync>,
    ) -> Self {
        Self {
            config,
            running: Arc::new(AtomicBool::new(false)),
            status_provider,
            last_reload: Arc::new(Mutex::new(None)),
            thread: None,
        }
    }

    pub async fn start(&mut self) -> Result<()> {
        if self.thread.is_some() {
            return Ok(());
        }

        let addr = format!("{}:{}", self.config.host, self.config.port);
        let listener = TcpListener::bind(&addr).map_err(|err| {
            crate::v2::Error::other(format!("[devserver] failed to bind {}: {}", addr, err))
        })?;
        listener.set_nonblocking(true).map_err(|err| {
            crate::v2::Error::other(format!("[devserver] non-blocking failed: {}", err))
        })?;

        self.running.store(true, Ordering::SeqCst);
        let running = Arc::clone(&self.running);
        let config = self.config.clone();
        let status_provider = Arc::clone(&self.status_provider);
        let last_reload = Arc::clone(&self.last_reload);

        let handle = thread::spawn(move || {
            let listener = listener;
            while running.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _addr)) => {
                        let _ =
                            handle_request(&mut stream, &config, &status_provider, &last_reload);
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(50));
                    }
                    Err(err) => {
                        eprintln!("[devserver] accept error: {}", err);
                        thread::sleep(Duration::from_millis(50));
                    }
                }
            }
        });

        self.thread = Some(handle);

        Ok(())
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn url(&self) -> String {
        format!("http://{}:{}", self.config.host, self.config.port)
    }

    pub fn notify_reload(&self, component: &str) {
        let mut last_reload = self.last_reload.lock().unwrap();
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        *last_reload = Some(format!("{}@{}", component, timestamp));
    }

    pub fn notifier(&self) -> DevServerNotifier {
        DevServerNotifier {
            last_reload: Arc::clone(&self.last_reload),
        }
    }

    pub fn status(&self) -> DevServerStatus {
        DevServerStatus {
            running: self.running.load(Ordering::SeqCst),
            url: self.url(),
            websocket_enabled: self.config.websocket,
            dashboard_enabled: self.config.dashboard,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DevServerStatus {
    pub running: bool,
    pub url: String,
    pub websocket_enabled: bool,
    pub dashboard_enabled: bool,
}

#[derive(Clone)]
pub struct DevServerNotifier {
    last_reload: Arc<Mutex<Option<String>>>,
}

impl DevServerNotifier {
    pub fn notify_reload(&self, component: &str) {
        let mut last_reload = self.last_reload.lock().unwrap();
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        *last_reload = Some(format!("{}@{}", component, timestamp));
    }
}

#[allow(dead_code)]
pub fn dashboard_html(project_name: &str, components: &[ComponentStatus]) -> String {
    let mut html = String::new();

    html.push_str("<!DOCTYPE html>\n<html>\n<head>\n");
    html.push_str(&format!("<title>{} - Run Dev</title>\n", project_name));
    html.push_str("<style>\n");
    html.push_str("body { font-family: system-ui; margin: 0; padding: 20px; background: #1a1a2e; color: #eee; }\n");
    html.push_str("h1 { color: #00d4ff; }\n");
    html.push_str(
        ".component { background: #16213e; padding: 15px; margin: 10px 0; border-radius: 8px; }\n",
    );
    html.push_str(".running { border-left: 4px solid #00ff88; }\n");
    html.push_str(".stopped { border-left: 4px solid #ff4444; }\n");
    html.push_str(".name { font-weight: bold; font-size: 1.2em; }\n");
    html.push_str(".status { color: #888; }\n");
    html.push_str("</style>\n");
    html.push_str("</head>\n<body>\n");

    html.push_str(&format!("<h1> {}</h1>\n", project_name));
    html.push_str("<h2>Components</h2>\n");

    for comp in components {
        let class = if comp.running { "running" } else { "stopped" };
        html.push_str(&format!("<div class=\"component {}\">\n", class));
        html.push_str(&format!("<div class=\"name\">{}</div>\n", comp.name));
        html.push_str(&format!(
            "<div class=\"status\">Status: {}</div>\n",
            if comp.running { "Running" } else { "Stopped" }
        ));
        html.push_str(&format!(
            "<div class=\"status\">Calls: {}</div>\n",
            comp.call_count
        ));
        html.push_str("</div>\n");
    }

    html.push_str("</body>\n</html>");
    html
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
#[derive(Serialize)]
pub struct ComponentStatus {
    pub name: String,
    pub running: bool,
    pub call_count: u64,
    pub error_count: u64,
    pub uptime_ms: u64,
}

#[derive(Serialize)]
struct DevStatusResponse {
    project: String,
    running: bool,
    url: String,
    components: Vec<ComponentStatus>,
    last_reload: Option<String>,
}

fn handle_request(
    stream: &mut std::net::TcpStream,
    config: &DevServerConfig,
    status_provider: &Arc<dyn Fn() -> Vec<ComponentStatus> + Send + Sync>,
    last_reload: &Arc<Mutex<Option<String>>>,
) -> std::io::Result<()> {
    let mut buffer = [0u8; 4096];
    let read = stream.read(&mut buffer)?;
    if read == 0 {
        return Ok(());
    }
    let request = String::from_utf8_lossy(&buffer[..read]);
    let mut lines = request.lines();
    let first_line = lines.next().unwrap_or("");
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("/");

    if method != "GET" {
        return respond(
            stream,
            "405 Method Not Allowed",
            "text/plain",
            "Method not allowed",
        );
    }

    match path {
        "/health" => respond(stream, "200 OK", "text/plain", "ok"),
        "/status" => {
            let components = status_provider();
            let status = DevStatusResponse {
                project: config.project_name.clone(),
                running: true,
                url: format!("http://{}:{}", config.host, config.port),
                components,
                last_reload: last_reload.lock().unwrap().clone(),
            };
            let body = serde_json::to_string(&status).unwrap_or_else(|_| "{}".to_string());
            respond(stream, "200 OK", "application/json", &body)
        }
        "/" | "/index.html" => {
            let components = status_provider();
            let body = dashboard_html(&config.project_name, &components);
            respond(stream, "200 OK", "text/html; charset=utf-8", &body)
        }
        _ => respond(stream, "404 Not Found", "text/plain", "Not found"),
    }
}

fn respond(
    stream: &mut std::net::TcpStream,
    status: &str,
    content_type: &str,
    body: &str,
) -> std::io::Result<()> {
    let response = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status,
        content_type,
        body.len(),
        body
    );
    stream.write_all(response.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_dev_server_config() {
        let config = DevServerConfig::default();
        assert_eq!(config.port, 3000);
        assert_eq!(config.host, "127.0.0.1");
    }

    #[test]
    fn test_dev_server_url() {
        let server = DevServer::new(DevServerConfig::default(), Arc::new(|| Vec::new()));
        assert_eq!(server.url(), "http://127.0.0.1:3000");
    }
}
