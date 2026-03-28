# Rust 重写方案 - pico-mem

## 一、项目概述

将 Python 参考代码 `ref/main.py` 重写为 Rust 实现，保持核心功能：
- JSON-RPC Hook 协议通信
- LanceDB 向量数据库存储记忆
- LLM API 调用（embedding + summarization）
- 后台闲置总结线程

## 二、核心架构对比

### Python vs Rust

| 方面 | Python | Rust |
|------|---------|------|
| 向量数据库 | Milvus Lite | LanceDB 0.27+ |
| 异步框架 | threading | tokio async/await |
| HTTP Client | requests | reqwest (async) |
| 日志系统 | logging | tracing + tracing-appender |
| 错误处理 | try/except | Result<T, anyhow::Error> |
| 并发安全 | threading.Lock | Arc<Mutex<T>> |

### 主要差异

1. **Milvus Lite → LanceDB**
   - Milvus Lite 使用简单的 int64 ID 和字段 schema
   - LanceDB 使用 Arrow RecordBatch，需要 FixedSizeListArray 存储 vector
   - LanceDB API: `create_table(name, initial_data)` 直接接受 RecordBatch
   - 搜索: `nearest_to(slice)` 接受 slice 而非引用

2. **JSON-RPC 协议处理**
   - Python: 动态解析 JSON，直接修改 dict
   - Rust: serde 反序列化，需要处理借用和所有权
   - **关键点**: `before_llm` 中修改 messages 时需要先提取 content，再修改 msg

## 三、文件结构

```
src/
├── main.rs       # 主循环、Handler、Watchdog
├── config.rs     # 配置管理
├── rpc.rs        # JSON-RPC Request/Response 类型
├── api.rs        # HTTP Client (embedding + summarization)
└── memory.rs     # LanceDB 存储管理
```

## 四、关键技术点

### 1. LanceDB Schema 设计

```rust
Schema::new(vec![
    Field::new("id", DataType::Int64, false),
    Field::new("text", DataType::Utf8, false),
    Field::new("role", DataType::Utf8, false),
    Field::new("timestamp", DataType::Utf8, false),
    Field::new(
        "vector",
        DataType::FixedSizeList(
            Arc::new(Field::new("item", DataType::Float32, true)),
            1024,  // embedding dimension
        ),
        false,
    ),
])
```

**注意**: 
- Python 使用 `int64` ID (UUID 转整数)
- LanceDB 需要 FixedSizeListArray 包装 vector
- 创建表时需要提供 initial_data (RecordBatch)

### 2. JSON-RPC 关键处理

#### before_llm - 修改 messages

```rust
// ❌ 错误做法 - 借用冲突
let user_content = msg.get("content").unwrap_or("");  // immutable borrow
msg.as_object_mut().insert("content", ...);             // mutable borrow!

// ✅ 正确做法 - 先提取，再修改
let user_content = msg.get("content").unwrap_or("").to_string();
// ... search_relevant ...
if let Some(msg_obj) = msg.as_object_mut() {
    msg_obj.insert("content".to_string(), json!(format!("{}{}", user_content, inject_str)));
}
```

#### after_llm - 空响应

```rust
// Python: _respond(req_id, result={})
// Rust: 写入空字典，不返回 response 对象
write_response(&Response::success(id, json!({})))?;
```

**原因**: Hook 协议期望空 result，不覆盖实际 LLM response。

### 3. 异步初始化

```rust
// 后台初始化 LanceDB，不阻塞主进程握手
tokio::spawn(async move {
    let manager = MemoryManager::new(...).await?;
    *manager_clone.lock().await = Some(manager);
});

// 主循环立即开始，处理 hello 请求
while let Some(request) = read_request()? {
    // ...
}
```

### 4. Watchdog 线程

```rust
loop {
    tokio::time::sleep(Duration::from_secs(10)).await;
    
    if manager.should_summarize().await {
        let text = manager.get_and_clear_buffer().await;
        manager.summarize_and_store(&db, &collection_name, &text).await?;
    }
}
```

**改进**: 将 `summarize` 和 `store` 合并为一个方法，避免暴露 api_client。

## 五、JSON 处理注意事项

### Python JSON vs Rust serde_json

| Python | Rust | 说明 |
|--------|------|------|
| `msg["content"]` | `msg.get("content")` | Rust 需要处理 Option |
| `msg["content"] = value` | `msg.as_object_mut().insert(...)` | 需要转换为 Object |
| `params.get("key", default)` | `params.get("key").and_then(...).unwrap_or(default)` | 需要链式调用 |

### 关键 JSON-RPC 格式

```json
// Request
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "hook.before_llm",
  "params": {
    "messages": [{"role": "user", "content": "..."}]
  }
}

// Response - before_llm (modify)
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "action": "modify",
    "request": { /* modified params */ }
  }
}

// Response - after_llm (empty)
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {}
}
```

**⚠️ 必须严格遵守**:
- `before_llm` 返回 `{"action": "modify"}` 或 `{"action": "continue"}`
- `after_llm` 返回空 `{}`，不能返回 `{"response": {...}`
- 错误返回 `{"error": {"code": -32603, "message": "..."}}`

## 六、依赖版本

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
reqwest = { version = "0.12", features = ["json"] }
lancedb = "0.27"
arrow-array = "58"
arrow-schema = "58"
futures = "0.3"
uuid = { version = "1", features = ["v4"] }
tracing = "0.1"
tracing-subscriber = "0.3"
tracing-appender = "0.2"
chrono = "0.4"
anyhow = "1"
thiserror = "2"
```

**注意**: 
- `lancedb 0.27` 使用 Arrow 58.x
- 需要 `futures` 提供 `TryStreamExt`
- `protoc` 必须安装（系统或下载）

## 七、编译与运行

### 安装 protoc

```bash
# Ubuntu/Debian
sudo apt-get install protobuf-compiler

# 手动安装
wget https://github.com/protocolbuffers/protobuf/releases/download/v25.1/protoc-25.1-linux-x86_64.zip
unzip protoc-25.1-linux-x86_64.zip -d /usr/local
export PATH=/usr/local/bin:$PATH
```

### 编译

```bash
cargo build --release
```

### 运行

```bash
# 设置环境变量
export SF_API_KEY="your_key"
export SF_BASE_URL="https://api.siliconflow.cn/v1"
export ENGRAM_DB_PATH="/path/to/db"
export PICOCLAW_HOOK_LOG_FILE="/tmp/picoclaw-hook.log"

# 运行
./target/release/pico-mem
```

### 测试 JSON-RPC

```bash
# 发送 hello
echo '{"jsonrpc":"2.0","id":1,"method":"hook.hello","params":{}}' | ./target/release/pico-mem

# 预期输出
{"jsonrpc":"2.0","id":1,"result":{"name":"cloud_engram_gate","protocol_version":1}}
```

## 八、调试要点

### 日志级别

```bash
export RUST_LOG=info
# 或
export RUST_LOG=debug  # 详细调试
```

### 常见错误

1. **LanceDB 连接失败**
   - 检查路径权限
   - 确保目录存在（代码会自动创建）

2. **JSON-RPC 格式错误**
   - 必须是单行 JSON，末尾 `\n`
   - `id` 必须是 `Option<Value>` 类型

3. **借用检查错误**
   - 修改 JSON 前先提取值：`to_string()`
   - 使用 `clone()` 避免所有权转移

4. **异步阻塞**
   - 使用 `Arc<Mutex<T>>` 跨线程共享
   - 后台初始化用 `tokio::spawn`

## 九、未完成事项

当前代码需要：
1. 修复 Arrow schema 与 LanceDB 的类型匹配
2. 完善 RecordBatch 创建逻辑
3. 测试实际 JSON-RPC 通信
4. 验证 memory 注入格式与 Python 一致

## 十、总结

Rust 重写方案已搭建核心框架，关键技术点已识别：
- LanceDB API 适配（FixedSizeListArray, nearest_to slice）
- JSON-RPC 严格格式处理（before_llm modify, after_llm empty）
- Rust 所有权与借用处理（先提取再修改）
- 异步并发模式（tokio spawn + Arc<Mutex>）

下一步需要完成编译调试和实际测试验证。