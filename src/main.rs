mod api;
mod config;
mod memory;
mod rpc;
mod schema;

use anyhow::Result;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;
use clap::Parser;
use lancedb::connect;
use rpc::*;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{error, info};

use crate::api::ApiClient;
use crate::config::Config;
use crate::memory::MemoryManager;

#[derive(Parser, Debug)]
#[command(name = "pico-mem")]
#[command(about = "PicoClaw hook with Cloud LLM/Embedding & Idle Memory Summarization")]
struct Args {
    #[arg(short, long, value_name = "FILE")]
    config: PathBuf,
}

async fn handle_hello(id: Option<serde_json::Value>) -> Result<()> {
    let response = Response::success(
        id,
        serde_json::json!({
            "name": "cloud_engram_gate",
            "protocol_version": 1
        }),
    );
    write_response(&response)?;
    Ok(())
}

async fn handle_event(
    id: Option<serde_json::Value>,
    params: serde_json::Value,
    memory_manager: &Arc<Mutex<Option<MemoryManager>>>,
) -> Result<()> {
    let event_type = params.get("type").and_then(|t| t.as_str()).unwrap_or("");
    let payload = params.get("payload").cloned().unwrap_or(serde_json::Value::Null);

    if let Some(manager) = memory_manager.lock().await.as_ref() {
        match event_type {
            "turn_start" => {
                if let Some(content) = payload.get("user_message").and_then(|c| c.as_str()) {
                    if !content.is_empty() {
                        manager.add_message("user_message", content).await;
                        info!("Captured user message: {}...", &content.chars().take(50).collect::<String>());
                    }
                }
            }
            "llm_response" => {
                if let Some(content_len) = payload.get("content_len").and_then(|c| c.as_u64()) {
                    info!("LLM response received, content_len={}", content_len);
                }
            }
            "tool_exec_end" => {
                let tool = payload.get("tool").and_then(|t| t.as_str()).unwrap_or("unknown");
                let duration = payload.get("duration_ms").and_then(|d| d.as_u64()).unwrap_or(0);
                info!("Tool executed: {}, duration={}ms", tool, duration);
            }
            _ => {}
        }
    }

    write_response(&Response::success(id, serde_json::json!({})))?;
    Ok(())
}

async fn handle_before_llm(
    id: Option<serde_json::Value>,
    params: serde_json::Value,
    memory_manager: &Arc<Mutex<Option<MemoryManager>>>,
) -> Result<()> {
    let manager_guard = memory_manager.lock().await;
    if manager_guard.is_none() {
        write_response(&Response::success(id, serde_json::json!({
            "action": "continue",
            "request": params
        })))?;
        return Ok(());
    }
    drop(manager_guard);

    let messages = params.get("messages").and_then(|m| m.as_array()).cloned();
    if messages.is_none() {
        write_response(&Response::success(id, serde_json::json!({
            "action": "continue",
            "request": params
        })))?;
        return Ok(());
    }

    let mut messages = messages.unwrap();
    let mut modified = false;
    let mut params = params;

    if let Some(manager) = memory_manager.lock().await.as_ref() {
        for msg in messages.iter_mut().rev() {
            if msg.get("role").and_then(|r| r.as_str()) == Some("user") {
                let user_content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("").to_string();
                if user_content.is_empty() {
                    break;
                }

                info!(
                    "[Before LLM] User message captured: {}{}",
                    user_content.chars().take(200).collect::<String>(),
                    if user_content.len() > 200 { "..." } else { "" }
                );

                match manager.search_with_rerank(&user_content, 20).await {
                    Ok(relevant_memories) => {
                        if !relevant_memories.is_empty() {
                            info!("[Before LLM] Found {} relevant memories (after rerank):", relevant_memories.len());
                            for (i, mem) in relevant_memories.iter().enumerate() {
                                let summary = mem.get("summary").and_then(|t| t.as_str()).unwrap_or("");
                                let timestamp = mem.get("timestamp").and_then(|t| t.as_str()).unwrap_or("unknown");
                                let domain = mem.get("domain").and_then(|d| d.as_str()).unwrap_or("unknown");
                                let importance = mem.get("importance").and_then(|v| v.as_u64()).unwrap_or(0);
                                let score = mem.get("score").and_then(|s| s.as_f64()).unwrap_or(0.0);
                                info!(
                                    "  [Memory {}] score={:.3}, importance={}, timestamp={}, domain={}: {}{}",
                                    i + 1,
                                    score,
                                    importance,
                                    timestamp,
                                    domain,
                                    summary.chars().take(100).collect::<String>(),
                                    if summary.len() > 100 { "..." } else { "" }
                                );
                            }

                            let mut memory_lines = vec!["\n\n<memory_context>".to_string()];
                            for mem in &relevant_memories {
                                let timestamp = mem.get("timestamp").and_then(|t| t.as_str()).unwrap_or("unknown");
                                let summary = mem.get("summary").and_then(|t| t.as_str()).unwrap_or("");
                                let domain = mem.get("domain").and_then(|d| d.as_str()).unwrap_or("unknown");
                                memory_lines.push(format!(r#"  <memory timestamp="{}" domain="{}">"#, timestamp, domain));
                                memory_lines.push(format!("    {}", summary));
                                memory_lines.push("  </memory>".to_string());
                            }
                            memory_lines.push("</memory_context>".to_string());

                            let inject_str = memory_lines.join("\n");
                            if let Some(msg_obj) = msg.as_object_mut() {
                                msg_obj.insert("content".to_string(), serde_json::json!(format!("{}{}", user_content, inject_str)));
                            }

                            info!("[Before LLM] Injected {} memory entries into user message", relevant_memories.len());
                            modified = true;
                        } else {
                            info!("[Before LLM] No relevant memories found");
                        }
                    }
                    Err(e) => {
                        error!("[Before LLM] Memory injection failed: {}", e);
                    }
                }
                break;
            }
        }
    }

    if let Some(params_obj) = params.as_object_mut() {
        params_obj.insert("messages".to_string(), serde_json::json!(messages));
    }

    if modified {
        write_response(&Response::success(id, serde_json::json!({
            "action": "modify",
            "request": params
        })))?;
    } else {
        write_response(&Response::success(id, serde_json::json!({
            "action": "continue",
            "request": params
        })))?;
    }

    Ok(())
}

async fn handle_after_llm(
    id: Option<serde_json::Value>,
    params: serde_json::Value,
    memory_manager: &Arc<Mutex<Option<MemoryManager>>>,
) -> Result<()> {
    let content = params
        .get("response")
        .and_then(|r| r.get("content"))
        .and_then(|c| c.as_str())
        .or_else(|| params.get("content").and_then(|c| c.as_str()))
        .unwrap_or("");

    if !content.is_empty() {
        if let Some(manager) = memory_manager.lock().await.as_ref() {
            manager.add_message("assistant_message", content).await;
            info!("Captured assistant message: {}...", &content.chars().take(50).collect::<String>());
        }
    }

    write_response(&Response::success(id, serde_json::json!({})))?;
    Ok(())
}

async fn run_memory_watchdog(
    memory_manager: Arc<Mutex<Option<MemoryManager>>>,
    db: lancedb::Connection,
    collection_name: String,
    _idle_timeout: Duration,
) {
    loop {
        tokio::time::sleep(Duration::from_secs(10)).await;

        let should_summarize = {
            if let Some(manager) = memory_manager.lock().await.as_ref() {
                manager.should_summarize().await
            } else {
                false
            }
        };

        if should_summarize {
            info!("Idle timeout reached. Triggering memory summarization...");

            let text_to_summarize = {
                let manager_guard = memory_manager.lock().await;
                if let Some(manager) = manager_guard.as_ref() {
                    manager.get_and_clear_buffer().await
                } else {
                    String::new()
                }
            };

            if !text_to_summarize.is_empty() {
                if let Some(manager) = memory_manager.lock().await.as_ref() {
                    match manager.summarize_and_store(&db, &collection_name, &text_to_summarize).await {
                        Ok(_) => {}
                        Err(e) => {
                            error!("Failed to process memory engram: {}", e);
                        }
                    }
                }
            }
        }
    }
}

async fn run_debug_logger(memory_manager: Arc<Mutex<Option<MemoryManager>>>) {
    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;

        if let Some(manager) = memory_manager.lock().await.as_ref() {
            match manager.get_debug_info().await {
                debug_info => {
                    info!(
                        "[Debug] Buffer size: {}, Total messages: {}, Idle: {}s, Threshold: {}s, Should summarize: {}",
                        debug_info.get("buffer_size").and_then(|v| v.as_u64()).unwrap_or(0),
                        debug_info.get("total_messages_added").and_then(|v| v.as_u64()).unwrap_or(0),
                        debug_info.get("last_event_time").and_then(|v| v.as_f64()).unwrap_or(0.0),
                        debug_info.get("idle_timeout_threshold").and_then(|v| v.as_u64()).unwrap_or(0),
                        debug_info.get("should_summarize").and_then(|v| v.as_bool()).unwrap_or(false)
                    );
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    
    let mut config = Config::from_yaml(&args.config)?;
    config.expand_paths();

    let log_file = config.logging.log_file.clone();
    
    // Create a file appender
    let file_appender = tracing_appender::rolling::never(
        std::path::Path::new(&log_file).parent().unwrap_or(std::path::Path::new("/tmp")),
        std::path::Path::new(&log_file).file_name().unwrap_or(std::ffi::OsStr::new("picoclaw-hook-cloud.log"))
    );
    
    // Create a non-blocking writer for the file
    let (non_blocking_file, _guard) = tracing_appender::non_blocking(file_appender);
    
    // Create filter
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));
    
    // Create stderr layer - must call with_writer before with_filter
    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_filter(filter.clone());
    
    // Create file layer - must call with_writer before with_filter
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking_file)
        .with_filter(filter);

    tracing_subscriber::registry()
        .with(stderr_layer)
        .with(file_layer)
        .init();

    info!("=== Cloud Engram Gate Starting ===");
    info!("Loaded config from: {}", args.config.display());

    let db_path = config.database.db_path.clone();
    let collection_name = config.database.collection_name.clone();
    let idle_timeout_minutes = config.memory.idle_timeout_minutes;

    let api_client = ApiClient::new(
        config.llm.clone(),
        config.embedding.clone(),
    );

    let memory_manager = Arc::new(Mutex::new(None as Option<MemoryManager>));
    let manager_clone = memory_manager.clone();
    let db = connect(&db_path).execute().await?;
    let init_collection_name = collection_name.clone();
    let init_config_embedding_dim = config.embedding.embedding_dim;
    let init_memory_config = config.memory.clone();

    tokio::spawn(async move {
        info!("Background: Initializing LanceDB at {}", db_path);
        match MemoryManager::new(
            &db_path,
            &init_collection_name,
            api_client,
            init_config_embedding_dim,
            &init_memory_config,
        )
        .await
        {
            Ok(manager) => {
                *manager_clone.lock().await = Some(manager);
                info!("MemoryManager initialized successfully in background!");
            }
            Err(e) => {
                error!("Failed to initialize LanceDB background: {}", e);
            }
        }
    });

    let watchdog_manager = memory_manager.clone();
    let watchdog_db = db.clone();
    let watchdog_collection = collection_name.clone();
    tokio::spawn(async move {
        run_memory_watchdog(
            watchdog_manager,
            watchdog_db,
            watchdog_collection,
            Duration::from_secs(idle_timeout_minutes * 60),
        )
        .await;
    });

    let debug_manager = memory_manager.clone();
    tokio::spawn(async move {
        run_debug_logger(debug_manager).await;
    });

    info!("=== Main IO Loop Started ===");

    while let Some(request) = read_request()? {
        let method = request.method.as_str();
        let id = request.id.clone();
        let params = request.params.clone();

        let result = match method {
            "hook.hello" => handle_hello(id.clone()).await,
            "hook.event" => handle_event(id.clone(), params, &memory_manager).await,
            "hook.before_llm" => handle_before_llm(id.clone(), params, &memory_manager).await,
            "hook.after_llm" => handle_after_llm(id.clone(), params, &memory_manager).await,
            _ => {
                info!("Ignored method: {}", method);
                // 必须传原来的 params，绝对不能用 json!({})
                write_response(&Response::success(id.clone(), params))?;
                Ok(())
            }
        };

        if let Err(e) = result {
            error!("Handler error for {}: {}", method, e);
            write_response(&Response::error(id, -32603, e.to_string()))?;
        }
    }

    info!("=== Main IO Loop Ended ===");
    Ok(())
}