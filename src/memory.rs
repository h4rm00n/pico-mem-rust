use anyhow::Result;
use arrow::array::{Array, FixedSizeListArray, RecordBatch, StringArray};
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

pub struct MemoryManager {
    buffer: Arc<Mutex<Vec<String>>>,
    last_event_time: Arc<Mutex<Instant>>,
    table: Arc<Mutex<Option<Table>>>,
    api_client: Arc<ApiClient>,
    max_memory_results: usize,
    idle_timeout: Duration,
    total_messages_added: Arc<Mutex<usize>>,
    embedding_dim: i32,
}

impl MemoryManager {
    pub async fn new(
        db_path: &str,
        collection_name: &str,
        api_client: ApiClient,
        embedding_dim: usize,
        max_memory_results: usize,
        idle_timeout_minutes: u64,
    ) -> Result<Self> {
        // 验证维度有效性
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
            max_memory_results,
            idle_timeout: Duration::from_secs(idle_timeout_minutes * 60),
            total_messages_added: Arc::new(Mutex::new(0)),
            embedding_dim,
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

    pub async fn store_summary(&self, db: &Connection, collection_name: &str, summary: &str) -> Result<()> {
        let vector = self.api_client.get_embedding(summary).await?;
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        // 使用 UUID 字符串作为主键，避免整数 ID 碰撞风险
        let id_str = uuid::Uuid::new_v4().to_string();

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("text", DataType::Utf8, false),
            Field::new("role", DataType::Utf8, false),
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
                Arc::new(StringArray::from(vec![summary])),
                Arc::new(StringArray::from(vec!["system_summary"])),
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

    pub async fn search_relevant(&self, query: &str) -> Result<Vec<serde_json::Value>> {
        let query_vector = self.api_client.get_embedding(query).await?;
        
        let table_guard = self.table.lock().await;
        if let Some(table) = table_guard.as_ref() {
            let batches = table
                .query()
                .nearest_to(query_vector.as_slice())?
                .limit(self.max_memory_results)
                .execute()
                .await?
                .try_collect::<Vec<_>>()
                .await?;

            let mut memories = Vec::new();
            for batch in batches {
                if let Some(text_col) = batch.column_by_name("text") {
                    if let Some(timestamp_col) = batch.column_by_name("timestamp") {
                        let texts = text_col.as_any().downcast_ref::<StringArray>().unwrap();
                        let timestamps = timestamp_col.as_any().downcast_ref::<StringArray>().unwrap();
                        
                        for i in 0..texts.len() {
                            memories.push(serde_json::json!({
                                "text": texts.value(i),
                                "timestamp": timestamps.value(i)
                            }));
                        }
                    }
                }
            }
            
            return Ok(memories);
        }

        Ok(Vec::new())
    }

    pub async fn summarize_and_store(&self, db: &Connection, collection_name: &str, text: &str) -> Result<()> {
        let summary = self.api_client.summarize(text).await?;
        info!("Generated Summary: {}", summary);
        self.store_summary(db, collection_name, &summary).await?;
        info!("Engram successfully stored.");
        Ok(())
    }
}