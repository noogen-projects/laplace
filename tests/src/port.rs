use std::collections::HashSet;
use std::net::{TcpListener, TcpStream};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use lazy_static::lazy_static;

lazy_static! {
    static ref BUSY_PORTS: Mutex<HashSet<u16>> = Mutex::new(HashSet::new());
}

/// Use a free port, provided by system, because 0 is passed
/// to the [`TcpListener::local_addr`] method.
pub fn next_free_local_port() -> u16 {
    let mut busy_ports = BUSY_PORTS.lock().expect("Cannot lock busy ports collection");
    loop {
        if let Some(port) = TcpListener::bind(("127.0.0.1", 0))
            .and_then(|listener| listener.local_addr())
            .ok()
            .map(|address| address.port())
            .filter(|port| !busy_ports.contains(port))
        {
            busy_ports.insert(port);
            break port;
        }
    }
}

#[derive(Debug)]
pub struct PortOpeningError;

pub fn wait_for_port_opened(host: &str, port: u16, timeout: Duration) -> Result<(), PortOpeningError> {
    // wait for TCP port
    const PORT_CHECK_INTERVAL: Duration = Duration::from_millis(10);
    let checks_num = timeout.as_millis() / PORT_CHECK_INTERVAL.as_millis();
    for _ in 0..checks_num {
        if TcpStream::connect(format!("{}:{}", host, port)).is_ok() {
            return Ok(());
        }
        thread::sleep(PORT_CHECK_INTERVAL);
    }
    Err(PortOpeningError)
}

#[derive(Debug)]
pub struct PortClosingError;

pub fn wait_for_port_closed(host: &str, port: u16) -> Result<(), PortClosingError> {
    // wait for TCP port to close
    for _ in 0..100 {
        if TcpStream::connect(format!("{}:{}", host, port)).is_err() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(10));
    }
    Err(PortClosingError)
}
