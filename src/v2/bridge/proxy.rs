//! Bridge Proxy
//!
//! Provides transparent networking between WASI components and Docker services.

use crate::v2::{Error, Result};
use std::collections::HashMap;
use std::net::{TcpListener, TcpStream};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    pub listen_addr: String,

    pub target_addr: String,

    pub protocol: ProxyProtocol,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyProtocol {
    Tcp,
    Http,
    Grpc,
}
pub struct BridgeProxy {
    routes: HashMap<String, ProxyRoute>,

    running: bool,

    shutdown: Arc<AtomicBool>,

    handles: Vec<thread::JoinHandle<()>>,
}
#[derive(Debug, Clone)]
pub struct ProxyRoute {
    pub name: String,

    pub source: String,

    pub target: String,

    pub port: u16,

    pub protocol: ProxyProtocol,
}

impl BridgeProxy {
    pub fn new() -> Self {
        Self {
            routes: HashMap::new(),
            running: false,
            shutdown: Arc::new(AtomicBool::new(false)),
            handles: Vec::new(),
        }
    }
    pub fn add_route(&mut self, route: ProxyRoute) {
        self.routes.insert(route.name.clone(), route);
    }
    pub fn remove_route(&mut self, name: &str) {
        self.routes.remove(name);
    }
    pub fn start(&mut self) -> Result<()> {
        if self.running {
            return Ok(());
        }

        self.shutdown.store(false, Ordering::SeqCst);
        self.handles.clear();

        for route in self.routes.values() {
            let listen_addr = format!("127.0.0.1:{}", route.port);
            let target_addr = format!("{}:{}", route.target, route.port);
            let shutdown = Arc::clone(&self.shutdown);

            let handle = thread::spawn(move || {
                let listener = match TcpListener::bind(&listen_addr) {
                    Ok(l) => l,
                    Err(_) => return,
                };
                let _ = listener.set_nonblocking(true);

                while !shutdown.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let target = target_addr.clone();
                            thread::spawn(move || {
                                let _ = proxy_stream(stream, &target);
                            });
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(50));
                        }
                        Err(_) => break,
                    }
                }
            });
            self.handles.push(handle);
        }

        self.running = true;
        Ok(())
    }
    pub fn stop(&mut self) {
        self.running = false;
        self.shutdown.store(true, Ordering::SeqCst);
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
    }
    pub fn get_route(&self, name: &str) -> Option<&ProxyRoute> {
        self.routes.get(name)
    }
    pub fn list_routes(&self) -> Vec<&ProxyRoute> {
        self.routes.values().collect()
    }
    pub fn create_component_to_docker_route(
        component: &str,
        service: &str,
        port: u16,
    ) -> ProxyRoute {
        ProxyRoute {
            name: format!("{}-to-{}", component, service),
            source: component.to_string(),
            target: service.to_string(),
            port,
            protocol: ProxyProtocol::Tcp,
        }
    }
}

fn proxy_stream(mut inbound: TcpStream, target_addr: &str) -> Result<()> {
    let mut outbound = TcpStream::connect(target_addr)
        .map_err(|e| Error::other(format!("Proxy connect failed: {}", e)))?;

    let mut inbound_clone = inbound
        .try_clone()
        .map_err(|e| Error::other(format!("Proxy clone failed: {}", e)))?;
    let mut outbound_clone = outbound
        .try_clone()
        .map_err(|e| Error::other(format!("Proxy clone failed: {}", e)))?;

    let t1 = thread::spawn(move || {
        let _ = std::io::copy(&mut inbound_clone, &mut outbound);
    });
    let t2 = thread::spawn(move || {
        let _ = std::io::copy(&mut outbound_clone, &mut inbound);
    });

    let _ = t1.join();
    let _ = t2.join();
    Ok(())
}

impl Default for BridgeProxy {
    fn default() -> Self {
        Self::new()
    }
}
#[allow(dead_code)]
pub struct HttpProxy {
    routes: HashMap<String, String>,
}

#[allow(dead_code)]
impl HttpProxy {
    pub fn new() -> Self {
        Self {
            routes: HashMap::new(),
        }
    }
    pub fn route(&mut self, prefix: &str, target: &str) {
        self.routes.insert(prefix.to_string(), target.to_string());
    }
    pub fn resolve(&self, path: &str) -> Option<&str> {
        for (prefix, target) in &self.routes {
            if path.starts_with(prefix) {
                return Some(target);
            }
        }
        None
    }
}

impl Default for HttpProxy {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proxy_route() {
        let route = BridgeProxy::create_component_to_docker_route("api", "postgres", 5432);
        assert_eq!(route.name, "api-to-postgres");
        assert_eq!(route.port, 5432);
    }

    #[test]
    fn test_http_proxy() {
        let mut proxy = HttpProxy::new();
        proxy.route("/api", "http://backend:8080");
        proxy.route("/db", "http://postgres:5432");

        assert_eq!(proxy.resolve("/api/users"), Some("http://backend:8080"));
        assert_eq!(proxy.resolve("/db/query"), Some("http://postgres:5432"));
        assert_eq!(proxy.resolve("/other"), None);
    }
}
