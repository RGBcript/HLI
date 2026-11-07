//! Tiempo UTC en ISO-8601 con milisegundos, sin dependencias externas.

use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_iso() -> String {
    let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    iso_from_unix(d.as_secs() as i64, d.subsec_millis())
}

pub fn iso_from_unix(secs: i64, millis: u32) -> String {
    let days = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    let h = sod / 3600;
    let mi = (sod % 3600) / 60;
    let s = sod % 60;
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}.{millis:03}Z")
}

/// Algoritmo de Howard Hinnant: días desde 1970-01-01 a fecha civil.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch() {
        assert_eq!(iso_from_unix(0, 0), "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn known_date() {
        // 2026-11-07T00:00:00Z
        assert_eq!(iso_from_unix(1_794_009_600, 0), "2026-11-07T00:00:00.000Z");
    }
}
