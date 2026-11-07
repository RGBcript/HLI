//! Límite de reinicios automáticos por instancia y regresiones del supervisor.
//!
//! Políticas en milisegundos y `HLI_HOME` independiente por prueba. El fixture
//! se empaqueta e instala por la ruta real (`module.install`).

mod common;

use std::time::{Duration, Instant};

use common::*;
use serde_json::{json, Value};

const INSTANCE: &str = "limit-main";
const OTHER: &str = "limit-other";

/// Falla health indefinidamente, pero deja pasar el health del lifecycle para
/// que el proceso llegue a `ready` antes de empezar a fallar.
fn always_failing() -> Value {
    json!({
        "mode": "health_fail_n",
        "health_failures": 1_000_000,
        "skip_first_health": true,
        "trace": true
    })
}

fn fast(backoff_ms: &'static str, window_ms: &'static str) -> Vec<(&'static str, &'static str)> {
    vec![
        ("HLI_MCH_HEALTH_INTERVAL_MS", "20"),
        ("HLI_MCH_HEALTH_TIMEOUT_MS", "500"),
        ("HLI_MCH_DEGRADED_AFTER", "1"),
        ("HLI_MCH_RESTART_AFTER", "1"),
        ("HLI_MCH_MAX_RESTARTS", "3"),
        ("HLI_MCH_BACKOFF_MS", backoff_ms),
        ("HLI_MCH_RESTART_WINDOW_MS", window_ms),
    ]
}

fn pid(st: &Value) -> i64 {
    st["process"]["pid"].as_i64().expect("pid del proceso")
}

fn window_count(st: &Value) -> u64 {
    st["process"]["restart_count_in_window"]
        .as_u64()
        .unwrap_or(u64::MAX)
}

#[test]
fn mch_restart_limit_prevents_loop() {
    let env = fast("10", "1500");
    let mut c = prepare("limit-loop", &[(INSTANCE, false)], always_failing(), &env);
    c.start_env(&env);
    c.ok("mch.serve", json!({"name": INSTANCE}));

    let failed = c.wait_state(INSTANCE, state_is("failed"), Duration::from_secs(20));
    assert_eq!(
        window_count(&failed),
        3,
        "deben contarse exactamente 3 procesos de reemplazo"
    );
    let pids_after_failure = c.initialize_count();
    assert_eq!(pids_after_failure, 4, "proceso inicial + 3 reemplazos");

    // Aunque el health siga fallando, no debe nacer otro proceso.
    std::thread::sleep(Duration::from_millis(700));
    assert_eq!(
        c.initialize_count(),
        pids_after_failure,
        "el limitador debe impedir el bucle de reinicios"
    );
    let st = c.wait_state(INSTANCE, state_is("failed"), Duration::from_secs(5));
    assert!(
        window_count(&st) <= 3,
        "la ventana puede expirar, pero nunca debe superar el límite"
    );
}

#[test]
fn mch_fourth_restart_marks_failed() {
    let env = fast("10", "1500");
    let mut c = prepare("limit-fourth", &[(INSTANCE, false)], always_failing(), &env);
    c.start_env(&env);
    c.ok("mch.serve", json!({"name": INSTANCE}));

    let failed = c.wait_state(INSTANCE, state_is("failed"), Duration::from_secs(20));
    let process = &failed["process"];
    assert_eq!(process["state"].as_str(), Some("failed"));
    assert!(process["pid"].is_null(), "failed no debe conservar pid");
    assert_eq!(process["alive"].as_bool(), Some(false));
    assert!(
        process["next_restart_at"].is_null(),
        "failed no debe programar otro reinicio"
    );
    assert_eq!(
        process["last_error"]["code"].as_str(),
        Some("restart_limit_exceeded")
    );
    assert_eq!(
        process["generation"].as_u64(),
        Some(4),
        "generación tras el proceso inicial y 3 reemplazos"
    );
    assert_eq!(failed["instance"]["state"].as_str(), Some("failed"));
}

#[test]
fn mch_failed_instance_does_not_restart_automatically() {
    let env = fast("10", "1500");
    let mut c = prepare("limit-noauto", &[(INSTANCE, false)], always_failing(), &env);
    c.start_env(&env);
    c.ok("mch.serve", json!({"name": INSTANCE}));

    c.wait_state(INSTANCE, state_is("failed"), Duration::from_secs(20));
    let inits = c.initialize_count();
    std::thread::sleep(Duration::from_millis(700));

    assert_eq!(c.initialize_count(), inits, "failed no debe relanzarse");
    let st = c.ok("mch.status", json!({"name": INSTANCE}));
    assert_eq!(st["instance"]["state"].as_str(), Some("failed"));
    assert!(st["process"].is_null() || st["process"]["alive"].as_bool() == Some(false));
    c.ok("workspace.list", json!({}));
}

#[test]
fn mch_stable_window_resets_restart_count() {
    let env = fast("10", "500");
    let mut c = prepare("limit-window", &[(INSTANCE, false)], always_failing(), &env);
    c.start_env(&env);
    c.ok("mch.serve", json!({"name": INSTANCE}));

    // Dos reinicios automáticos dentro de la ventana.
    c.wait_state(
        INSTANCE,
        |st| {
            st["process"]["restart_count_in_window"]
                .as_u64()
                .unwrap_or(0)
                >= 2
        },
        Duration::from_secs(20),
    );
    // Dejar de fallar: la ventana debe expirar sin nuevos reinicios.
    c.fixture(json!({"trace": true}));
    let st = c.wait_state(
        INSTANCE,
        |st| {
            st["process"]["restart_count_in_window"].as_u64() == Some(0)
                && st["process"]["restart_window_started_at"].is_null()
                && st["process"]["next_restart_at"].is_null()
        },
        Duration::from_secs(10),
    );
    assert_eq!(
        st["process"]["restart_count_in_window"].as_u64(),
        Some(0),
        "ventana expirada debe poner el contador a 0"
    );

    // Una nueva necesidad de reinicio abre una ventana limpia.
    c.fixture(always_failing());
    let restarted = c.wait_state(
        INSTANCE,
        |st| {
            st["process"]["restart_count_in_window"]
                .as_u64()
                .unwrap_or(0)
                >= 1
        },
        Duration::from_secs(20),
    );
    assert!(restarted["process"]["restart_window_started_at"].is_string());
}

#[test]
fn mch_restart_limit_is_per_instance() {
    let env = fast("10", "1500");
    let mut c = prepare(
        "limit-per-inst",
        &[(INSTANCE, false), (OTHER, false)],
        json!({"trace": true}),
        &env,
    );
    c.start_env(&env);
    c.ok("mch.serve", json!({"name": INSTANCE}));
    c.ok("mch.serve", json!({"name": OTHER}));
    let other_pid = pid(&c.ok("mch.status", json!({"name": OTHER})));

    // Solo la instancia A falla; B mantiene health normal.
    c.fixture_for(INSTANCE, always_failing());
    let failed = c.wait_state(INSTANCE, state_is("failed"), Duration::from_secs(20));
    assert_eq!(failed["instance"]["state"].as_str(), Some("failed"));

    let other = c.ok("mch.status", json!({"name": OTHER}));
    assert_eq!(other["instance"]["state"].as_str(), Some("ready"));
    assert_eq!(other["process"]["alive"].as_bool(), Some(true));
    assert_eq!(pid(&other), other_pid, "B no debe reiniciarse");
    assert!(
        other["process"]["restart_count_in_window"]
            .as_u64()
            .unwrap_or(0)
            == 0,
        "el contador no debe ser global"
    );
    assert_eq!(c.ok("daemon.ping", json!({}))["pong"].as_bool(), Some(true));
    assert_eq!(c.invoke(OTHER)["result"]["pid"].as_i64(), Some(other_pid));
}

#[test]
fn mch_backoff_does_not_block_daemon() {
    let env = fast("600", "60000");
    let mut c = prepare(
        "limit-backoff",
        &[(INSTANCE, false)],
        always_failing(),
        &env,
    );
    c.start_env(&env);
    c.ok("mch.serve", json!({"name": INSTANCE}));

    let restarting = c.wait_state(
        INSTANCE,
        |st| st["process"]["state"].as_str() == Some("restarting"),
        Duration::from_secs(20),
    );
    assert!(
        restarting["process"]["next_restart_at"].is_string(),
        "durante el backoff debe anunciarse next_restart_at"
    );

    let t0 = Instant::now();
    c.ok("daemon.ping", json!({}));
    c.ok("mch.list", json!({}));
    c.ok("workspace.create", json!({"name": "during-backoff"}));
    let elapsed = t0.elapsed();
    assert!(
        elapsed < Duration::from_millis(300),
        "el backoff bloqueó al daemon ({elapsed:?})"
    );

    // El proceso de reemplazo termina naciendo tras el backoff.
    let st = c.wait_state(
        INSTANCE,
        |st| st["process"]["alive"].as_bool() == Some(true),
        Duration::from_secs(20),
    );
    assert!(pid(&st) > 0);
}

#[test]
fn explicit_restart_recovers_rate_limited_instance() {
    let env = fast("10", "1500");
    let mut c = prepare("limit-manual", &[(INSTANCE, false)], always_failing(), &env);
    c.start_env(&env);
    c.ok("mch.serve", json!({"name": INSTANCE}));

    let failed = c.wait_state(INSTANCE, state_is("failed"), Duration::from_secs(20));
    let generation = failed["process"]["generation"].as_u64().unwrap_or(0);

    // El módulo vuelve a estar sano y el operador reinicia a mano.
    c.fixture(json!({"trace": true}));
    let restarted = c.ok("mch.restart", json!({"name": INSTANCE}));
    assert!(restarted["pid"].as_i64().unwrap_or(0) > 0);

    let ready = c.wait_state(INSTANCE, is_ready, Duration::from_secs(15));
    assert!(
        ready["process"]["generation"].as_u64().unwrap_or(0) > generation,
        "el reinicio manual debe incrementar la generación"
    );
    assert_eq!(
        ready["process"]["restart_count_in_window"].as_u64(),
        Some(0),
        "el reinicio manual limpia el contador"
    );
    assert!(ready["process"]["restart_window_started_at"].is_null());
    assert!(ready["process"]["next_restart_at"].is_null());
    assert_eq!(
        c.invoke(INSTANCE)["result"]["pid"].as_i64(),
        Some(pid(&ready))
    );
}

#[test]
fn mch_stop_cancels_pending_backoff() {
    let env = fast("800", "60000");
    let mut c = prepare("limit-stop", &[(INSTANCE, false)], always_failing(), &env);
    c.start_env(&env);
    c.ok("mch.serve", json!({"name": INSTANCE}));

    c.wait_state(
        INSTANCE,
        |st| st["process"]["state"].as_str() == Some("restarting"),
        Duration::from_secs(20),
    );
    let inits = c.initialize_count();
    let stopped = c.ok("mch.stop", json!({"name": INSTANCE}));
    assert_eq!(stopped["state"].as_str(), Some("stopped"));

    std::thread::sleep(Duration::from_millis(1200));
    assert_eq!(
        c.initialize_count(),
        inits,
        "mch stop debe cancelar el reinicio pendiente"
    );
    let st = c.ok("mch.status", json!({"name": INSTANCE}));
    assert_eq!(st["instance"]["state"].as_str(), Some("stopped"));
    assert!(st["process"].is_null());
}

#[test]
fn mch_health_failure_marks_degraded() {
    let env = vec![
        ("HLI_MCH_HEALTH_INTERVAL_MS", "20"),
        ("HLI_MCH_HEALTH_TIMEOUT_MS", "500"),
        ("HLI_MCH_DEGRADED_AFTER", "2"),
        ("HLI_MCH_RESTART_AFTER", "1000"),
    ];
    let mut c = prepare("reg-degraded", &[(INSTANCE, false)], always_failing(), &env);
    c.start_env(&env);
    c.ok("mch.serve", json!({"name": INSTANCE}));

    let st = c.wait_state(
        INSTANCE,
        |st| st["instance"]["state"].as_str() == Some("degraded"),
        Duration::from_secs(10),
    );
    assert_eq!(st["process"]["alive"].as_bool(), Some(true));
    assert!(st["process"]["health_failures"].as_u64().unwrap_or(0) >= 2);
}

#[test]
fn mch_health_recovery_returns_ready() {
    let env = vec![
        ("HLI_MCH_HEALTH_INTERVAL_MS", "20"),
        ("HLI_MCH_HEALTH_TIMEOUT_MS", "500"),
        ("HLI_MCH_DEGRADED_AFTER", "2"),
        ("HLI_MCH_RESTART_AFTER", "1000"),
    ];
    let mut c = prepare(
        "reg-recovery",
        &[(INSTANCE, false)],
        json!({"mode": "recover_after_n", "recover_after_n": 3, "skip_first_health": true}),
        &env,
    );
    c.start_env(&env);
    c.ok("mch.serve", json!({"name": INSTANCE}));

    c.wait_state(
        INSTANCE,
        |st| st["instance"]["state"].as_str() == Some("degraded"),
        Duration::from_secs(10),
    );
    let ready = c.wait_state(INSTANCE, is_ready, Duration::from_secs(10));
    assert_eq!(ready["process"]["health_failures"].as_u64(), Some(0));
}

#[test]
fn mch_restarts_after_third_health_failure() {
    let env = vec![
        ("HLI_MCH_HEALTH_INTERVAL_MS", "20"),
        ("HLI_MCH_HEALTH_TIMEOUT_MS", "500"),
        ("HLI_MCH_DEGRADED_AFTER", "3"),
        ("HLI_MCH_RESTART_AFTER", "3"),
        ("HLI_MCH_MAX_RESTARTS", "3"),
        ("HLI_MCH_BACKOFF_MS", "10"),
        ("HLI_MCH_RESTART_WINDOW_MS", "60000"),
    ];
    let mut c = prepare("reg-third", &[(INSTANCE, false)], always_failing(), &env);
    c.start_env(&env);
    let first = c.ok("mch.serve", json!({"name": INSTANCE}));
    let pid1 = first["pid"].as_i64().expect("pid inicial");

    let st = c.wait_state(
        INSTANCE,
        |st| st["process"]["pid"].as_i64().is_some_and(|p| p != pid1),
        Duration::from_secs(20),
    );
    assert_ne!(pid(&st), pid1);
    assert_eq!(st["process"]["generation"].as_u64(), Some(2));
}

#[test]
fn mch_restart_uses_new_pid() {
    let mut c = prepare("reg-newpid", &[(INSTANCE, false)], json!({}), &[]);
    c.start();
    let first = c.ok("mch.serve", json!({"name": INSTANCE}));
    let pid1 = first["pid"].as_i64().expect("pid inicial");
    let second = c.ok("mch.restart", json!({"name": INSTANCE}));
    let pid2 = second["pid"].as_i64().expect("pid nuevo");
    assert_ne!(pid1, pid2);
    let st = c.wait_state(INSTANCE, is_ready, Duration::from_secs(10));
    assert_eq!(pid(&st), pid2);
}

#[test]
fn mch_restart_increments_generation() {
    let mut c = prepare("reg-gen", &[(INSTANCE, false)], json!({}), &[]);
    c.start();
    c.ok("mch.serve", json!({"name": INSTANCE}));
    let before = c.ok("mch.status", json!({"name": INSTANCE}));
    let gen1 = before["process"]["generation"]
        .as_u64()
        .expect("generación inicial");
    c.ok("mch.restart", json!({"name": INSTANCE}));
    let st = c.wait_state(INSTANCE, is_ready, Duration::from_secs(10));
    assert_eq!(st["process"]["generation"].as_u64(), Some(gen1 + 1));
}

#[test]
fn stale_health_result_is_ignored() {
    let env = vec![
        ("HLI_MCH_HEALTH_INTERVAL_MS", "20"),
        ("HLI_MCH_HEALTH_TIMEOUT_MS", "50"),
        ("HLI_MCH_DEGRADED_AFTER", "2"),
        ("HLI_MCH_RESTART_AFTER", "3"),
        ("HLI_MCH_MAX_RESTARTS", "3"),
        ("HLI_MCH_BACKOFF_MS", "10"),
        ("HLI_MCH_RESTART_WINDOW_MS", "60000"),
    ];
    let mut c = prepare(
        "reg-stale",
        &[(INSTANCE, false)],
        json!({"mode": "health_timeout", "health_delay_ms": 300, "trace": true}),
        &env,
    );
    c.start_env(&env);
    c.ok("mch.serve", json!({"name": INSTANCE}));

    let restarted = c.wait_state(
        INSTANCE,
        |st| st["process"]["generation"].as_u64().unwrap_or(0) >= 2,
        Duration::from_secs(20),
    );
    assert!(pid(&restarted) > 0);

    // El proceso viejo puede responder health tarde; su resultado pertenece a
    // la generación anterior y no debe contaminar a la nueva.
    c.fixture(json!({"trace": true}));
    let ready = c.wait_state(
        INSTANCE,
        |st| is_ready(st) && st["process"]["health_failures"].as_u64() == Some(0),
        Duration::from_secs(15),
    );
    std::thread::sleep(Duration::from_millis(400));
    let st = c.ok("mch.status", json!({"name": INSTANCE}));
    assert_eq!(st["instance"]["state"].as_str(), Some("ready"));
    assert_eq!(st["process"]["health_failures"].as_u64(), Some(0));
    assert_eq!(
        st["process"]["pid"].as_i64(),
        ready["process"]["pid"].as_i64()
    );
}
