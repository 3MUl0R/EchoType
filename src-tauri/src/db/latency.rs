use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// A single latency measurement for one dictation event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyEntry {
    pub id: i64,
    pub created_at: String,
    pub engine_id: String,
    pub audio_duration_ms: i64,
    /// Time from hotkey release to transcription request ready (local processing).
    pub processing_ms: i64,
    /// Network round-trip time (cloud only; 0 for local).
    pub network_ms: i64,
    /// Time the engine spent transcribing (included in network_ms for cloud).
    pub transcription_ms: i64,
    /// Time to insert text into target application.
    pub insertion_ms: i64,
    /// Total end-to-end time from hotkey release to text inserted.
    pub total_ms: i64,
    pub word_count: i64,
}

/// Parameters for recording a latency measurement.
pub struct InsertParams<'a> {
    pub created_at: &'a str,
    pub engine_id: &'a str,
    pub audio_duration_ms: i64,
    pub processing_ms: i64,
    pub network_ms: i64,
    pub transcription_ms: i64,
    pub insertion_ms: i64,
    pub total_ms: i64,
    pub word_count: i64,
}

/// Insert a latency measurement.
pub fn insert(conn: &Connection, params: &InsertParams) -> Result<i64, String> {
    conn.execute(
        "INSERT INTO latency_log (created_at, engine_id, audio_duration_ms, processing_ms, network_ms, transcription_ms, insertion_ms, total_ms, word_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            params.created_at,
            params.engine_id,
            params.audio_duration_ms,
            params.processing_ms,
            params.network_ms,
            params.transcription_ms,
            params.insertion_ms,
            params.total_ms,
            params.word_count,
        ],
    )
    .map_err(|e| format!("Failed to insert latency entry: {e}"))?;

    Ok(conn.last_insert_rowid())
}

/// Aggregated latency statistics per engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineLatencyStats {
    pub engine_id: String,
    pub sample_count: i64,
    pub avg_processing_ms: f64,
    pub avg_network_ms: f64,
    pub avg_transcription_ms: f64,
    pub avg_insertion_ms: f64,
    pub avg_total_ms: f64,
    pub p50_total_ms: i64,
    pub p95_total_ms: i64,
    pub avg_audio_duration_ms: f64,
    pub avg_word_count: f64,
    /// Processing time normalized: ms per second of audio (averaged per-sample).
    pub processing_per_sec: f64,
    /// Transcription time normalized: ms per second of audio (averaged per-sample).
    pub transcription_per_sec: f64,
}

/// Get aggregated latency stats grouped by engine.
pub fn get_engine_stats(conn: &Connection) -> Result<Vec<EngineLatencyStats>, String> {
    // First get the list of engines and their aggregate stats.
    // Normalized rates (per second of audio) are computed per-sample then averaged,
    // which is more accurate than dividing aggregate averages.
    let mut stmt = conn
        .prepare(
            "SELECT engine_id,
                    COUNT(*) AS cnt,
                    AVG(processing_ms) AS avg_proc,
                    AVG(network_ms) AS avg_net,
                    AVG(transcription_ms) AS avg_trans,
                    AVG(insertion_ms) AS avg_ins,
                    AVG(total_ms) AS avg_total,
                    AVG(audio_duration_ms) AS avg_audio,
                    AVG(word_count) AS avg_words,
                    AVG(CASE WHEN audio_duration_ms > 0
                         THEN processing_ms * 1000.0 / audio_duration_ms
                         ELSE 0 END) AS proc_per_sec,
                    AVG(CASE WHEN audio_duration_ms > 0
                         THEN transcription_ms * 1000.0 / audio_duration_ms
                         ELSE 0 END) AS trans_per_sec
             FROM latency_log
             GROUP BY engine_id
             ORDER BY cnt DESC",
        )
        .map_err(|e| format!("Failed to prepare latency stats query: {e}"))?;

    let engines: Vec<(String, i64, f64, f64, f64, f64, f64, f64, f64, f64, f64)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, f64>(2)?,
                row.get::<_, f64>(3)?,
                row.get::<_, f64>(4)?,
                row.get::<_, f64>(5)?,
                row.get::<_, f64>(6)?,
                row.get::<_, f64>(7)?,
                row.get::<_, f64>(8)?,
                row.get::<_, f64>(9)?,
                row.get::<_, f64>(10)?,
            ))
        })
        .map_err(|e| format!("Failed to query latency stats: {e}"))?
        .filter_map(|r| r.ok())
        .collect();

    let mut results = Vec::new();
    for (engine_id, cnt, avg_proc, avg_net, avg_trans, avg_ins, avg_total, avg_audio, avg_words, proc_per_sec, trans_per_sec) in engines {
        // Compute percentiles for this engine
        let p50 = get_percentile(conn, &engine_id, 50)?;
        let p95 = get_percentile(conn, &engine_id, 95)?;

        results.push(EngineLatencyStats {
            engine_id,
            sample_count: cnt,
            avg_processing_ms: avg_proc,
            avg_network_ms: avg_net,
            avg_transcription_ms: avg_trans,
            avg_insertion_ms: avg_ins,
            avg_total_ms: avg_total,
            p50_total_ms: p50,
            p95_total_ms: p95,
            avg_audio_duration_ms: avg_audio,
            avg_word_count: avg_words,
            processing_per_sec: proc_per_sec,
            transcription_per_sec: trans_per_sec,
        });
    }

    Ok(results)
}

/// Get a percentile of total_ms for a given engine.
fn get_percentile(conn: &Connection, engine_id: &str, percentile: u32) -> Result<i64, String> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM latency_log WHERE engine_id = ?1",
            [engine_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("Failed to count latency entries: {e}"))?;

    if count == 0 {
        return Ok(0);
    }

    let idx = ((count as f64 * percentile as f64 / 100.0).ceil() as i64 - 1).max(0);

    conn.query_row(
        "SELECT total_ms FROM latency_log WHERE engine_id = ?1 ORDER BY total_ms ASC LIMIT 1 OFFSET ?2",
        rusqlite::params![engine_id, idx],
        |row| row.get(0),
    )
    .map_err(|e| format!("Failed to get percentile: {e}"))
}

/// Get recent latency entries for a specific engine.
pub fn get_recent(conn: &Connection, engine_id: &str, limit: i64) -> Result<Vec<LatencyEntry>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, created_at, engine_id, audio_duration_ms, processing_ms, network_ms, transcription_ms, insertion_ms, total_ms, word_count
             FROM latency_log
             WHERE engine_id = ?1
             ORDER BY created_at DESC
             LIMIT ?2",
        )
        .map_err(|e| format!("Failed to prepare recent latency query: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![engine_id, limit], |row| {
            Ok(LatencyEntry {
                id: row.get(0)?,
                created_at: row.get(1)?,
                engine_id: row.get(2)?,
                audio_duration_ms: row.get(3)?,
                processing_ms: row.get(4)?,
                network_ms: row.get(5)?,
                transcription_ms: row.get(6)?,
                insertion_ms: row.get(7)?,
                total_ms: row.get(8)?,
                word_count: row.get(9)?,
            })
        })
        .map_err(|e| format!("Failed to query recent latency: {e}"))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(rows)
}
