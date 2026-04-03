//! Memory index — SQLite (sqlite-vec + FTS5) for hybrid search.
//!
//! Each agent has one `{agentId}.sqlite` under its memory dir. Tables:
//! - `chunks(id, source, content)` canonical store
//! - `vec_embeddings` vec0(embedding, +chunk_id, +content, +source)
//! - `memory_fts` FTS5(content) with rowid = chunk_id

use anyhow::{Context, Result};
use rusqlite::Connection;
use std::sync::Once;

use crate::config::Config;
use crate::config;
use crate::agent::embeddings;

/// Default embedding dimension when using stub provider (no real embedding).
pub const DEFAULT_EMBED_DIM: u32 = 768;

static VEC_EXTENSION_REGISTERED: Once = Once::new();

fn ensure_vec_extension() {
    VEC_EXTENSION_REGISTERED.call_once(|| {
        unsafe {
            let init = sqlite_vec::sqlite3_vec_init;
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(init as *const ())));
        }
    });
}

fn stub_embedding(dim: u32) -> Vec<f32> {
    vec![0.0; dim as usize]
}

fn db_path(agent_id: &str) -> std::path::PathBuf {
    let dir = config::memory_dir(agent_id);
    dir.join(format!("{}.sqlite", if agent_id.is_empty() { "main" } else { agent_id }))
}

fn dim_marker_path(agent_id: &str) -> std::path::PathBuf {
    config::memory_dir(agent_id).join(".embedding_dim")
}

/// If embedding dimension changed, remove the SQLite DB so vec0 can be recreated with a new width.
/// If `.embedding_dim` is missing but `{agent}.sqlite` still exists (e.g. user deleted only the marker),
/// the old vec0 width would not match new embeddings — remove the DB in that case too.
fn maybe_reset_sqlite_for_dim_change(agent_id: &str, dim: u32) -> Result<()> {
    let dir = config::memory_dir(agent_id);
    std::fs::create_dir_all(&dir)?;
    let marker = dim_marker_path(agent_id);
    let db = db_path(agent_id);
    let need_reset = match std::fs::read_to_string(&marker) {
        Ok(s) => s.trim().parse::<u32>().ok() != Some(dim),
        Err(_) => db.exists(),
    };
    if need_reset {
        let _ = std::fs::remove_file(&db);
        let _ = std::fs::remove_file(dir.join(".last_index"));
    }
    std::fs::write(marker, format!("{}", dim))?;
    Ok(())
}

/// Opens the index DB for an agent and ensures schema exists.
pub fn open_index(agent_id: &str, embedding_dim: u32) -> Result<Connection> {
    ensure_vec_extension();
    maybe_reset_sqlite_for_dim_change(agent_id, embedding_dim)?;
    let dir = config::memory_dir(agent_id);
    std::fs::create_dir_all(&dir).context("create memory dir")?;
    let db_path = db_path(agent_id);
    let conn = Connection::open(&db_path).context("open index db")?;
    create_tables_if_needed(&conn, embedding_dim)?;
    Ok(conn)
}

fn create_tables_if_needed(conn: &Connection, embedding_dim: u32) -> Result<()> {
    let vec_sql = format!(
        r#"
        CREATE TABLE IF NOT EXISTS chunks (
            id INTEGER PRIMARY KEY,
            source TEXT NOT NULL,
            content TEXT NOT NULL
        );
        CREATE VIRTUAL TABLE IF NOT EXISTS vec_embeddings USING vec0(
            embedding float[{dim}],
            +chunk_id INTEGER,
            +content TEXT,
            +source TEXT
        );
        CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
            content,
            content=''
        );
        "#,
        dim = embedding_dim
    );
    conn.execute_batch(&vec_sql)?;
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

/// Add a text chunk with a precomputed embedding.
pub fn index_chunk_with_embedding(
    conn: &mut Connection,
    source: &str,
    content: &str,
    embedding: &[f32],
) -> Result<i64> {
    conn.execute(
        "INSERT INTO chunks (source, content) VALUES (?1, ?2)",
        [source, content],
    )?;
    let chunk_id = conn.last_insert_rowid();

    let bytes = embedding_as_bytes(embedding);
    conn.execute(
        "INSERT INTO vec_embeddings (embedding, chunk_id, content, source) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![bytes, chunk_id, content, source],
    )?;

    // Standard FTS5 row insert (avoids SQL logic errors with contentless / shadow-style forms).
    conn.execute(
        "INSERT INTO memory_fts(rowid, content) VALUES (?1, ?2)",
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

/// Full reindex using async embeddings (loads config for API keys and dimensions).
pub async fn reindex_agent_async(agent_id: &str, config: &Config) -> Result<usize> {
    let dim = config.memory.embedding_dimensions;
    maybe_reset_sqlite_for_dim_change(agent_id, dim)?;
    let dir = config::memory_dir(agent_id);
    let mut conn = open_index(agent_id, dim)?;

    conn.execute("DELETE FROM chunks", [])?;
    conn.execute_batch(
        r#"
        DROP TABLE IF EXISTS vec_embeddings;
        DROP TABLE IF EXISTS memory_fts;
        "#,
    )?;
    let recreate = format!(
        r#"
        CREATE VIRTUAL TABLE vec_embeddings USING vec0(
            embedding float[{dim}],
            +chunk_id INTEGER,
            +content TEXT,
            +source TEXT
        );
        CREATE VIRTUAL TABLE memory_fts USING fts5(content, content='');
        "#,
        dim = dim
    );
    conn.execute_batch(&recreate)?;

    let memory_md = dir.join("MEMORY.md");
    let notes_dir = dir.join("memory");
    let mut count = 0usize;

    if memory_md.exists() {
        let content = std::fs::read_to_string(&memory_md).unwrap_or_default();
        for c in chunk_text(&content, 2000) {
            let emb = embeddings::embed_text(config, &c).await?;
            index_chunk_with_embedding(&mut conn, "MEMORY.md", &c, &emb)?;
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
                    let emb = embeddings::embed_text(config, &c).await?;
                    index_chunk_with_embedding(&mut conn, &source, &c, &emb)?;
                    count += 1;
                }
            }
        }
    }

    Ok(count)
}

/// Sync wrapper for contexts without an async runtime (e.g. tests).
pub fn reindex_agent_blocking(agent_id: &str, config: &Config) -> Result<usize> {
    if let Ok(h) = tokio::runtime::Handle::try_current() {
        h.block_on(reindex_agent_async(agent_id, config))
    } else {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("build tokio runtime for reindex")?
            .block_on(reindex_agent_async(agent_id, config))
    }
}

/// Backward-compatible sync entry (loads config from disk).
pub fn reindex_agent(agent_id: &str) -> Result<usize> {
    let cfg = config::load_config(None)?;
    reindex_agent_blocking(agent_id, &cfg)
}

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
pub async fn reindex_if_changed_async(agent_id: &str, config: &Config) -> Result<usize> {
    let dir = config::memory_dir(agent_id);
    let current = match max_mtime_paths(&dir) {
        Some(t) => t,
        None => return Ok(0),
    };
    let last = read_last_index_mtime(agent_id);
    if last.map_or(true, |l| current > l) {
        let count = reindex_agent_async(agent_id, config).await?;
        write_last_index_mtime(agent_id, current)?;
        return Ok(count);
    }
    Ok(0)
}

/// Sync variant.
pub fn reindex_if_changed_blocking(agent_id: &str, config: &Config) -> Result<usize> {
    if let Ok(h) = tokio::runtime::Handle::try_current() {
        h.block_on(reindex_if_changed_async(agent_id, config))
    } else {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("build tokio runtime for reindex_if_changed")?
            .block_on(reindex_if_changed_async(agent_id, config))
    }
}

/// Legacy sync API (loads config from disk).
pub fn reindex_if_changed(agent_id: &str) -> Result<usize> {
    let cfg = config::load_config(None)?;
    reindex_if_changed_blocking(agent_id, &cfg)
}

/// Result row from vector KNN (rowid = vec_embeddings rowid, not chunk_id).
struct VecHit {
    rowid: i64,
    distance: f64,
}

/// Hybrid search: vector KNN + FTS5, union by chunk_id, then sort by weighted score.
/// When `query_embedding` is `None`, uses a zero vector of `embed_dim` (stub) for the query side.
pub fn hybrid_search(
    conn: &Connection,
    query: &str,
    limit: usize,
    vector_weight: f64,
    text_weight: f64,
    query_embedding: Option<&[f32]>,
    embed_dim: u32,
) -> Result<Vec<IndexedChunk>> {
    let embedding: Vec<f32> = match query_embedding {
        Some(e) => {
            if e.len() != embed_dim as usize {
                anyhow::bail!(
                    "query embedding dim {} != {}",
                    e.len(),
                    embed_dim
                );
            }
            e.to_vec()
        }
        None => stub_embedding(embed_dim),
    };
    let bytes = embedding_as_bytes(&embedding);

    // 1) Vector KNN
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

    let mut chunk_vec_score: std::collections::HashMap<i64, f64> = std::collections::HashMap::new();
    for hit in &vec_hits {
        let chunk_id: i64 = conn.query_row(
            "SELECT chunk_id FROM vec_embeddings WHERE rowid = ?1",
            [hit.rowid],
            |r| r.get(0),
        )?;
        let score = 1.0 / (1.0 + hit.distance);
        chunk_vec_score
            .entry(chunk_id)
            .and_modify(|s| *s = (*s).max(score))
            .or_insert(score);
    }

    // 2) FTS5
    let mut chunk_text_score: std::collections::HashMap<i64, f64> = std::collections::HashMap::new();
    let fts_query = query.replace('"', "\"\"");
    if !fts_query.trim().is_empty() {
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
    }

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

/// Hybrid search using [`Config`] for embedding API and dimensions.
pub fn hybrid_search_with_config(
    conn: &Connection,
    query: &str,
    limit: usize,
    config: &Config,
) -> Result<Vec<IndexedChunk>> {
    let dim = config.memory.embedding_dimensions;
    let q_emb = embeddings::try_embed_query_sync(config, query);
    let q_slice = q_emb.as_deref();
    hybrid_search(
        conn,
        query,
        limit,
        config.memory.vector_weight as f64,
        config.memory.text_weight as f64,
        q_slice,
        dim,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
