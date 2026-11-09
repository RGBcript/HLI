//! Protocolo del daemon con un servidor TCP falso.
//!
//! Prueba lo que el daemon real no provoca a propósito: respuestas gigantes,
//! JSON inválido, errores, lentitud y concurrencia.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use hli_tui::client::call_at;
use serde_json::json;

/// Levanta un servidor que atiende `connections` conexiones con `handler`.
fn spawn_server<F>(connections: usize, handler: F) -> u16
where
    F: Fn(&str) -> (Duration, Vec<u8>) + Send + Sync + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let handler = Arc::new(handler);
    thread::spawn(move || {
        for _ in 0..connections {
            let Ok((stream, _)) = listener.accept() else {
                return;
            };
            let handler = Arc::clone(&handler);
            thread::spawn(move || serve_one(stream, &*handler));
        }
    });
    port
}

fn serve_one<F>(mut stream: TcpStream, handler: &F)
where
    F: Fn(&str) -> (Duration, Vec<u8>),
{
    let mut line = String::new();
    let _ = BufReader::new(stream.try_clone().unwrap()).read_line(&mut line);
    let (delay, body) = handler(line.trim());
    if !delay.is_zero() {
        thread::sleep(delay);
    }
    // El cliente puede cerrar antes: los errores de escritura no importan.
    let _ = stream.write_all(&body);
    let _ = stream.flush();
}

const OK: &str = "{\"ok\":true,\"data\":{\"v\":1}}\n";

#[test]
fn oversized_response_is_rejected_without_oom() {
    let port = spawn_server(1, |_| {
        let mut body = vec![b'x'; 17 * 1024 * 1024];
        body.push(b'\n');
        (Duration::ZERO, body)
    });
    let err = call_at(port, "big", json!({}), Duration::from_secs(5)).unwrap_err();
    assert!(err.contains("demasiado grande"), "{err}");
}

#[test]
fn invalid_json_is_an_error() {
    let port = spawn_server(1, |_| (Duration::ZERO, b"no soy json\n".to_vec()));
    assert!(call_at(port, "x", json!({}), Duration::from_secs(2)).is_err());
}

#[test]
fn valid_json_without_data_is_null_not_panic() {
    let port = spawn_server(1, |_| (Duration::ZERO, b"{\"ok\":true}\n".to_vec()));
    let v = call_at(port, "x", json!({}), Duration::from_secs(2)).unwrap();
    assert!(v.is_null());
}

#[test]
fn daemon_error_message_is_propagated() {
    let port = spawn_server(1, |_| {
        (
            Duration::ZERO,
            b"{\"ok\":false,\"error\":{\"code\":\"x\",\"message\":\"boom\"}}\n".to_vec(),
        )
    });
    let err = call_at(port, "x", json!({}), Duration::from_secs(2)).unwrap_err();
    assert_eq!(err, "boom");
}

#[test]
fn slow_daemon_hits_the_timeout() {
    let port = spawn_server(1, |_| (Duration::from_millis(800), OK.as_bytes().to_vec()));
    let start = std::time::Instant::now();
    let result = call_at(port, "slow", json!({}), Duration::from_millis(150));
    assert!(result.is_err(), "debió expirar");
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "no respetó el techo"
    );
}

#[test]
fn closed_port_fails_fast() {
    // Puerto reservado y cerrado: connect debe fallar, no colgarse.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let start = std::time::Instant::now();
    assert!(call_at(port, "x", json!({}), Duration::from_secs(2)).is_err());
    assert!(start.elapsed() < Duration::from_secs(1));
}

#[test]
fn concurrent_calls_get_their_own_reply() {
    let port = spawn_server(2, |line| {
        let cmd = serde_json::from_str::<serde_json::Value>(line)
            .ok()
            .and_then(|v| v["cmd"].as_str().map(str::to_string))
            .unwrap_or_default();
        let body = format!("{{\"ok\":true,\"data\":{{\"cmd\":\"{cmd}\"}}}}\n");
        (Duration::from_millis(50), body.into_bytes())
    });
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Vec::new();
    for name in ["uno", "dos"] {
        let seen = Arc::clone(&seen);
        handles.push(thread::spawn(move || {
            let v = call_at(port, name, json!({}), Duration::from_secs(2)).unwrap();
            seen.lock()
                .unwrap()
                .push(v["cmd"].as_str().unwrap().to_string());
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }
    let mut got = seen.lock().unwrap().clone();
    got.sort();
    assert_eq!(got, vec!["dos", "uno"]);
}
