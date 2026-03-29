# 待办事项功能集成计划

## 状态：暂缓集成

`check_pending_tasks` 方法已实现，但暂未集成到主线流程。

## 功能描述

**方法签名：**
```rust
pub async fn check_pending_tasks(&self) -> Result<Vec<serde_json::Value>>
```

**查询逻辑：**
- 纯标量查询，不需要 embedding
- 过滤条件：`memory_type = 'task' AND status = 'in_progress'`
- 返回最多 5 条进行中的任务

**预期用途：**
Agent 启动时主动查询待办事项，无需用户提问，自动提醒当前任务状态。

## 集成前置条件

### 1. MCP 工具：更新待办事项状态

需要开发 MCP 工具，允许 Agent 主动更新任务状态：

**工具设计：**
```json
{
  "name": "update_task_status",
  "description": "更新记忆中的任务状态",
  "parameters": {
    "task_summary": "任务摘要文本",
    "new_status": "in_progress | done | cancelled"
  }
}
```

**Rust 实现：**
```rust
pub async fn update_task_status(
    &self,
    task_summary: &str,
    new_status: &str,
) -> Result<()> {
    // 1. 查找匹配的任务记忆
    // 2. 更新 status 字段
    // 3. 记录操作日志
}
```

### 2. 任务状态更新触发机制

**原本想法：**
LLM 总结时发现任务完成 → 自动标记旧任务为 done

**核心问题：如何匹配到具体的待办事项？**

**匹配难点：**
- 文本不完全匹配："正在写毕业论文第二章" vs "第二章写完了"
- 语义相似但表述不同
- 旧记忆可能有多条相似任务

**方案对比：**

| 方案 | 实现方式 | 优点 | 缺点 |
|------|---------|------|------|
| A. 向量相似度匹配 | 新任务 status=done 时，搜索相似的 in_progress 任务，相似度>0.8 则更新 | 自动化程度高 | 可能误匹配，阈值难定 |
| B. task_id 标识符 | LLM 提取任务时生成唯一 task_id，状态更新时匹配 ID | 精准匹配 | LLM 需要记住/查询旧 ID |
| C. LLM 双阶段判断 | 先提取新记忆，再让 LLM 判断是否为状态更新并指出旧任务摘要 | 智能，理解上下文 | 多一次 API 调用，成本增加 |
| D. 人工确认 | 检测到可能的完成状态，提示用户确认更新哪条任务 | 最准确 | 需要用户介入 |

**推荐：方案 A + D 混合**

```rust
pub async fn detect_task_completion(&self, new_memory: &MemoryExtraction) -> Result<Option<String>> {
    // 1. 判断是否为任务完成
    if new_memory.memory_type != MemoryType::Task || new_memory.status != Some(TaskStatus::Done) {
        return Ok(None);
    }
    
    // 2. 向量搜索相似进行中任务
    let vector = self.api_client.get_embedding(&new_memory.summary).await?;
    let similar_tasks = self.table
        .query()
        .only_if("memory_type = 'task' AND status = 'in_progress'")
        .nearest_to(vector.as_slice())?
        .limit(3)
        .execute()
        .await?;
    
    // 3. 返回最相似任务供确认（相似度 > 0.75）
    if let Some(task) = similar_tasks.first() {
        let similarity = 1.0 - task.distance;
        if similarity > 0.75 {
            return Ok(Some(task.summary)); // 返回旧任务摘要，待用户/LLM确认
        }
    }
    
    Ok(None)
}
```

**触发时机：**
在 `summarize_and_store` 流程中：
```
新记忆生成 → 检测是否为任务完成 → 向量匹配旧任务 → 提示确认 → 更新状态
```

## 集成步骤

**第一阶段：MCP 工具开发**
1. 实现 `update_task_status` 方法
2. 注册为 MCP 工具
3. 测试工具调用流程

**第二阶段：主线集成**
1. 在 `main.rs` 初始化阶段调用 `check_pending_tasks`
2. 将待办事项注入 System Prompt
3. 日志记录任务提醒

**第三阶段：自动化**
1. 探索自动状态更新机制
2. 实现任务过期检测
3. 完善任务生命周期管理

## 当前代码状态

- `check_pending_tasks` 方法已实现（src/memory.rs:315）
- 添加了 `#[allow(dead_code)]` 消除警告
- 等待 MCP 工具完成后再集成

## 相关文件

- 实现代码：`src/memory.rs:315-354`
- 配置示例：`config.yaml.exp`（memory 配置段）
- 原始设计：`plans/hybrid-search-implementation-plan.md`（玩法三）