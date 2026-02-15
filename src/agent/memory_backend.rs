//! Memory backend abstraction — trait and default file+SQLite implementation.

use chrono::NaiveDate;

use crate::agent::memory::MemoryStore;
use crate::agent::memory_index::{hybrid_search, reindex_agent, IndexedChunk};
use crate::config::{memory_dir, MemoryConfig};

/// Options for building memory context (e.g. recent days, use search).
#[derive(Debug, Clone)]
pub struct MemoryContextOptions {
    pub recent_days: u32,
    pub query_for_search: Option<String>,
    pub search_limit: usize,
}

impl Default for MemoryContextOptions {
    fn default() -> Self {
        Self {
            recent_days: 1,
            query_for_search: None,
            search_limit: 5,
        }
    }
}

/// Backend for agent memory: storage + optional index and search.
pub trait MemoryBackend: Send + Sync {
    fn get_memory_context(
        &self,
        agent_id: &str,
        options: &MemoryContextOptions,
    ) -> anyhow::Result<String>;

    fn search(
        &self,
        agent_id: &str,
        query: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<IndexedChunk>>;

    fn append_long_term(&self, agent_id: &str, content: &str) -> anyhow::Result<()>;

    fn append_daily_note(
        &self,
        agent_id: &str,
        date: NaiveDate,
        content: &str,
    ) -> anyhow::Result<()>;

    fn index_now(&self, agent_id: &str) -> anyhow::Result<usize> {
        reindex_agent(agent_id)
    }
}

/// Returns true if compression is enabled and message count exceeds threshold.
pub fn should_compress(config: &MemoryConfig, message_count: usize) -> bool {
    config.compression.enabled
        && message_count > config.compression.max_conversation_turns as usize
}

/// Default backend: ~/.synbot/memory/{agentId}, MEMORY.md + memory/YYYY-MM-DD.md + SQLite index.
pub struct FileSqliteMemoryBackend {
    config: MemoryConfig,
}

impl FileSqliteMemoryBackend {
    pub fn new(config: MemoryConfig) -> Self {
        Self { config }
    }
}

impl MemoryBackend for FileSqliteMemoryBackend {
    fn get_memory_context(
        &self,
        agent_id: &str,
        options: &MemoryContextOptions,
    ) -> anyhow::Result<String> {
        let store = MemoryStore::new(agent_id);
        let mut parts = Vec::new();

        let lt = store.read_long_term();
        if !lt.is_empty() {
            parts.push(format!("## Long-term Memory\n\n{}", lt));
        }

        let days = if options.recent_days > 0 {
            options.recent_days
        } else {
            1
        };
        let recent = store.get_recent_memories(days);
        if !recent.is_empty() {
            parts.push(format!("## Recent Notes ({} days)\n\n{}", days, recent));
        }

        if let Some(ref q) = options.query_for_search {
            if !q.trim().is_empty() {
                let limit = options.search_limit.min(10);
                if let Ok(conn) = crate::agent::memory_index::open_index(agent_id) {
                    if let Ok(hits) = hybrid_search(
                        &conn,
                        q,
                        limit,
                        self.config.vector_weight as f64,
                        self.config.text_weight as f64,
                    ) {
                        if !hits.is_empty() {
                            let search_block: String = hits
                                .into_iter()
                                .map(|c| format!("- [{}] {}", c.source, c.content))
                                .collect::<Vec<_>>()
                                .join("\n\n");
                            parts.push(format!("## Relevant memory (search)\n\n{}", search_block));
                        }
                    }
                }
            }
        }

        Ok(parts.join("\n\n"))
    }

    fn search(
        &self,
        agent_id: &str,
        query: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<IndexedChunk>> {
        let conn = crate::agent::memory_index::open_index(agent_id)?;
        hybrid_search(
            &conn,
            query,
            limit,
            self.config.vector_weight as f64,
            self.config.text_weight as f64,
        )
    }

    fn append_long_term(&self, agent_id: &str, content: &str) -> anyhow::Result<()> {
        let dir = memory_dir(agent_id);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("MEMORY.md");
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        let new_content = if existing.is_empty() {
            content.to_string()
        } else {
            format!("{}\n\n{}", existing.trim_end(), content)
        };
        std::fs::write(path, new_content)?;
        Ok(())
    }

    fn append_daily_note(
        &self,
        agent_id: &str,
        date: NaiveDate,
        content: &str,
    ) -> anyhow::Result<()> {
        let dir = memory_dir(agent_id);
        let notes_dir = dir.join("memory");
        std::fs::create_dir_all(&notes_dir)?;
        let path = notes_dir.join(format!("{}.md", date.format("%Y-%m-%d")));
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        let new_content = if existing.is_empty() {
            content.to_string()
        } else {
            format!("{}\n\n{}", existing.trim_end(), content)
        };
        std::fs::write(path, new_content)?;
        Ok(())
    }
}
