# 配置文档

## 配置文件说明

pico-mem 使用 YAML 格式的配置文件，通过 `--config` 参数指定配置文件路径。

### 配置文件结构

配置文件包含以下五个部分：

```yaml
llm:           # LLM 配置
embedding:     # Embedding 配置
database:      # 数据库配置
memory:        # 记忆管理配置
logging:       # 日志配置
```

### 详细配置项

#### 1. LLM 配置 (llm)

| 配置项 | 类型 | 说明 | 默认值 |
|--------|------|------|--------|
| `api_key` | string | LLM API 密钥 | `"your_llm_api_key_here"` |
| `base_url` | string | LLM API 基础 URL | `"https://api.siliconflow.cn/v1"` |
| `model` | string | LLM 模型名称 | `"Pro/deepseek-ai/DeepSeek-V3.2"` |

#### 2. Embedding 配置 (embedding)

| 配置项 | 类型 | 说明 | 默认值 |
|--------|------|------|--------|
| `api_key` | string | Embedding API 密钥 | `"your_embedding_api_key_here"` |
| `base_url` | string | Embedding API 基础 URL | `"https://api.siliconflow.cn/v1"` |
| `model` | string | Embedding 模型名称 | `"BAAI/bge-m3"` |
| `embedding_dim` | integer | 向量维度 | `1024` |

#### 3. 数据库配置 (database)

| 配置项 | 类型 | 说明 | 默认值 |
|--------|------|------|--------|
| `db_path` | string | LanceDB 数据库路径，支持 `~` 展开 | `"~/.pico-mem/engram_memory.db"` |
| `collection_name` | string | 数据库表名 | `"engrams"` |

#### 4. 记忆管理配置 (memory)

| 配置项 | 类型 | 说明 | 默认值 |
|--------|------|------|--------|
| `max_memory_results` | integer | 检索返回的最大记忆数量 | `3` |
| `idle_timeout_minutes` | integer | 空闲超时时间（分钟），触发自动总结 | `3` |

#### 5. 日志配置 (logging)

| 配置项 | 类型 | 说明 | 默认值 |
|--------|------|------|--------|
| `log_file` | string | 日志文件路径，支持 `~` 展开 | `"~/.pico-mem/picoclaw-hook-cloud.log"` |

## 使用方法

### 启动命令

```bash
# 使用默认配置文件
pico-mem --config config.yaml

# 使用自定义配置文件
pico-mem --config /path/to/your/config.yaml

# 查看帮助
pico-mem --help
```

### 配置示例

#### 基础配置 (config.yaml)

```yaml
llm:
  api_key: "your_llm_api_key_here"
  base_url: "https://api.siliconflow.cn/v1"
  model: "Pro/deepseek-ai/DeepSeek-V3.2"

embedding:
  api_key: "your_embedding_api_key_here"
  base_url: "https://api.siliconflow.cn/v1"
  model: "BAAI/bge-m3"
  embedding_dim: 1024

database:
  db_path: "~/.pico-mem/engram_memory.db"
  collection_name: "engrams"

memory:
  max_memory_results: 3
  idle_timeout_minutes: 3

logging:
  log_file: "/tmp/picoclaw-hook-cloud.log"
```

#### 生产环境配置示例

```yaml
llm:
  api_key: "sk-xxxxxxxxxxxxxxxx"  # 替换为真实 LLM API 密钥
  base_url: "https://api.siliconflow.cn/v1"
  model: "Pro/deepseek-ai/DeepSeek-V3.2"

embedding:
  api_key: "sk-yyyyyyyyyyyyyyyy"  # 替换为真实 Embedding API 密钥
  base_url: "https://api.siliconflow.cn/v1"
  model: "BAAI/bge-m3"
  embedding_dim: 1024

database:
  db_path: "/var/lib/pico-mem/engram_memory.db"
  collection_name: "engrams"

memory:
  max_memory_results: 5
  idle_timeout_minutes: 5

logging:
  log_file: "/var/log/pico-mem/hook.log"
```

#### 多供应商配置示例

LLM 和 Embedding 可以使用不同的供应商：

```yaml
llm:
  api_key: "sk-openai-key"
  base_url: "https://api.openai.com/v1"
  model: "gpt-4"

embedding:
  api_key: "sk-siliconflow-key"
  base_url: "https://api.siliconflow.cn/v1"
  model: "BAAI/bge-m3"
  embedding_dim: 1024

database:
  db_path: "~/.pico-mem/engram_memory.db"
  collection_name: "engrams"

memory:
  max_memory_results: 3
  idle_timeout_minutes: 3

logging:
  log_file: "/tmp/picoclaw-hook-cloud.log"
```

#### 开发环境配置示例

```yaml
llm:
  api_key: "sk-test-key"
  base_url: "https://api.siliconflow.cn/v1"
  model: "Pro/deepseek-ai/DeepSeek-V3.2"

embedding:
  api_key: "sk-test-key"
  base_url: "https://api.siliconflow.cn/v1"
  model: "BAAI/bge-m3"
  embedding_dim: 1024

database:
  db_path: "./test_db/engram_memory.db"
  collection_name: "test_engrams"

memory:
  max_memory_results: 10
  idle_timeout_minutes: 1

logging:
  log_file: "./logs/dev.log"
```

## 特殊说明

### 路径展开

配置文件中的 `db_path` 和 `log_file` 支持 `~` 符号展开：
- `~` 会自动展开为用户的 HOME 目录
- 例如：`~/.pico-mem/db.db` → `/home/username/.pico-mem/db.db`

### 向量维度

`embedding_dim` 必须与 `embedding.model` 的输出维度匹配：
- `BAAI/bge-m3`: 1024维
- 其他模型请查阅对应文档

### 多供应商支持

LLM 和 Embedding 可以配置不同的供应商：
- 可以使用 OpenAI 的 GPT-4 作为 LLM，同时使用 SiliconFlow 的 BGE-M3 作为 Embedding
- 只需分别配置 `llm` 和 `embedding` 部分的 `api_key`、`base_url` 和 `model` 即可

### 空闲超时机制

系统会在以下条件下自动触发记忆总结：
1. Buffer 中有未处理的对话内容
2. 自最后一次事件后超过 `idle_timeout_minutes` 分钟

### 环境变量覆盖

虽然已迁移到 YAML 配置，但仍支持通过环境变量覆盖部分配置：
- `RUST_LOG`: 控制 tracing 日志级别（例如：`RUST_LOG=debug`）

## 迁移指南

### 从 Python 版本迁移

Python 版本使用环境变量配置，迁移步骤：

1. 创建 `config.yaml` 文件
2. 将环境变量值转换为 YAML 格式：
   ```bash
   # Python 环境变量 → YAML 配置映射
   SF_API_KEY → llm.api_key 和 embedding.api_key
   SF_BASE_URL → llm.base_url 和 embedding.base_url
   ENGRAM_DB_PATH → database.db_path
   PICOCLAW_MAX_MEMORY → memory.max_memory_results
   PICOCLAW_HOOK_LOG_FILE → logging.log_file
   ```
3. 使用新启动方式：
   ```bash
   # Python 版本
   python ref/main.py
   
   # Rust 版本
   pico-mem --config config.yaml
   ```

### 从旧版本 YAML 配置迁移

旧版本使用单一的 `api` 配置块，迁移步骤：

1. 将 `api.api_key` 拆分为 `llm.api_key` 和 `embedding.api_key`
2. 将 `api.base_url` 拆分为 `llm.base_url` 和 `embedding.base_url`
3. 将 `api.llm_model` 改为 `llm.model`
4. 将 `api.embedding_model` 改为 `embedding.model`
5. 将 `api.embedding_dim` 改为 `embedding.embedding_dim`

旧配置格式：
```yaml
api:
  api_key: "your_api_key_here"
  base_url: "https://api.siliconflow.cn/v1"
  embedding_model: "BAAI/bge-m3"
  llm_model: "Pro/deepseek-ai/DeepSeek-V3.2"
  embedding_dim: 1024
```

新配置格式：
```yaml
llm:
  api_key: "your_llm_api_key_here"
  base_url: "https://api.siliconflow.cn/v1"
  model: "Pro/deepseek-ai/DeepSeek-V3.2"

embedding:
  api_key: "your_embedding_api_key_here"
  base_url: "https://api.siliconflow.cn/v1"
  model: "BAAI/bge-m3"
  embedding_dim: 1024
```

## 配置验证

启动时会在日志中输出配置文件路径：
```
[INFO] Loaded config from: config.yaml
```

如果配置文件不存在或格式错误，程序会立即报错退出。

## 常见问题

### Q: 如何修改 API 密钥？
A: 编辑配置文件中的 `llm.api_key` 和 `embedding.api_key` 字段，重启程序即可生效。

### Q: 如何使用不同的 LLM 和 Embedding 供应商？
A: 分别配置 `llm` 和 `embedding` 部分的 `api_key`、`base_url` 和 `model` 即可使用不同供应商。

### Q: 如何更改数据库存储位置？
A: 修改 `database.db_path`，支持绝对路径和相对路径。

### Q: 如何调整检索记忆数量？
A: 修改 `memory.max_memory_results`，建议值范围：1-10。

### Q: 如何禁用自动总结？
A: 将 `memory.idle_timeout_minutes` 设置为极大值（如 9999）。