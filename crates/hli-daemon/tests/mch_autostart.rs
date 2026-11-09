//! Autostart MCH: recuperación tras reinicio del daemon, aislamiento de fallos,
//! lifecycle completo antes de `ready` y exclusión mutua entre autostart y
//! `mch serve`.
//!
//! Cada prueba usa su propio `HLI_HOME`, políticas de milisegundos y un módulo
//! fixture empaquetado al vuelo, sin estado compartido entre pruebas.

mod common;

use std::time::{Duration, Instant};

use common::*;
use serde_json::{Value, json};

const INSTANCE: &str = "autostart-main";

fn prepare_case(tag: &str, autostart: bool, fixture: Value) -> Ctx {
    prepare(tag, &[(INSTANCE, autostart)], fixture, &[])
}

#[test]
#[cfg_attr(not(windows), ignore = "MCH autostart: port a Linux pendiente")]
fn mch_autostart_recovers_after_daemon_restart() {
    let mut c = prepare_case("recovers", true, json!({"trace": true}));

    c.start();
    let first = c.wait_state(INSTANCE, is_ready, Duration::from_secs(15));
    let pid1 = first["process"]["pid"]
        .as_i64()
        .expect("pid tras autostart");
    assert_eq!(c.invoke(INSTANCE)["result"]["pid"].as_i64(), Some(pid1));

    // Reinicio del daemon: la instancia debe volver sola a ready con un proceso
    // nuevo, sin `mch serve` manual.
    c.stop();
    c.start();
    let second = c.wait_state(INSTANCE, is_ready, Duration::from_secs(15));
    let pid2 = second["process"]["pid"]
        .as_i64()
        .expect("pid tras reinicio");
    assert_ne!(pid2, pid1, "el autostart debe lanzar un proceso nuevo");
    assert_eq!(
        c.invoke(INSTANCE)["result"]["pid"].as_i64(),
        Some(pid2),
        "el proceso recuperado debe atender invocaciones"
    );
    assert_eq!(
        c.ok("mch.inspect", json!({"name": INSTANCE}))["autostart"].as_bool(),
        Some(true),
        "autostart debe persistir en la instancia"
    );
}

#[test]
#[cfg_attr(not(windows), ignore = "MCH autostart: port a Linux pendiente")]
fn mch_non_autostart_remains_stopped() {
    let mut c = prepare_case("nonauto", false, json!({"trace": true}));

    c.start();
    let served = c.ok("mch.serve", json!({"name": INSTANCE}));
    assert!(served["pid"].as_i64().unwrap_or(0) > 0);
    c.stop();

    let before = c.initialize_count();

    c.start();
    std::thread::sleep(Duration::from_millis(600));

    let st = c.ok("mch.status", json!({"name": INSTANCE}));
    assert_eq!(st["instance"]["state"].as_str(), Some("stopped"));
    assert!(
        st["instance"]["pid"].is_null(),
        "el PID previo debe limpiarse"
    );
    assert!(st["process"].is_null(), "no debe haber proceso supervisado");
    let err = c
        .call(
            "mch.invoke",
            json!({"name": INSTANCE, "operation": "memory.search", "arguments": {}}),
        )
        .unwrap_err();
    assert!(
        err.contains("no está ready") || err.contains("no iniciado"),
        "error inesperado: {err}"
    );
    assert_eq!(
        c.initialize_count(),
        before,
        "una instancia sin autostart no debe relanzarse al arrancar el daemon"
    );
}

#[test]
fn failing_autostart_does_not_block_daemon() {
    let mut c = prepare_case(
        "failing",
        true,
        json!({"mode": "initialize_failure", "initialize_delay_ms": 3000}),
    );

    let started = Instant::now();
    c.start();
    let ping = started.elapsed();
    assert!(
        ping < Duration::from_millis(2200),
        "el daemon tardó {ping:?} en responder: el autostart bloqueó el arranque"
    );

    // Con el lifecycle todavía en curso, el daemon ya atiende comandos.
    let list = c.ok("mch.list", json!({}));
    assert_eq!(list.as_array().map(Vec::len), Some(1));

    let failed = c.wait_state(INSTANCE, state_is("failed"), Duration::from_secs(20));
    assert!(
        failed["process"].is_null(),
        "un autostart fallido no debe dejar proceso supervisado"
    );

    // Y sigue operativo después del fallo.
    c.ok("workspace.list", json!({}));
    c.ok("mch.list", json!({}));
}

#[test]
#[cfg_attr(not(windows), ignore = "MCH autostart: port a Linux pendiente")]
fn autostart_reaches_ready_only_after_lifecycle() {
    let mut c = prepare_case(
        "lifecycle",
        true,
        json!({"trace": true, "configure_delay_ms": 800}),
    );

    c.start();
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut saw_starting = false;
    let (at_ready, trace_at_ready) = loop {
        let st = c.ok("mch.status", json!({"name": INSTANCE}));
        match st["instance"]["state"].as_str().unwrap_or("") {
            "starting" => saw_starting = true,
            "ready" => break (st, c.trace()),
            _ => {}
        }
        if Instant::now() >= deadline {
            panic!("sin ready; último: {st}\n{}", c.log_tail());
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(saw_starting, "debe observarse `starting` antes de `ready`");

    let pid = at_ready["process"]["pid"].as_i64().expect("pid");
    let methods: Vec<String> = trace_at_ready
        .iter()
        .filter(|l| l["pid"].as_i64() == Some(pid))
        .filter_map(|l| l["method"].as_str().map(str::to_string))
        .collect();
    let pos = |m: &str| {
        methods
            .iter()
            .position(|x| x == m)
            .unwrap_or_else(|| panic!("falta {m} antes de ready: {methods:?}"))
    };
    assert!(
        pos("initialize") < pos("capabilities")
            && pos("capabilities") < pos("configure")
            && pos("configure") < pos("health"),
        "orden de lifecycle incorrecto: {methods:?}"
    );
    assert_eq!(
        methods.iter().filter(|m| *m == "initialize").count(),
        1,
        "un solo initialize por proceso: {methods:?}"
    );

    let inv = c.invoke(INSTANCE);
    assert_eq!(inv["result"]["pid"].as_i64(), Some(pid));
    assert!(inv["result"]["health_count"].as_u64().unwrap_or(0) >= 1);
}

#[test]
#[cfg_attr(not(windows), ignore = "MCH autostart: port a Linux pendiente")]
fn autostart_does_not_reuse_stale_pid() {
    let mut c = prepare_case("stale", true, json!({}));

    c.start();
    let first = c.wait_state(INSTANCE, is_ready, Duration::from_secs(15));
    let stale = first["process"]["pid"].as_i64().expect("pid inicial");

    // Caída abrupta: la DB conserva state=ready y el PID viejo.
    c.kill();
    kill_pid(stale);
    assert!(!pid_alive(stale), "el proceso viejo debe estar muerto");

    c.start();
    let st = c.wait_state(INSTANCE, is_ready, Duration::from_secs(15));
    let pid = st["process"]["pid"].as_i64().expect("pid nuevo");
    assert_ne!(pid, stale, "no debe reutilizarse el PID persistido");
    assert!(pid_alive(pid), "el proceso autostart debe estar vivo");
    assert_eq!(
        c.invoke(INSTANCE)["result"]["pid"].as_i64(),
        Some(pid),
        "las invocaciones deben ir al proceso nuevo"
    );
}

#[test]
#[cfg_attr(not(windows), ignore = "MCH autostart: port a Linux pendiente")]
fn concurrent_serve_and_autostart_spawn_once() {
    let mut c = prepare_case(
        "concurrent",
        true,
        json!({"trace": true, "configure_delay_ms": 600}),
    );

    c.start();
    let mut threads = Vec::new();
    for _ in 0..4 {
        let home = c.home().to_path_buf();
        threads.push(std::thread::spawn(move || {
            call_home(&home, "mch.serve", json!({"name": INSTANCE}))
        }));
    }
    let mut served = Vec::new();
    for t in threads {
        match t.join().expect("hilo de mch serve") {
            Ok(v) => served.push(v["pid"].as_i64().expect("pid de mch serve")),
            Err(e) => panic!("mch serve concurrente falló: {e}"),
        }
    }

    let st = c.wait_state(INSTANCE, is_ready, Duration::from_secs(20));
    let pid = st["process"]["pid"].as_i64().expect("pid supervisado");
    assert!(
        served.iter().all(|p| *p == pid),
        "mch serve y autostart devolvieron procesos distintos: {served:?} vs {pid}"
    );

    let inits: Vec<i64> = c
        .trace()
        .iter()
        .filter(|l| l["method"] == "initialize")
        .filter_map(|l| l["pid"].as_i64())
        .collect();
    assert_eq!(
        inits.len(),
        1,
        "el guard por instancia debe impedir procesos duplicados: {inits:?}"
    );
    assert_eq!(inits[0], pid);
    assert_eq!(c.invoke(INSTANCE)["result"]["pid"].as_i64(), Some(pid));
}
