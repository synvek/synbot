//! Memory index — SQLite (sqlite-vec + FTS5) for hybrid search.
//!
//! Each agent has one `{agentId}.sqlite` under its memory dir. Tables:
//! - `chunks(id, source, content)` canonical store
//! - `vec_embeddings` vec0(embedding, +chunk_id, +content, +source)
//! - `memory_fts` FTS5(content) with rowid = chunk_id

use anyhow::{Context, Result};
use rusqlite::Connection;
use std::sync::Once;

use crate::config;

/// Default embedding dimension when using stub provider (no real embedding).
pub const DEFAULT_EMBED_DIM: u32 = 384;

static VEC_EXTENSION_REGISTERED: Once = Once::new();

fn ensure_vec_extension() {
    VEC_EXTENSION_REGISTERED.call_once(|| {
        unsafe {
            let init = sqlite_vec::sqlite3_vec_init;
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(init as *const ())));
        }
    });
}

/// In-memory stub: returns a constant-dimension zero vector.
/// Replace with a real EmbeddingProvider for production.
fn stub_embedding(_text: &str, dim: u32) -> Vec<f32> {
    vec![0.0; dim as usize]
}

/// Opens the index DB for an agent and ensures schema exists.
pub fn open_index(agent_id: &str) -> Result<Connection> {
    ensure_vec_extension();
    let dir = config::memory_dir(agent_id);
    std::fs::create_dir_all(&dir).context("create memory dir")?;
    let db_path = dir.join(format!("{}.sqlite", if agent_id.is_empty() { "main" } else { agent_id }));
    let conn = Connection::open(&db_path).context("open index db")?;
    create_tables_if_needed(&conn)?;
    Ok(conn)
}

fn create_tables_if_needed(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS chunks (
            id INTEGER PRIMARY KEY,
            source TEXT NOT NULL,
            content TEXT NOT NULL
        );
        CREATE VIRTUAL TABLE IF NOT EXISTS vec_embeddings USING vec0(
            embedding float[384],
            +chunk_id INTEGER,
            +content TEXT,
            +source TEXT
        );
        CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
            content,
            content=''
        );
        "#,
    )?;
    Ok(())
}

/// One indexed chunk (for search results), with optional hybrid score.
#[derive(Debug, Clone)]
pub struct IndexedChunk {
    pub id: i64,
    pub source: String,
    pub content: String,
    /// Combined score from vector + text (when from hybrid search).
    pub score: Option<f64>,
}

/// Cast f32 slice to u8 for sqlite-vec binary format.
fn embedding_as_bytes(embedding: &[f32]) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(
            embedding.as_ptr() as *const u8,
            embedding.len() * std::mem::size_of::<f32>(),
        )
    }
}

/// Add a text chunk to the index (embeds and inserts into vec + FTS).
pub fn index_chunk(conn: &mut Connection, source: &str, content: &str) -> Result<i64> {
    let embedding = stub_embedding(content, DEFAULT_EMBED_DIM);
    conn.execute(
        "INSERT INTO chunks (source, content) VALUES (?1, ?2)",
        [source, content],
    )?;
    let chunk_id = conn.last_insert_rowid();

    let bytes = embedding_as_bytes(&embedding);
    conn.execute(
        "INSERT INTO vec_embeddings (embedding, chunk_id, content, source) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![bytes, chunk_id, content, source],
    )?;

    conn.execute(
        "INSERT INTO memory_fts(memory_fts, rowid, content) VALUES ('insert', ?1, ?2)",
        rusqlite::params![chunk_id, content],
    )?;

    Ok(chunk_id)
}

/// Chunk MEMORY.md and memory/*.md into paragraphs (simple split on double newline).
pub fn chunk_text(content: &str, max_chunk_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    for para in content.split("\n\n") {
        let para = para.trim();
        if para.is_empty() {
            continue;
        }
        if para.len() <= max_chunk_chars {
            chunks.push(para.to_string());
        } else {
            for s in para.as_bytes().chunks(max_chunk_chars) {
                if let Ok(s) = std::str::from_utf8(s) {
                    chunks.push(s.to_string());
                }
            }
        }
    }
    if chunks.is_empty() && !content.trim().is_empty() {
        chunks.push(content.trim().to_string());
    }
    chunks
}

/// Index all content from MEMORY.md and memory/*.md for an agent.
/// Clears existing index and rebuilds (full reindex).
pub fn reindex_agent(agent_id: &str) -> Result<usize> {
    let dir = config::memory_dir(agent_id);
    let mut conn = open_index(agent_id)?;

    conn.execute("DELETE FROM chunks", [])?;
    // Virtual tables: drop and recreate to clear (vec0 may not support DELETE)
    conn.execute_batch(
        r#"
        DROP TABLE IF EXISTS vec_embeddings;
        DROP TABLE IF EXISTS memory_fts;
        CREATE VIRTUAL TABLE vec_embeddings USING vec0(
            embedding float[384],
            +chunk_id INTEGER,
            +content TEXT,
            +source TEXT
        );
        CREATE VIRTUAL TABLE memory_fts USING fts5(content, content='');
        "#,
    )?;

    let memory_md = dir.join("MEMORY.md");
    let notes_dir = dir.join("memory");
    let mut count = 0usize;

    if memory_md.exists() {
        let content = std::fs::read_to_string(&memory_md).unwrap_or_default();
        for c in chunk_text(&content, 2000) {
            index_chunk(&mut conn, "MEMORY.md", &c)?;
            count += 1;
        }
    }

    if notes_dir.is_dir() {
        for entry in std::fs::read_dir(&notes_dir).context("read notes dir")? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "md") {
                let source = path.file_name().and_then(|p| p.to_str()).unwrap_or("").to_string();
                let content = std::fs::read_to_string(&path).unwrap_or_default();
                for c in chunk_text(&content, 2000) {
                    index_chunk(&mut conn, &source, &c)?;
                    count += 1;
                }
            }
        }
    }

    Ok(count)
}

const LAST_INDEX_META_KEY: &str = "last_index_mtime";

fn max_mtime_paths(dir: &std::path::Path) -> Option<std::time::SystemTime> {
    let memory_md = dir.join("MEMORY.md");
    let mut max_mtime = memory_md.metadata().ok()?.modified().ok()?;
    let notes_dir = dir.join("memory");
    if notes_dir.is_dir() {
        for e in std::fs::read_dir(&notes_dir).ok()?.flatten() {
            if e.path().extension().map_or(false, |x| x == "md") {
                if let Ok(m) = e.metadata().and_then(|m| m.modified()) {
                    if m > max_mtime {
                        max_mtime = m;
                    }
                }
            }
        }
    }
    Some(max_mtime)
}

fn read_last_index_mtime(agent_id: &str) -> Option<std::time::SystemTime> {
    let dir = config::memory_dir(agent_id);
    let stamp_path = dir.join(".last_index");
    let s = std::fs::read_to_string(&stamp_path).ok()?;
    let nanos: u64 = s.trim().parse().ok()?;
    std::time::UNIX_EPOCH.checked_add(std::time::Duration::from_nanos(nanos))
}

fn write_last_index_mtime(agent_id: &str, t: std::time::SystemTime) -> Result<()> {
    let dir = config::memory_dir(agent_id);
    std::fs::create_dir_all(&dir)?;
    let stamp_path = dir.join(".last_index");
    let nanos = t.duration_since(std::time::UNIX_EPOCH).context("time")?.as_nanos() as u64;
    std::fs::write(stamp_path, format!("{}", nanos))?;
    Ok(())
}

/// If MEMORY.md or memory/*.md have changed since last index, run reindex and return count.
/// Otherwise return Ok(0). Call this periodically (e.g. every 60s) or after file writes.
pub fn reindex_if_changed(agent_id: &str) -> Result<usize> {
    let dir = config::memory_dir(agent_id);
    let current = match max_mtime_paths(&dir) {
        Some(t) => t,
        None => return Ok(0),
    };
    let last = read_last_index_mtime(agent_id);
    if last.map_or(true, |l| current > l) {
        let count = reindex_agent(agent_id)?;
        write_last_index_mtime(agent_id, current)?;
        return Ok(count);
    }
    Ok(0)
}

/// Result row from vector KNN (rowid = vec_embeddings rowid, not chunk_id).
struct VecHit {
    rowid: i64,
    distance: f64,
}

/// Hybrid search: vector KNN + FTS5, union by chunk_id, then sort by weighted score.
/// `vector_weight` + `text_weight` should be 1.0 (e.g. 0.7 and 0.3).
pub fn hybrid_search(
    conn: &Connection,
    query: &str,
    limit: usize,
    vector_weight: f64,
    text_weight: f64,
) -> Result<Vec<IndexedChunk>> {
    let embedding = stub_embedding(query, DEFAULT_EMBED_DIM);
    let bytes = embedding_as_bytes(&embedding);

    // 1) Vector KNN: get rowid and distance from vec_embeddings
    let vec_hits: Vec<VecHit> = conn
        .prepare(
            "SELECT rowid, distance FROM vec_embeddings WHERE embedding MATCH ?1 ORDER BY distance LIMIT ?2",
        )?
        .query_map(rusqlite::params![bytes, limit as i64], |row| {
            Ok(VecHit {
                rowid: row.get(0)?,
                distance: row.get(1)?,
            })
        })?
        .filter_map(Result::ok)
        .collect();

    // Map vec rowid -> (chunk_id, vector_score). vec0 rowid may not equal chunk_id; we need chunk_id from auxiliary.
    let mut chunk_vec_score: std::collections::HashMap<i64, f64> = std::collections::HashMap::new();
    for hit in &vec_hits {
        let chunk_id: i64 = conn.query_row(
            "SELECT chunk_id FROM vec_embeddings WHERE rowid = ?1",
            [hit.rowid],
            |r| r.get(0),
        )?;
        // Normalize distance to [0,1] score: 1/(1+distance)
        let score = 1.0 / (1.0 + hit.distance);
        chunk_vec_score
            .entry(chunk_id)
            .and_modify(|s| *s = (*s).max(score))
            .or_insert(score);
    }

    // 2) FTS5: get rowid (chunk_id) and bm25 rank
    let mut chunk_text_score: std::collections::HashMap<i64, f64> = std::collections::HashMap::new();
    let fts_query = query.replace('"', "\"\"");
    if let Ok(mut stmt) = conn.prepare(
        "SELECT rowid, bm25(memory_fts) FROM memory_fts WHERE memory_fts MATCH ?1 LIMIT ?2",
    ) {
        let rows = stmt.query_map(rusqlite::params![&fts_query, limit as i64], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, f64>(1)?))
        });
        if let Ok(rows) = rows {
            let mut min_rank = f64::INFINITY;
            let mut max_rank = f64::NEG_INFINITY;
            let mut raw: Vec<(i64, f64)> = Vec::new();
            for r in rows.filter_map(Result::ok) {
                raw.push(r);
                min_rank = min_rank.min(r.1);
                max_rank = max_rank.max(r.1);
            }
            let range = max_rank - min_rank;
            for (chunk_id, rank) in raw {
                // bm25: lower is better, so invert so best match has score 1
                let normalized = if range > 0.0 {
                    (max_rank - rank) / range
                } else {
                    1.0
                };
                chunk_text_score
                    .entry(chunk_id)
                    .and_modify(|s| *s = (*s).max(normalized))
                    .or_insert(normalized);
            }
        }
    }

    // 3) Union chunk ids and weighted score
    let all_ids: std::collections::HashSet<i64> = chunk_vec_score
        .keys()
        .chain(chunk_text_score.keys())
        .copied()
        .collect();
    let mut scored: Vec<(i64, f64)> = all_ids
        .into_iter()
        .map(|id| {
            let vs = chunk_vec_score.get(&id).copied().unwrap_or(0.0);
            let ts = chunk_text_score.get(&id).copied().unwrap_or(0.0);
            let score = vector_weight * vs + text_weight * ts;
            (id, score)
        })
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);

    // 4) Load chunk content from chunks table
    let mut out = Vec::with_capacity(scored.len());
    for (id, score) in scored {
        if let Ok((source, content)) = conn.query_row(
            "SELECT source, content FROM chunks WHERE id = ?1",
            [id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        ) {
            out.push(IndexedChunk {
                id,
                source,
                content,
                score: Some(score),
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn chunk_text_splits_paragraphs() {
        let s = "A\n\nB\n\nC";
        let c = chunk_text(s, 1000);
        assert_eq!(c.len(), 3);
        assert_eq!(c[0], "A");
        assert_eq!(c[1], "B");
        assert_eq!(c[2], "C");
    }

    #[test]
    fn chunk_text_respects_max_len() {
        let s = "a".repeat(5000);
        let c = chunk_text(&s, 1000);
        assert!(c.len() >= 5);
    }
}
