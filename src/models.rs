/// 模型数据（与上游 WorkBuddy 5.3.11 保持一致的快照）
///
/// 来源：本机 WorkBuddy 日志
///   - main.log  `[buildResolvedProductConfig]`（基础配置 44 个模型 ID）
///   - renderer.log `declaredModels=[...]`（CLI 界面可选模型，如 glm-5.2 / hy3 / minimax-m3 / kimi-k3-1 等）
/// 已剔除上游不存在的模型（Claude 全系、Gemini 全系、-ioa 后缀旧名等），
/// 仅保留上游真实 ID + pi 正在使用的兼容别名（kimi-k3 → kimi-k3-1）。

/// Cursor 模型名 → WorkBuddy 模型 ID
/// 仅保留：上游真实存在的自身映射 + pi 兼容别名
pub const CURSOR_TO_WB_MAP: &[(&str, &str)] = &[
    // Kimi
    ("kimi-k3", "kimi-k3-1"),
    ("kimi-k3-1", "kimi-k3-1"),
    ("kimi-k2.5", "kimi-k2.5"),
    // DeepSeek
    ("deepseek-v4-pro", "deepseek-v4-pro"),
    ("deepseek-v4-flash", "deepseek-v4-flash"),
    // Hunyuan
    ("hy3", "hy3"),
    // GLM
    ("glm-5.1", "glm-5.1"),
    ("glm-5.2", "glm-5.2"),
];

/// WB 模型 ID → 首选 Cursor 别名（用于 /v1/models，与上游一致）
pub const WB_TO_CURSOR_MAP: &[(&str, &str)] = &[
    ("kimi-k3-1", "kimi-k3-1"),
    ("kimi-k2.5", "kimi-k2.5"),
    ("deepseek-v4-pro", "deepseek-v4-pro"),
    ("deepseek-v4-flash", "deepseek-v4-flash"),
    ("hy3", "hy3"),
    ("glm-5.1", "glm-5.1"),
    ("glm-5.2", "glm-5.2"),
];

/// 推理模型（超时用 300s）
pub const REASONING_MODELS: &[&str] = &[
    "deepseek-r1-0528-lkeap",
    "deepseek-v4-pro",
    "deepseek-v4-flash",
    "glm-5.2",
];

/// 可用模型列表（仅上游真实存在的 ID，与 WorkBuddy 保持一致）
pub const MODELS: &[(&str, &str)] = &[
    // DeepSeek
    ("deepseek-v4-pro", "DeepSeek-V4-Pro"),
    ("deepseek-v4-flash", "DeepSeek-V4-Flash"),
    ("deepseek-v3-1", "DeepSeek-V3.1"),
    ("deepseek-v3-1-volc", "DeepSeek-V3-1-Terminus"),
    ("deepseek-v3-0324", "DeepSeek-V3-0324"),
    ("deepseek-v3-0324-lkeap", "DeepSeek-V3-0324-LKEAP"),
    ("deepseek-r1-0528-lkeap", "DeepSeek-R1-0528-LKEAP"),
    // GLM
    ("glm-5.2", "GLM-5.2"),
    ("glm-5.1", "GLM-5.1"),
    ("glm-4.7", "GLM-4.7"),
    ("glm-4.6", "GLM-4.6"),
    ("glm-5v-turbo", "GLM-5v-Turbo"),
    // Hunyuan
    ("hy3", "Hunyuan-3 (Hy3)"),
    ("hunyuan-2.0-instruct", "Hunyuan-2.0-Instruct"),
    // Kimi
    ("kimi-k3-1", "Kimi-K3.1"),
    ("kimi-k2.7", "Kimi-K2.7"),
    ("kimi-k2.6", "Kimi-K2.6"),
    ("kimi-k2.5", "Kimi-K2.5"),
    // MiniMax
    ("minimax-m3", "MiniMax-M3"),
];

/// 与 Python `resolve_model()` 一致：无映射则透传
pub fn resolve_model(model: &str) -> String {
    for (cursor, wb) in CURSOR_TO_WB_MAP {
        if *cursor == model {
            return wb.to_string();
        }
    }
    model.to_string()
}

/// 与 Python `_timeout_for()` 一致
pub fn timeout_for(model: &str, default: u64, reasoning: u64) -> u64 {
    if REASONING_MODELS.contains(&model) {
        reasoning
    } else {
        default
    }
}

/// 构建 /v1/models 响应（与 Python 版顺序一致：先 Cursor 别名，再原始 WB 模型）
pub fn build_models_response() -> serde_json::Value {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut data: Vec<serde_json::Value> = Vec::new();

    // First: Cursor 兼容别名
    for (cursor_name, wb_id) in CURSOR_TO_WB_MAP {
        if seen.insert(cursor_name.to_string()) {
            // 从 MODELS 找显示名
            let display_name = MODELS
                .iter()
                .find(|(id, _)| id == wb_id)
                .map(|(_, name)| name.to_string())
                .unwrap_or_else(|| cursor_name.to_string());
            data.push(serde_json::json!({
                "id": cursor_name,
                "object": "model",
                "created": 1700000000,
                "owned_by": "workbuddy",
                "name": format!("{} (Cursor)", display_name),
            }));
        }
    }

    // Then: 原始 WB 模型
    for (id, name) in MODELS {
        if seen.insert(id.to_string()) {
            data.push(serde_json::json!({
                "id": id,
                "object": "model",
                "created": 1700000000,
                "owned_by": "workbuddy",
                "name": name,
            }));
        }
    }

    serde_json::json!({ "object": "list", "data": data })
}
