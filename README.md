# Pico-Mem

一个基于 Rust 的智能记忆管理服务，为 PicoClaw 提供对话记忆的存储、检索和自动总结功能。

## 项目简介

Pico-Mem 是 PicoClaw 的 hook 服务，主要功能包括：

- **对话捕获**：自动捕获用户和助手的对话消息
- **智能记忆提取**：使用 LLM 从对话中提取多条用户偏好、事实信息和任务状态（支持一次对话提取多条独立记忆）
- **向量存储**：使用 LanceDB 进行高效的向量存储和检索
- **记忆注入**：在 LLM 请求前自动注入相关的历史记忆上下文
- **空闲总结**：在对话空闲时自动触发记忆总结
- **重排序机制**：结合相似度和重要性对记忆进行智能排序

## 安装与运行

### 编译

```bash
cargo build --release
```

### 配置 PicoClaw Hook

在 PicoClaw 的配置文件中添加以下 hook 配置：

```json
{
  "cloud_engram_gate": {
    "enabled": true,
    "priority": 100,
    "transport": "stdio",
    "command": [
      "/path/to/pico-mem/target/release/pico-mem",
      "--config",
      "/path/to/config.yaml"
    ],
    "observe": [
      "turn_start",
      "llm_response",
      "tool_exec_start",
      "tool_exec_end",
      "tool_exec_skipped"
    ],
    "intercept": [
      "before_llm"
    ]
  }
}
```

#### 配置字段说明

| 字段 | 说明 |
|------|------|
| `enabled` | 是否启用此 hook |
| `priority` | 优先级，数值越大优先级越高 |
| `transport` | 通信方式，使用 `stdio` 通过标准输入/输出通信 |
| `command` | 启动命令，第一个参数为可执行文件路径，后续为命令行参数 |
| `observe` | 观察的事件列表，不拦截仅记录 |
| `intercept` | 拦截的事件列表，可修改请求内容 |

#### 事件类型

| 事件 | 类型 | 说明 |
|------|------|------|
| `turn_start` | observe | 对话轮次开始，包含用户消息 |
| `llm_response` | observe | LLM 响应完成 |
| `tool_exec_start` | observe | 工具执行开始 |
| `tool_exec_end` | observe | 工具执行结束 |
| `tool_exec_skipped` | observe | 工具执行跳过 |
| `before_llm` | intercept | LLM 请求前，用于注入记忆上下文 |

## 配置文件说明

配置文件采用 YAML 格式，包含以下五个主要部分：

### 1. LLM 配置 (`llm`)

用于配置大语言模型 API，主要用于记忆提取和总结。

```yaml
llm:
  api_key: "your_llm_api_key_here"
  base_url: "https://api.siliconflow.cn/v1"
  model: "Pro/deepseek-ai/DeepSeek-V3.2"
  summarize_prompt: >
    你是一个高阶的 Agent 记忆提取器。请分析以下对话，提取出用户的偏好、事实或当前的任务状态。
    绝对不要输出任何无关的解释，你的输出必须严格遵守以下 JSON 格式：
    {SCHEMA_PLACEHOLDER}
    
    对话内容：
    {CHAT_HISTORY}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `api_key` | string | 是 | LLM API 的访问密钥 |
| `base_url` | string | 是 | LLM API 的基础 URL 地址 |
| `model` | string | 是 | 使用的模型名称，需支持 JSON 输出格式 |
| `summarize_prompt` | string | 是 | 记忆提取的提示词模板，支持两个占位符：<br>• `{SCHEMA_PLACEHOLDER}` - JSON 输出格式定义<br>• `{CHAT_HISTORY}` - 待总结的对话历史 |

### 2. Embedding 配置 (`embedding`)

用于配置文本向量化服务，将记忆内容转换为向量以支持语义搜索。

```yaml
embedding:
  api_key: "your_embedding_api_key_here"
  base_url: "https://api.siliconflow.cn/v1"
  model: "BAAI/bge-m3"
  embedding_dim: 1024
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `api_key` | string | 是 | Embedding API 的访问密钥 |
| `base_url` | string | 是 | Embedding API 的基础 URL 地址 |
| `model` | string | 是 | 使用的嵌入模型名称 |
| `embedding_dim` | integer | 是 | 嵌入向量的维度，需与所选模型匹配（如 bge-m3 为 1024） |

### 3. 数据库配置 (`database`)

用于配置 LanceDB 向量数据库的存储位置。

```yaml
database:
  db_path: "/home/harmoon/projects/pico-mem-rust/engram_memory_db"
  collection_name: "engrams"
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `db_path` | string | 是 | LanceDB 数据库存储路径，支持 `~` 表示用户主目录 |
| `collection_name` | string | 是 | 存储记忆的集合（表）名称 |

### 4. 记忆管理配置 (`memory`)

用于配置记忆检索和管理的核心参数。

```yaml
memory:
  max_memory_results: 3
  idle_timeout_minutes: 3
  overlap_threshold: 0.85
  enable_dedup: true
  similarity_weight: 0.6
  importance_weight: 0.4
  domains:
    - frontend_dev
    - backend_dev
    - daily_life
```

| 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| `max_memory_results` | integer | 是 | 3 | 每次检索返回的最大记忆数量，用于注入到 LLM 上下文 |
| `idle_timeout_minutes` | integer | 是 | 3 | 空闲超时时间（分钟），超过此时间无新消息则触发记忆总结 |
| `overlap_threshold` | float | 是 | 0.85 | 重叠阈值（0.0-1.0），用于判断新记忆是否与已有记忆重复 |
| `enable_dedup` | boolean | 是 | true | 是否启用记忆去重功能 |
| `domains` | string[] | 否 | `["frontend_dev", "backend_dev", "daily_life"]` | 领域标签列表，用于记忆分类。LLM 会从列表中选择最匹配的领域 |

#### 记忆去重机制说明

当 `enable_dedup` 设置为 `true` 时，系统在存储新记忆前会执行以下去重检查：

1. **向量相似度搜索**：使用新记忆的向量在数据库中搜索最相似的已有记忆
2. **相似度计算**：`相似度 = 1.0 - 向量距离`
3. **拒绝条件**：当同时满足以下两个条件时，新记忆将被拒绝存储：
   - 相似度 > `overlap_threshold`（语义高度重叠）
   - 新记忆的重要性 ≤ 已有记忆的重要性（新记忆价值不更高）

**去重的好处**：
- 避免存储语义重复的记忆，节省存储空间
- 提高检索效率，减少冗余结果
- 保留高重要性的记忆，不会被低重要性的重复内容覆盖

**示例场景**：
- 用户多次表达"我喜欢用 Python 编程"，系统只会保留第一次或重要性最高的记录
- 如果用户说"我非常喜欢 Python，它是我最爱的语言"（重要性更高），可能会替换之前的记录
| `similarity_weight` | float | 是 | 0.6 | 相似度权重，用于记忆重排序时的综合评分计算 |
| `importance_weight` | float | 是 | 0.4 | 重要性权重，用于记忆重排序时的综合评分计算 |

**权重说明**：
- `similarity_weight` + `importance_weight` 应等于 1.0
- 综合评分 = `similarity_weight × 相似度分数 + importance_weight × 重要性分数`
- 较高的 `similarity_weight` 会优先返回语义最相关的记忆
- 较高的 `importance_weight` 会优先返回标记为重要的记忆

### 5. 日志配置 (`logging`)

用于配置日志输出位置。

```yaml
logging:
  log_file: "/tmp/picoclaw-hook-cloud-rust.log"
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `log_file` | string | 是 | 日志文件路径，支持 `~` 表示用户主目录 |

## 完整配置示例

```yaml
llm:
  api_key: "your_llm_api_key_here"
  base_url: "https://api.siliconflow.cn/v1"
  model: "Pro/deepseek-ai/DeepSeek-V3.2"
  summarize_prompt: >
    你是一个高阶的 Agent 记忆提取器。请分析以下对话，提取出用户的偏好、事实或当前的任务状态。
    绝对不要输出任何无关的解释，你的输出必须严格遵守以下 JSON 格式：
    {SCHEMA_PLACEHOLDER}
    
    对话内容：
    {CHAT_HISTORY}

embedding:
  api_key: "your_embedding_api_key_here"
  base_url: "https://api.siliconflow.cn/v1"
  model: "BAAI/bge-m3"
  embedding_dim: 1024

database:
  db_path: "/home/user/pico-mem-rust/engram_memory_db"
  collection_name: "engrams"

memory:
  max_memory_results: 3
  idle_timeout_minutes: 3
  overlap_threshold: 0.85
  enable_dedup: true
  similarity_weight: 0.6
  importance_weight: 0.4
  domains:
    - frontend_dev
    - backend_dev
    - daily_life

logging:
  log_file: "/tmp/pico-mem.log"
```

## 工作流程

1. **初始化**：服务启动时加载配置，初始化 LanceDB 和 API 客户端
2. **消息捕获**：通过 `hook.event` 捕获用户消息和助手响应
3. **记忆检索**：在 `hook.before_llm` 时，根据用户消息检索相关记忆并注入上下文
4. **记忆存储**：在空闲超时后，使用 LLM 提取记忆并存储到向量数据库

## Hook 接口

服务通过标准输入/输出实现 JSON-RPC 风格的 hook 接口：

| 方法 | 说明 |
|------|------|
| `hook.hello` | 握手响应，返回服务名称和协议版本 |
| `hook.event` | 事件处理，捕获对话事件 |
| `hook.before_llm` | LLM 请求前处理，注入记忆上下文 |
| `hook.after_llm` | LLM 响应后处理，捕获助手消息 |

## 依赖

- **Tokio**：异步运行时
- **LanceDB**：向量数据库
- **Reqwest**：HTTP 客户端
- **Serde**：序列化框架
- **Tracing**：日志框架

## 许可证

MIT License