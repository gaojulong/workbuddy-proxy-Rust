//! 模型映射单元测试（与上游 WorkBuddy 5.3.11 保持一致）

use workbuddy_proxy_rust::models::{
    build_models_response, resolve_model, timeout_for, CURSOR_TO_WB_MAP, MODELS,
    REASONING_MODELS,
};

#[test]
fn test_resolve_model_mapped() {
    // kimi-k3 → kimi-k3-1（兼容别名）
    assert_eq!(resolve_model("kimi-k3"), "kimi-k3-1");
    // 自身映射
    assert_eq!(resolve_model("deepseek-v4-pro"), "deepseek-v4-pro");
    assert_eq!(resolve_model("hy3"), "hy3");
    assert_eq!(resolve_model("glm-5.2"), "glm-5.2");
}

#[test]
fn test_resolve_model_passthrough() {
    // 无映射的模型原样透传（与 Python 一致）
    assert_eq!(resolve_model("deepseek-v3"), "deepseek-v3");
    assert_eq!(resolve_model("unknown-model"), "unknown-model");
    // 上游真实模型但不在映射表（如 minimax-m3）也应透传
    assert_eq!(resolve_model("minimax-m3"), "minimax-m3");
    assert_eq!(resolve_model("kimi-k2.7"), "kimi-k2.7");
}

#[test]
fn test_resolve_model_no_stale_aliases() {
    // 已删除的假名/过期模型：resolve 后不应映射到上游不存在的模型
    // 这些模型上游根本不存在，只会原样透传（而不是映射成错误目标）
    assert_eq!(resolve_model("claude-4.6-opus-high"), "claude-4.6-opus-high");
    assert_eq!(resolve_model("gemini-3.1-pro"), "gemini-3.1-pro");
    assert_eq!(resolve_model("glm-5.0-ioa"), "glm-5.0-ioa");
}

#[test]
fn test_timeout_for() {
    // 推理模型用 300s
    assert_eq!(timeout_for("deepseek-r1-0528-lkeap", 120, 300), 300);
    assert_eq!(timeout_for("deepseek-v4-pro", 120, 300), 300);
    assert_eq!(timeout_for("glm-5.2", 120, 300), 300);
    // 普通模型用 120s
    assert_eq!(timeout_for("deepseek-v3-1", 120, 300), 120);
    assert_eq!(timeout_for("hy3", 120, 300), 120);
    assert_eq!(timeout_for("kimi-k2.7", 120, 300), 120);
}

#[test]
fn test_reasoning_models_consistent() {
    for m in REASONING_MODELS {
        assert!(!m.is_empty());
    }
    assert!(REASONING_MODELS.contains(&"deepseek-r1-0528-lkeap"));
    assert!(REASONING_MODELS.contains(&"glm-5.2"));
    assert!(REASONING_MODELS.contains(&"deepseek-v4-pro"));
}

#[test]
fn test_models_count() {
    // 19 个 WB 模型 + 8 个 Cursor 别名去重 = 20
    let resp = build_models_response();
    let data = resp.get("data").unwrap().as_array().unwrap();
    assert_eq!(data.len(), 20, "模型总数应为 20，与上游一致");
}

#[test]
fn test_models_field_structure() {
    let resp = build_models_response();
    let data = resp.get("data").unwrap().as_array().unwrap();
    for m in data {
        assert!(m.get("id").is_some());
        assert!(m.get("object").is_some());
        assert!(m.get("created").is_some());
        assert!(m.get("owned_by").is_some());
        assert!(m.get("name").is_some());
    }
}

#[test]
fn test_models_no_duplicates() {
    let resp = build_models_response();
    let data = resp.get("data").unwrap().as_array().unwrap();
    let mut ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for m in data {
        let id = m.get("id").unwrap().as_str().unwrap().to_string();
        assert!(ids.insert(id), "重复模型 ID");
    }
}

#[test]
fn test_models_critical_present() {
    // 与上游 CLI 可选模型一致的核心集合
    let resp = build_models_response();
    let data = resp.get("data").unwrap().as_array().unwrap();
    let ids: std::collections::HashSet<&str> = data
        .iter()
        .filter_map(|m| m.get("id").and_then(|v| v.as_str()))
        .collect();
    for critical in [
        "deepseek-v4-pro",
        "deepseek-v4-flash",
        "hy3",
        "glm-5.2",
        "glm-5.1",
        "kimi-k3-1",
        "kimi-k2.7",
        "kimi-k2.6",
        "minimax-m3",
        "glm-5v-turbo",
    ] {
        assert!(ids.contains(critical), "缺少核心模型: {}", critical);
    }
}

#[test]
fn test_models_no_upstream_absent() {
    // 确保没有上游不存在的模型出现在列表里
    let resp = build_models_response();
    let data = resp.get("data").unwrap().as_array().unwrap();
    let ids: std::collections::HashSet<String> = data
        .iter()
        .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .collect();
    for absent in ["claude", "gemini", "-ioa"] {
        for id in &ids {
            assert!(
                !id.contains(absent),
                "上游不存在的模型不应出现: {} (含 {})",
                id,
                absent
            );
        }
    }
}

#[test]
fn test_cursor_map_valid_targets() {
    // CURSOR_TO_WB_MAP 的每个目标都应在 MODELS 中
    let model_ids: std::collections::HashSet<&str> = MODELS.iter().map(|(id, _)| *id).collect();
    for (_, wb) in CURSOR_TO_WB_MAP {
        assert!(
            model_ids.contains(wb),
            "映射目标不在模型列表中: {}",
            wb
        );
    }
}
