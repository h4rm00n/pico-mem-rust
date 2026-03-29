use anyhow::Result;
use arrow::array::{Array, FixedSizeListArray, RecordBatch, StringArray, UInt8Array, UInt32Array};
use arrow::datatypes::{DataType, Field, Schema, Float32Type};
use chrono::Local;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::table::Table;
use lancedb::{connect, Connection};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use futures::TryStreamExt;
use tracing::info;

use crate::api::ApiClient;
use crate::schema::{MemoryExtraction, MemoryType, TaskStatus};
use crate::config::MemoryConfig;

#[derive(Debug)]
pub enum StoreResult {
    Stored,
    Rejected {
        reason: String,
        similarity: f32,
    },
}

pub struct MemoryManager {
    buffer: Arc<Mutex<Vec<String>>>,
    last_event_time: Arc<Mutex<Instant>>,
    table: Arc<Mutex<Option<Table>>>,
    api_client: Arc<ApiClient>,
    max_memory_results: usize,
    idle_timeout: Duration,
    total_messages_added: Arc<Mutex<usize>>,
    embedding_dim: i32,
    overlap_threshold: f32,
    enable_dedup: bool,
    similarity_weight: f32,
    importance_weight: f32,
}

impl MemoryManager {
    pub async fn new(
        db_path: &str,
        collection_name: &str,
        api_client: ApiClient,
        embedding_dim: usize,
        memory_config: &MemoryConfig,
    ) -> Result<Self> {
        if embedding_dim == 0 || embedding_dim > 8192 {
            anyhow::bail!("Invalid embedding dimension: {}, must be 1-8192", embedding_dim);
        }
        
        let embedding_dim = embedding_dim as i32;
        
        let db = connect(db_path).execute().await?;
        
        let table = if db.table_names().execute().await?.contains(&collection_name.to_string()) {
            Some(db.open_table(collection_name).execute().await?)
        } else {
            None
        };

        Ok(Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
            last_event_time: Arc::new(Mutex::new(Instant::now())),
            table: Arc::new(Mutex::new(table)),
            api_client: Arc::new(api_client),
            max_memory_results: memory_config.max_memory_results,
            idle_timeout: Duration::from_secs(memory_config.idle_timeout_minutes * 60),
            total_messages_added: Arc::new(Mutex::new(0)),
            embedding_dim,
            overlap_threshold: memory_config.overlap_threshold,
            enable_dedup: memory_config.enable_dedup,
            similarity_weight: memory_config.similarity_weight,
            importance_weight: memory_config.importance_weight,
        })
    }

    pub async fn add_message(&self, event_type: &str, content: &str) {
        let mut buffer = self.buffer.lock().await;
        buffer.push(format!("[{}]: {}", event_type, content));
        let mut last_time = self.last_event_time.lock().await;
        *last_time = Instant::now();
        let mut total = self.total_messages_added.lock().await;
        *total += 1;
    }

    pub async fn should_summarize(&self) -> bool {
        let buffer = self.buffer.lock().await;
        let last_time = self.last_event_time.lock().await;
        !buffer.is_empty() && last_time.elapsed() > self.idle_timeout
    }

    pub async fn get_and_clear_buffer(&self) -> String {
        let mut buffer = self.buffer.lock().await;
        let text = buffer.join("\n");
        buffer.clear();
        text
    }

    pub async fn get_debug_info(&self) -> serde_json::Value {
        let buffer = self.buffer.lock().await;
        let last_time = self.last_event_time.lock().await;
        let total = self.total_messages_added.lock().await;
        
        serde_json::json!({
            "buffer_size": buffer.len(),
            "total_messages_added": *total,
            "last_event_time": last_time.elapsed().as_secs_f64(),
            "idle_timeout_threshold": self.idle_timeout.as_secs(),
            "should_summarize": !buffer.is_empty() && last_time.elapsed() > self.idle_timeout
        })
    }

    pub async fn store_new_memory(
        &self,
        db: &Connection,
        collection_name: &str,
        memory: &MemoryExtraction,
        vector: Vec<f32>,
    ) -> Result<StoreResult> {
        if self.enable_dedup {
            let table_guard = self.table.lock().await;
            if let Some(table) = table_guard.as_ref() {
                let batches = table
                    .query()
                    .nearest_to(vector.as_slice())?
                    .limit(1)
                    .execute()
                    .await?
                    .try_collect::<Vec<_>>()
                    .await?;

                if let Some(batch) = batches.first() {
                    if let (Some(importance_col), Some(_distance_col)) = (
                        batch.column_by_name("importance"),
                        batch.column_by_name("_distance")
                    ) {
                        let importances = importance_col.as_any().downcast_ref::<UInt8Array>().unwrap();
                        
                        if importances.len() > 0 {
                            let old_importance = importances.value(0);
                            let distance = batch.column_by_name("_distance")
                                .and_then(|col| {
                                    col.as_any().downcast_ref::<arrow::array::Float32Array>()
                                })
                                .map(|arr| arr.value(0))
                                .unwrap_or(1.0);
                            
                            let similarity = 1.0 - distance;
                            
                            if similarity > self.overlap_threshold && memory.importance <= old_importance {
                                return Ok(StoreResult::Rejected {
                                    reason: "高重叠且低重要性".to_string(),
                                    similarity,
                                });
                            }
                        }
                    }
                }
            }
        }

        self.store_memory(db, collection_name, memory, vector).await?;
        Ok(StoreResult::Stored)
    }

    pub async fn store_memory(
        &self,
        db: &Connection,
        collection_name: &str,
        memory: &MemoryExtraction,
        vector: Vec<f32>,
    ) -> Result<()> {
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let id_str = uuid::Uuid::new_v4().to_string();
        
        let memory_type_str = match memory.memory_type {
            MemoryType::Preference => "preference",
            MemoryType::Fact => "fact",
            MemoryType::Task => "task",
            MemoryType::Other => "other",
        };
        
        let status_str = match &memory.status {
            Some(TaskStatus::InProgress) => "in_progress",
            Some(TaskStatus::Done) => "done",
            Some(TaskStatus::Other) | None => "",
        };

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("summary", DataType::Utf8, false),
            Field::new("domain", DataType::Utf8, false),
            Field::new("memory_type", DataType::Utf8, false),
            Field::new("importance", DataType::UInt8, false),
            Field::new("status", DataType::Utf8, false),
            Field::new("access_count", DataType::UInt32, false),
            Field::new("timestamp", DataType::Utf8, false),
            Field::new(
                "vector",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    self.embedding_dim,
                ),
                false,
            ),
        ]));

        let record_batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(StringArray::from(vec![id_str])),
                Arc::new(StringArray::from(vec![memory.summary.clone()])),
                Arc::new(StringArray::from(vec![memory.domain.clone()])),
                Arc::new(StringArray::from(vec![memory_type_str])),
                Arc::new(UInt8Array::from(vec![memory.importance])),
                Arc::new(StringArray::from(vec![status_str])),
                Arc::new(UInt32Array::from(vec![0u32])),
                Arc::new(StringArray::from(vec![timestamp])),
                Arc::new(
                    FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
                        vec![Some(vector.iter().map(|v| Some(*v)).collect::<Vec<_>>())],
                        self.embedding_dim,
                    ),
                ),
            ],
        )?;

        let mut table_guard = self.table.lock().await;
        
        if let Some(table) = table_guard.as_ref() {
            table.add(record_batch).execute().await?;
        } else {
            let table = db.create_table(collection_name, record_batch)
                .execute()
                .await?;
            *table_guard = Some(table);
        }

        Ok(())
    }

    pub async fn search_with_rerank(&self, query: &str, limit: usize) -> Result<Vec<serde_json::Value>> {
        let query_vector = self.api_client.get_embedding(query).await?;
        
        let table_guard = self.table.lock().await;
        if let Some(table) = table_guard.as_ref() {
            let batches = table
                .query()
                .nearest_to(query_vector.as_slice())?
                .limit(limit)
                .execute()
                .await?
                .try_collect::<Vec<_>>()
                .await?;

            let mut candidates: Vec<(f32, serde_json::Value)> = Vec::new();
            for batch in batches {
                if let (Some(id_col), Some(summary_col), Some(timestamp_col), Some(domain_col),
                        Some(memory_type_col), Some(importance_col), Some(status_col),
                        Some(distance_col)) = (
                    batch.column_by_name("id"),
                    batch.column_by_name("summary"),
                    batch.column_by_name("timestamp"),
                    batch.column_by_name("domain"),
                    batch.column_by_name("memory_type"),
                    batch.column_by_name("importance"),
                    batch.column_by_name("status"),
                    batch.column_by_name("_distance")
                ) {
                    let ids = id_col.as_any().downcast_ref::<StringArray>().unwrap();
                    let summaries = summary_col.as_any().downcast_ref::<StringArray>().unwrap();
                    let timestamps = timestamp_col.as_any().downcast_ref::<StringArray>().unwrap();
                    let domains = domain_col.as_any().downcast_ref::<StringArray>().unwrap();
                    let memory_types = memory_type_col.as_any().downcast_ref::<StringArray>().unwrap();
                    let importances = importance_col.as_any().downcast_ref::<UInt8Array>().unwrap();
                    let statuses = status_col.as_any().downcast_ref::<StringArray>().unwrap();
                    let distances = distance_col.as_any().downcast_ref::<arrow::array::Float32Array>().unwrap();
                    
                    for i in 0..summaries.len() {
                        let distance = distances.value(i);
                        let similarity = 1.0 - distance;
                        
                        let importance_normalized = importances.value(i) as f32 / 10.0;
                        let score = (similarity * self.similarity_weight) + (importance_normalized * self.importance_weight);
                        
                        candidates.push((
                            score,
                            serde_json::json!({
                                "id": ids.value(i),
                                "summary": summaries.value(i),
                                "timestamp": timestamps.value(i),
                                "domain": domains.value(i),
                                "memory_type": memory_types.value(i),
                                "importance": importances.value(i),
                                "status": statuses.value(i),
                                "score": score
                            })
                        ));
                    }
                }
            }
            
            candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
            
            return Ok(candidates
                .into_iter()
                .take(self.max_memory_results)
                .map(|(_, record)| record)
                .collect());
        }

        Ok(Vec::new())
    }

    pub async fn check_pending_tasks(&self) -> Result<Vec<serde_json::Value>> {
        let table_guard = self.table.lock().await;
        if let Some(table) = table_guard.as_ref() {
            let batches = table
                .query()
                .only_if("memory_type = 'task' AND status = 'in_progress'")
                .limit(5)
                .execute()
                .await?
                .try_collect::<Vec<_>>()
                .await?;

            let mut tasks = Vec::new();
            for batch in batches {
                if let (Some(summary_col), Some(timestamp_col), Some(domain_col),
                        Some(importance_col)) = (
                    batch.column_by_name("summary"),
                    batch.column_by_name("timestamp"),
                    batch.column_by_name("domain"),
                    batch.column_by_name("importance")
                ) {
                    let summaries = summary_col.as_any().downcast_ref::<StringArray>().unwrap();
                    let timestamps = timestamp_col.as_any().downcast_ref::<StringArray>().unwrap();
                    let domains = domain_col.as_any().downcast_ref::<StringArray>().unwrap();
                    let importances = importance_col.as_any().downcast_ref::<UInt8Array>().unwrap();
                    
                    for i in 0..summaries.len() {
                        tasks.push(serde_json::json!({
                            "summary": summaries.value(i),
                            "timestamp": timestamps.value(i),
                            "domain": domains.value(i),
                            "importance": importances.value(i)
                        }));
                    }
                }
            }
            
            return Ok(tasks);
        }

        Ok(Vec::new())
    }

    pub async fn summarize_and_store(&self, db: &Connection, collection_name: &str, text: &str) -> Result<()> {
        let memories = self.api_client.summarize_with_schema(text).await?;
        info!("Generated {} memory entries from conversation", memories.len());
        
        for memory in memories {
            info!(
                "Memory entry: summary={}, domain={}, type={:?}, importance={}, status={:?}",
                memory.summary.chars().take(100).collect::<String>(),
                memory.domain,
                memory.memory_type,
                memory.importance,
                memory.status
            );
            
            let vector = self.api_client.get_embedding(&memory.summary).await?;
            let result = self.store_new_memory(db, collection_name, &memory, vector).await?;
            
            match result {
                StoreResult::Stored => {
                    info!("Memory engram successfully stored.");
                }
                StoreResult::Rejected { reason, similarity } => {
                    info!("Memory rejected: {} (similarity={:.3})", reason, similarity);
                }
            }
        }
        
        Ok(())
    }
}