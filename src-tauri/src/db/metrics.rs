use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// A daily metrics summary row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyMetrics {
    pub id: i64,
    pub date_local: String,
    pub dictation_count: i64,
    pub total_words: i64,
    pub total_duration_ms: i64,
    pub total_wpm_weighted_sum: f64,
}

/// Per-engine daily breakdown row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyEngineMetrics {
    pub id: i64,
    pub date_local: String,
    pub engine_id: String,
    pub dictation_count: i64,
    pub total_words: i64,
    pub total_duration_ms: i64,
}

/// Lifetime aggregate metrics (singleton row, id = 1).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LifetimeMetrics {
    pub total_words: i64,
    pub total_dictations: i64,
    pub total_wpm_weighted_sum: f64,
    pub first_use_date: Option<String>,
    pub current_streak_days: i64,
    pub longest_streak_days: i64,
    pub typing_baseline_wpm: Option<f64>,
}

/// Record a dictation event in the daily and lifetime metrics tables.
///
/// - `date_local`: ISO 8601 date string (e.g. "2026-02-25") in the user's local timezone.
/// - `word_count`: number of words in the transcription.
/// - `duration_ms`: audio duration in milliseconds.
/// - `wpm`: words per minute for this dictation.
/// - `engine_id`: engine identifier (e.g. "whisper-tiny", "groq").
pub fn record_dictation(
    conn: &Connection,
    date_local: &str,
    word_count: i64,
    duration_ms: i64,
    wpm: f64,
    engine_id: &str,
) -> Result<(), String> {
    // Upsert daily metrics
    conn.execute(
        "INSERT INTO metrics_daily (date_local, dictation_count, total_words, total_duration_ms, total_wpm_weighted_sum)
         VALUES (?1, 1, ?2, ?3, ?4)
         ON CONFLICT(date_local) DO UPDATE SET
           dictation_count = dictation_count + 1,
           total_words = total_words + ?2,
           total_duration_ms = total_duration_ms + ?3,
           total_wpm_weighted_sum = total_wpm_weighted_sum + ?4",
        rusqlite::params![date_local, word_count, duration_ms, word_count as f64 * wpm],
    )
    .map_err(|e| format!("Failed to upsert daily metrics: {e}"))?;

    // Upsert per-engine daily breakdown
    conn.execute(
        "INSERT INTO metrics_daily_engine (date_local, engine_id, dictation_count, total_words, total_duration_ms)
         VALUES (?1, ?2, 1, ?3, ?4)
         ON CONFLICT(date_local, engine_id) DO UPDATE SET
           dictation_count = dictation_count + 1,
           total_words = total_words + ?3,
           total_duration_ms = total_duration_ms + ?4",
        rusqlite::params![date_local, engine_id, word_count, duration_ms],
    )
    .map_err(|e| format!("Failed to upsert engine metrics: {e}"))?;

    // Update lifetime metrics (singleton row id = 1)
    let wpm_weighted = word_count as f64 * wpm;
    conn.execute(
        "INSERT INTO lifetime_metrics (id, total_words, total_dictations, total_wpm_weighted_sum, first_use_date, current_streak_days, longest_streak_days)
         VALUES (1, ?1, 1, ?3, ?2, 1, 1)
         ON CONFLICT(id) DO UPDATE SET
           total_words = total_words + ?1,
           total_dictations = total_dictations + 1,
           total_wpm_weighted_sum = total_wpm_weighted_sum + ?3,
           first_use_date = COALESCE(first_use_date, ?2)",
        rusqlite::params![word_count, date_local, wpm_weighted],
    )
    .map_err(|e| format!("Failed to upsert lifetime metrics: {e}"))?;

    // Update streak
    update_streak(conn, date_local)?;

    Ok(())
}

/// Update the streak in lifetime_metrics based on today's local date.
fn update_streak(conn: &Connection, today: &str) -> Result<(), String> {
    // Check if we already counted today for the streak
    let last_streak_date: Option<String> = conn
        .query_row(
            "SELECT last_streak_date FROM lifetime_metrics WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .ok()
        .flatten();

    if last_streak_date.as_deref() == Some(today) {
        // Already counted today — nothing to do
        return Ok(());
    }

    // Get the most recent date with dictation before today
    let prev_date: Option<String> = conn
        .query_row(
            "SELECT date_local FROM metrics_daily WHERE date_local < ?1 ORDER BY date_local DESC LIMIT 1",
            [today],
            |row| row.get(0),
        )
        .ok();

    let is_consecutive = prev_date
        .as_deref()
        .map(|prev| is_next_day(prev, today))
        .unwrap_or(false);

    if is_consecutive {
        // Extend streak
        conn.execute(
            "UPDATE lifetime_metrics SET
               current_streak_days = current_streak_days + 1,
               longest_streak_days = MAX(longest_streak_days, current_streak_days + 1),
               last_streak_date = ?1
             WHERE id = 1",
            [today],
        )
        .map_err(|e| format!("Failed to extend streak: {e}"))?;
    } else if prev_date.is_some() {
        // Gap > 1 day: reset streak to 1
        conn.execute(
            "UPDATE lifetime_metrics SET current_streak_days = 1, last_streak_date = ?1 WHERE id = 1",
            [today],
        )
        .map_err(|e| format!("Failed to reset streak: {e}"))?;
    } else {
        // First ever day — mark last_streak_date
        conn.execute(
            "UPDATE lifetime_metrics SET last_streak_date = ?1 WHERE id = 1",
            [today],
        )
        .map_err(|e| format!("Failed to set last streak date: {e}"))?;
    }

    Ok(())
}

/// Check if `next` is exactly one calendar day after `prev`.
/// Both should be "YYYY-MM-DD" format.
fn is_next_day(prev: &str, next: &str) -> bool {
    // Parse simple date format YYYY-MM-DD
    let parse = |s: &str| -> Option<(i32, u32, u32)> {
        let parts: Vec<&str> = s.split('-').collect();
        if parts.len() != 3 {
            return None;
        }
        let y: i32 = parts[0].parse().ok()?;
        let m: u32 = parts[1].parse().ok()?;
        let d: u32 = parts[2].parse().ok()?;
        Some((y, m, d))
    };

    let Some((py, pm, pd)) = parse(prev) else {
        return false;
    };
    let Some((ny, nm, nd)) = parse(next) else {
        return false;
    };

    // Check if next == prev + 1 day using simple calendar arithmetic
    let days_in_month = |y: i32, m: u32| -> u32 {
        match m {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => {
                if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
                    29
                } else {
                    28
                }
            }
            _ => 30,
        }
    };

    // Advance prev by 1 day
    let (ey, em, ed) = if pd < days_in_month(py, pm) {
        (py, pm, pd + 1)
    } else if pm < 12 {
        (py, pm + 1, 1)
    } else {
        (py + 1, 1, 1)
    };

    ey == ny && em == nm && ed == nd
}

/// Get daily metrics for a specific date.
pub fn get_daily(conn: &Connection, date_local: &str) -> Result<Option<DailyMetrics>, String> {
    let result = conn.query_row(
        "SELECT id, date_local, dictation_count, total_words, total_duration_ms, total_wpm_weighted_sum
         FROM metrics_daily WHERE date_local = ?1",
        [date_local],
        |row| {
            Ok(DailyMetrics {
                id: row.get(0)?,
                date_local: row.get(1)?,
                dictation_count: row.get(2)?,
                total_words: row.get(3)?,
                total_duration_ms: row.get(4)?,
                total_wpm_weighted_sum: row.get(5)?,
            })
        },
    );

    match result {
        Ok(m) => Ok(Some(m)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(format!("Failed to get daily metrics: {e}")),
    }
}

/// Get daily metrics for a date range (inclusive).
pub fn get_daily_range(
    conn: &Connection,
    from: &str,
    to: &str,
) -> Result<Vec<DailyMetrics>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, date_local, dictation_count, total_words, total_duration_ms, total_wpm_weighted_sum
             FROM metrics_daily
             WHERE date_local >= ?1 AND date_local <= ?2
             ORDER BY date_local ASC",
        )
        .map_err(|e| format!("Failed to prepare daily range query: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![from, to], |row| {
            Ok(DailyMetrics {
                id: row.get(0)?,
                date_local: row.get(1)?,
                dictation_count: row.get(2)?,
                total_words: row.get(3)?,
                total_duration_ms: row.get(4)?,
                total_wpm_weighted_sum: row.get(5)?,
            })
        })
        .map_err(|e| format!("Failed to query daily range: {e}"))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(rows)
}

/// Get per-engine breakdown for a date range.
pub fn get_engine_breakdown(
    conn: &Connection,
    from: &str,
    to: &str,
) -> Result<Vec<DailyEngineMetrics>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, date_local, engine_id, dictation_count, total_words, total_duration_ms
             FROM metrics_daily_engine
             WHERE date_local >= ?1 AND date_local <= ?2
             ORDER BY date_local ASC, engine_id ASC",
        )
        .map_err(|e| format!("Failed to prepare engine breakdown query: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![from, to], |row| {
            Ok(DailyEngineMetrics {
                id: row.get(0)?,
                date_local: row.get(1)?,
                engine_id: row.get(2)?,
                dictation_count: row.get(3)?,
                total_words: row.get(4)?,
                total_duration_ms: row.get(5)?,
            })
        })
        .map_err(|e| format!("Failed to query engine breakdown: {e}"))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(rows)
}

/// Get lifetime metrics (singleton).
pub fn get_lifetime(conn: &Connection) -> Result<LifetimeMetrics, String> {
    let result = conn.query_row(
        "SELECT total_words, total_dictations, total_wpm_weighted_sum, first_use_date, current_streak_days, longest_streak_days, typing_baseline_wpm
         FROM lifetime_metrics WHERE id = 1",
        [],
        |row| {
            Ok(LifetimeMetrics {
                total_words: row.get(0)?,
                total_dictations: row.get(1)?,
                total_wpm_weighted_sum: row.get(2)?,
                first_use_date: row.get(3)?,
                current_streak_days: row.get(4)?,
                longest_streak_days: row.get(5)?,
                typing_baseline_wpm: row.get(6)?,
            })
        },
    );

    match result {
        Ok(m) => Ok(m),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(LifetimeMetrics::default()),
        Err(e) => Err(format!("Failed to get lifetime metrics: {e}")),
    }
}

/// Set the typing baseline WPM for comparison.
pub fn set_typing_baseline(conn: &Connection, wpm: f64) -> Result<(), String> {
    conn.execute(
        "INSERT INTO lifetime_metrics (id, total_words, total_dictations, total_wpm_weighted_sum, current_streak_days, longest_streak_days, typing_baseline_wpm)
         VALUES (1, 0, 0, 0.0, 0, 0, ?1)
         ON CONFLICT(id) DO UPDATE SET typing_baseline_wpm = ?1",
        [wpm],
    )
    .map_err(|e| format!("Failed to set typing baseline: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn is_next_day_works() {
        assert!(is_next_day("2026-02-25", "2026-02-26"));
        assert!(!is_next_day("2026-02-25", "2026-02-27"));
        assert!(is_next_day("2026-02-28", "2026-03-01"));
        assert!(is_next_day("2026-12-31", "2027-01-01"));
        assert!(is_next_day("2024-02-28", "2024-02-29")); // leap year
        assert!(is_next_day("2024-02-29", "2024-03-01")); // leap year
        assert!(!is_next_day("2025-02-28", "2025-03-02")); // not leap year
    }

    #[test]
    fn record_and_get_daily() {
        let handle = db::open_memory();
        let conn = handle.blocking_lock();

        record_dictation(&conn, "2026-02-25", 10, 5000, 120.0, "whisper-tiny").unwrap();

        let daily = get_daily(&conn, "2026-02-25").unwrap().unwrap();
        assert_eq!(daily.dictation_count, 1);
        assert_eq!(daily.total_words, 10);
        assert_eq!(daily.total_duration_ms, 5000);
        assert!((daily.total_wpm_weighted_sum - 1200.0).abs() < 0.01);
    }

    #[test]
    fn record_multiple_same_day() {
        let handle = db::open_memory();
        let conn = handle.blocking_lock();

        record_dictation(&conn, "2026-02-25", 10, 5000, 120.0, "whisper-tiny").unwrap();
        record_dictation(&conn, "2026-02-25", 20, 8000, 150.0, "groq").unwrap();

        let daily = get_daily(&conn, "2026-02-25").unwrap().unwrap();
        assert_eq!(daily.dictation_count, 2);
        assert_eq!(daily.total_words, 30);
        assert_eq!(daily.total_duration_ms, 13000);
    }

    #[test]
    fn lifetime_updates() {
        let handle = db::open_memory();
        let conn = handle.blocking_lock();

        record_dictation(&conn, "2026-02-25", 10, 5000, 120.0, "whisper-tiny").unwrap();
        record_dictation(&conn, "2026-02-25", 20, 8000, 150.0, "groq").unwrap();

        let lt = get_lifetime(&conn).unwrap();
        assert_eq!(lt.total_dictations, 2);
        assert_eq!(lt.total_words, 30);
        assert_eq!(lt.first_use_date, Some("2026-02-25".to_string()));
    }

    #[test]
    fn streak_consecutive_days() {
        let handle = db::open_memory();
        let conn = handle.blocking_lock();

        record_dictation(&conn, "2026-02-25", 10, 5000, 120.0, "whisper-tiny").unwrap();
        record_dictation(&conn, "2026-02-26", 10, 5000, 120.0, "whisper-tiny").unwrap();
        record_dictation(&conn, "2026-02-27", 10, 5000, 120.0, "whisper-tiny").unwrap();

        let lt = get_lifetime(&conn).unwrap();
        assert_eq!(lt.current_streak_days, 3);
        assert_eq!(lt.longest_streak_days, 3);
    }

    #[test]
    fn streak_breaks_on_gap() {
        let handle = db::open_memory();
        let conn = handle.blocking_lock();

        record_dictation(&conn, "2026-02-25", 10, 5000, 120.0, "whisper-tiny").unwrap();
        record_dictation(&conn, "2026-02-26", 10, 5000, 120.0, "whisper-tiny").unwrap();
        // gap: skip Feb 27
        record_dictation(&conn, "2026-02-28", 10, 5000, 120.0, "whisper-tiny").unwrap();

        let lt = get_lifetime(&conn).unwrap();
        assert_eq!(lt.current_streak_days, 1);
        assert_eq!(lt.longest_streak_days, 2);
    }

    #[test]
    fn streak_no_over_increment_after_gap() {
        let handle = db::open_memory();
        let conn = handle.blocking_lock();

        // Day 1 and Day 2: consecutive
        record_dictation(&conn, "2026-02-25", 10, 5000, 120.0, "whisper-tiny").unwrap();
        record_dictation(&conn, "2026-02-26", 10, 5000, 120.0, "whisper-tiny").unwrap();
        // Gap: skip Feb 27
        // Day 4: after gap
        record_dictation(&conn, "2026-02-28", 10, 5000, 120.0, "whisper-tiny").unwrap();
        // Day 4: second dictation same day — must NOT increment streak
        record_dictation(&conn, "2026-02-28", 10, 5000, 120.0, "whisper-tiny").unwrap();
        // Day 5: consecutive from Day 4
        record_dictation(&conn, "2026-03-01", 10, 5000, 120.0, "whisper-tiny").unwrap();
        // Day 5: second dictation same day — must NOT increment streak
        record_dictation(&conn, "2026-03-01", 10, 5000, 120.0, "whisper-tiny").unwrap();

        let lt = get_lifetime(&conn).unwrap();
        assert_eq!(lt.current_streak_days, 2); // Feb 28 + Mar 1
        assert_eq!(lt.longest_streak_days, 2); // longest was Feb 25-26 = 2, Feb 28 - Mar 1 = 2
    }

    #[test]
    fn engine_breakdown() {
        let handle = db::open_memory();
        let conn = handle.blocking_lock();

        record_dictation(&conn, "2026-02-25", 10, 5000, 120.0, "whisper-tiny").unwrap();
        record_dictation(&conn, "2026-02-25", 20, 8000, 150.0, "groq").unwrap();
        record_dictation(&conn, "2026-02-25", 5, 3000, 100.0, "whisper-tiny").unwrap();

        let engines = get_engine_breakdown(&conn, "2026-02-25", "2026-02-25").unwrap();
        assert_eq!(engines.len(), 2);

        let groq = engines.iter().find(|e| e.engine_id == "groq").unwrap();
        assert_eq!(groq.dictation_count, 1);
        assert_eq!(groq.total_words, 20);

        let whisper = engines
            .iter()
            .find(|e| e.engine_id == "whisper-tiny")
            .unwrap();
        assert_eq!(whisper.dictation_count, 2);
        assert_eq!(whisper.total_words, 15);
    }

    #[test]
    fn daily_range() {
        let handle = db::open_memory();
        let conn = handle.blocking_lock();

        record_dictation(&conn, "2026-02-24", 10, 5000, 120.0, "whisper-tiny").unwrap();
        record_dictation(&conn, "2026-02-25", 20, 8000, 150.0, "whisper-tiny").unwrap();
        record_dictation(&conn, "2026-02-26", 15, 6000, 130.0, "whisper-tiny").unwrap();

        let range = get_daily_range(&conn, "2026-02-24", "2026-02-26").unwrap();
        assert_eq!(range.len(), 3);
        assert_eq!(range[0].date_local, "2026-02-24");
        assert_eq!(range[2].date_local, "2026-02-26");
    }

    #[test]
    fn typing_baseline() {
        let handle = db::open_memory();
        let conn = handle.blocking_lock();

        set_typing_baseline(&conn, 65.0).unwrap();
        let lt = get_lifetime(&conn).unwrap();
        assert!((lt.typing_baseline_wpm.unwrap() - 65.0).abs() < 0.01);
    }

    #[test]
    fn default_lifetime_when_empty() {
        let handle = db::open_memory();
        let conn = handle.blocking_lock();

        let lt = get_lifetime(&conn).unwrap();
        assert_eq!(lt.total_words, 0);
        assert_eq!(lt.total_dictations, 0);
        assert!(lt.first_use_date.is_none());
    }
}
