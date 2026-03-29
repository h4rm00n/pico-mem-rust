use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MemoryExtraction {
    pub summary: String,
    pub domain: String,
    pub memory_type: MemoryType,
    pub importance: u8,
    pub status: Option<TaskStatus>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    Preference,
    Fact,
    Task,
    #[serde(other)]
    Other,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    InProgress,
    Done,
    #[serde(other)]
    Other,
}

impl MemoryExtraction {
    pub fn schema_description() -> String {
        serde_json::to_string_pretty(&serde_json::json!({
            "summary": "核心事实或摘要字符串",
            "domain": "所属领域标签，如 frontend_dev, backend_dev, daily_life",
            "memory_type": "枚举: preference, fact, task",
            "importance": "1到10的整数，表示重要性",
            "status": "如果是任务，填写 in_progress 或 done，否则为 null"
        }))
        .unwrap()
    }
}
