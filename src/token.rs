use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use futures_util::{SinkExt, StreamExt};

use crate::config::Config;
use crate::jwt::{is_expired, log_token_info, parse_jwt_claims};

/// 与 Python 版 TokenManager 完全一致。
/// 注意：TokenManager 自身不持锁，由 AppState 用 Arc<Mutex<TokenManager>> 保护，
/// 调用方在锁内调用其 &mut self 方法（对应 Python asyncio.Lock + 双检）。
pub struct TokenManager {
    pub access_token: String,
    pub refresh_token: String,
    pub user_id: String,
    pub enterprise_id: String,
    pub domain: String,
    pub department_info: String,
    pub config: Arc<Config>,
    pub client: reqwest::Client,
}

impl TokenManager {
    pub fn new(config: Arc<Config>) -> Self {
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true) // 与 Python verify=False 一致
            .timeout(std::time::Duration::from_secs(config.wb_timeout))
            .connect_timeout(std::time::Duration::from_secs(10))
            .pool_max_idle_per_host(10)
            .build()
            .expect("failed to build reqwest client");

        TokenManager {
            access_token: String::new(),
            refresh_token: String::new(),
            user_id: String::new(),
            enterprise_id: String::new(),
            domain: String::new(),
            department_info: String::new(),
            config,
            client,
        }
    }

    /// 初始化（与 Python init() 一致），返回是否成功获取到 token
    pub async fn init(&mut self) -> bool {
        self.access_token = self.config.wb_token.clone();
        self.refresh_token = self.config.wb_refresh_token.clone();

        if self.access_token.is_empty() {
            self.load_from_file();
        }

        if self.access_token.is_empty() {
            self.extract_from_cdp().await;
        }

        if !self.access_token.is_empty() {
            self.apply_claims();
            log_token_info(&self.access_token);
            self.save_to_file();
            return true;
        }
        false
    }

    /// 从 token.json 加载（与 Python _load_from_file 一致）
    fn load_from_file(&mut self) {
        let path = self.config.token_file();
        if !path.exists() {
            return;
        }
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(data) = serde_json::from_str::<serde_json::Value>(&text) {
                self.access_token = data
                    .get("access_token")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                self.refresh_token = data
                    .get("refresh_token")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if !self.access_token.is_empty() {
                    tracing::info!("Token loaded from file");
                }
            }
        }
    }

    /// 保存到 token.json（与 Python _save_to_file 一致，字段完全兼容）
    pub fn save_to_file(&self) {
        let path = self.config.token_file();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let saved_at = {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            format_saved_at(now)
        };
        let data = serde_json::json!({
            "access_token": self.access_token,
            "refresh_token": self.refresh_token,
            "saved_at": saved_at,
        });
        if let Ok(text) = serde_json::to_string_pretty(&data) {
            let _ = std::fs::write(&path, text);
        }
    }

    /// 解析 JWT 中的用户信息（与 Python _apply_claims 一致）
    pub fn apply_claims(&mut self) {
        let claims = parse_jwt_claims(&self.access_token);
        self.user_id = if self.config.wb_user_id.is_empty() {
            claims.user_id
        } else {
            self.config.wb_user_id.clone()
        };
        self.enterprise_id = if self.config.wb_enterprise_id.is_empty() {
            claims.enterprise_id
        } else {
            self.config.wb_enterprise_id.clone()
        };
        self.domain = if self.config.wb_domain.is_empty() {
            claims.domain
        } else {
            self.config.wb_domain.clone()
        };
        let user_preview: String = self.user_id.chars().take(8).collect();
        tracing::info!(
            "User: {}..., Enterprise: {}, Domain: {}",
            user_preview,
            self.enterprise_id,
            self.domain
        );
    }

    /// 获取有效 token（与 Python get_token 一致，需在外部锁内调用）
    pub fn get_token(&mut self) -> String {
        if is_expired(&self.access_token) {
            let rt = self.refresh_token.clone();
            // 有 refresh_token 优先 API，否则 CDP（与 Python refresh() 一致）
            if !rt.is_empty() {
                let ok = tokio::task::block_in_place(|| {
                    // 注意：此函数设计为在 async 上下文中调用，这里用运行时调度
                    unreachable!("use async get_token_async instead")
                });
                let _ = ok;
            }
            // 实际异步刷新在 get_token_async 中处理
            self.access_token.clone()
        } else {
            self.access_token.clone()
        }
    }

    /// 异步获取有效 token（推荐路径，与 Python get_token 一致）
    pub async fn get_token_async(&mut self) -> String {
        if is_expired(&self.access_token) {
            self.refresh_async().await;
        }
        self.access_token.clone()
    }

    /// 异步刷新（与 Python refresh() 一致：有 refresh_token → API，失败 → CDP）
    pub async fn refresh_async(&mut self) {
        if !is_expired(&self.access_token) {
            return;
        }
        if !self.refresh_token.is_empty() {
            let ok = self.refresh_via_api().await;
            if ok {
                return;
            }
        }
        self.extract_from_cdp().await;
    }

    /// 通过 API 刷新 token（与 Python _refresh_via_api 一致）
    async fn refresh_via_api(&mut self) -> bool {
        tracing::info!("Refreshing token via API...");
        let headers = build_headers(
            &self.config,
            &self.access_token,
            &self.user_id,
            &self.enterprise_id,
            &self.domain,
            &self.department_info,
        );
        let url = format!("{}/v2/plugin/auth/token/refresh", self.config.wb_api_base);
        let resp = self
            .client
            .post(&url)
            .headers(headers)
            .json(&serde_json::json!({}))
            .send()
            .await;

        match resp {
            Ok(resp) => {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(&text) {
                    if data.get("code").and_then(|v| v.as_i64()) == Some(0) {
                        if let Some(access) = data
                            .get("data")
                            .and_then(|d| d.get("accessToken"))
                            .and_then(|v| v.as_str())
                        {
                            self.access_token = access.to_string();
                            if let Some(rt) = data
                                .get("data")
                                .and_then(|d| d.get("refreshToken"))
                                .and_then(|v| v.as_str())
                            {
                                self.refresh_token = rt.to_string();
                            }
                            self.apply_claims();
                            log_token_info(&self.access_token);
                            self.save_to_file();
                            tracing::info!("Token refreshed successfully via API");
                            return true;
                        }
                    }
                    tracing::error!(
                        "Token refresh failed: {}",
                        text.chars().take(200).collect::<String>()
                    );
                } else {
                    tracing::error!(
                        "Token refresh failed (status {}): {}",
                        status,
                        text.chars().take(200).collect::<String>()
                    );
                }
            }
            Err(e) => {
                tracing::error!("Token refresh request failed: {}", e);
            }
        }
        false
    }

    /// CDP 提取（与 Python _extract_from_cdp 一致，JS 注入字符串原样保留）
    pub async fn extract_from_cdp(&mut self) {
        tracing::info!(
            "Extracting token from WorkBuddy via CDP ({})...",
            self.config.cdp_url
        );

        // 1. 获取 CDP targets
        let targets: Vec<serde_json::Value> = match self
            .client
            .get(format!("{}/json", self.config.cdp_url))
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(resp) => match resp.json::<serde_json::Value>().await {
                Ok(v) => v.as_array().cloned().unwrap_or_default(),
                Err(_) => return,
            },
            Err(_) => return,
        };

        // 2. 找 workbench page，否则任意 page
        let mut ws_url: Option<String> = None;
        for t in &targets {
            let ttype = t.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let url = t.get("url").and_then(|v| v.as_str()).unwrap_or("");
            if ttype == "page" && url.contains("workbench") {
                ws_url = t
                    .get("webSocketDebuggerUrl")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                break;
            }
        }
        if ws_url.is_none() {
            for t in &targets {
                let ttype = t.get("type").and_then(|v| v.as_str()).unwrap_or("");
                if ttype == "page" {
                    ws_url = t
                        .get("webSocketDebuggerUrl")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    break;
                }
            }
        }
        let ws_url = match ws_url {
            Some(u) => u,
            None => {
                tracing::error!("No CDP target found");
                return;
            }
        };

        // 3. WebSocket 连接并执行 JS
        // JS 注入字符串与 Python 版 100% 一致（WorkBuddy 5.3.8+ 认证接口）
        let js_expression = r#"
            (async () => {
                try {
                    var auth = window.__GENIE_DEFAULT_APP_PROVIDERS__
                        && window.__GENIE_DEFAULT_APP_PROVIDERS__.auth;
                    var fn = auth && auth.getToken;
                    if (typeof fn === 'function') {
                        var t = await fn();
                        if (typeof t === 'string') {
                            return JSON.stringify({ accessToken: t });
                        }
                        if (t && typeof t === 'object') {
                            return JSON.stringify(t);
                        }
                    }
                } catch (e) {}
                try {
                    const s = await window.vscode.ipcRenderer.invoke(
                        'vscode:genie:auth:getSession'
                    );
                    return JSON.stringify(s);
                } catch (e) {
                    return JSON.stringify({error: e.message});
                }
            })()
        "#;

        let cmd = serde_json::json!({
            "id": 1,
            "method": "Runtime.evaluate",
            "params": {
                "expression": js_expression,
                "awaitPromise": true,
                "returnByValue": true,
            }
        });

        // 连接 WebSocket
        let (mut ws, _) = match tokio_tungstenite::connect_async(&ws_url).await {
            Ok(v) => v,
            Err(e) => {
                tracing::error!("CDP WebSocket connect failed: {}", e);
                return;
            }
        };

        // 发送命令
        let msg = tokio_tungstenite::tungstenite::Message::Text(cmd.to_string());
        if ws.send(msg).await.is_err() {
            tracing::error!("CDP send failed");
            let _ = ws.close(None).await;
            return;
        }

        // 等待响应（10s 超时），找 id==1 的消息
        let resp_text = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            async {
                loop {
                    match ws.next().await {
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Text(t))) => {
                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&t) {
                                if v.get("id").and_then(|x| x.as_i64()) == Some(1) {
                                    return Some(t);
                                }
                            }
                        }
                        Some(Ok(_)) => continue,
                        Some(Err(e)) => {
                            tracing::error!("CDP recv error: {}", e);
                            return None;
                        }
                        None => return None,
                    }
                }
            },
        )
        .await;

        let result = match resp_text {
            Ok(Some(t)) => t,
            _ => {
                tracing::error!("CDP extraction timeout");
                let _ = ws.close(None).await;
                return;
            }
        };
        let _ = ws.close(None).await;

        // 4. 解析结果（与 Python 版一致：result.result.result.value）
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&result) {
            let value = v
                .get("result")
                .and_then(|r| r.get("result"))
                .and_then(|r| r.get("value"))
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            if value.is_empty() {
                return;
            }
            if let Ok(mut session) = serde_json::from_str::<serde_json::Value>(&value) {
                // 极少数情况直接返回裸 JWT 字符串
                if session.is_string() {
                    let s = session.as_str().unwrap_or("").to_string();
                    session = serde_json::json!({ "accessToken": s });
                }
                // auth 字段回退
                let auth = session.get("auth").cloned().unwrap_or_else(|| session.clone());
                if let Some(access) = auth.get("accessToken").and_then(|v| v.as_str()) {
                    self.access_token = access.to_string();
                    self.refresh_token = auth
                        .get("refreshToken")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if let Some(account) = session.get("account") {
                        self.department_info = account
                            .get("departmentFullName")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                    }
                    self.apply_claims();
                    log_token_info(&self.access_token);
                    self.save_to_file();
                    tracing::info!("Token extracted from CDP successfully");
                } else if let Some(err) = session.get("error").and_then(|v| v.as_str()) {
                    tracing::error!("CDP extraction error: {}", err);
                }
            }
        }
    }
}

/// 构建请求头（与 Python _build_headers 一致）
pub fn build_headers(
    config: &Config,
    access_token: &str,
    user_id: &str,
    enterprise_id: &str,
    domain: &str,
    department_info: &str,
) -> reqwest::header::HeaderMap {
    use reqwest::header::{HeaderMap, HeaderValue};
    let mut headers = HeaderMap::new();
    let wb_version = config.final_wb_version();

    let fixed: Vec<(&str, String)> = vec![
        ("X-IDE-Type", "CodeBuddyIDE".to_string()),
        ("X-IDE-Name", "CodeBuddyIDE".to_string()),
        ("X-IDE-Version", wb_version.clone()),
        ("X-Product-Version", wb_version.clone()),
        ("X-Product", "SaaS".to_string()),
        ("X-Env-ID", "production".to_string()),
        ("X-Requested-With", "XMLHttpRequest".to_string()),
        (
            "User-Agent",
            format!("CodeBuddyIDE/{} coding-copilot/{}", wb_version, wb_version),
        ),
        ("Content-Type", "application/json".to_string()),
        ("Accept", "text/event-stream".to_string()),
        ("Authorization", format!("Bearer {}", access_token)),
        ("X-User-Id", user_id.to_string()),
        ("X-Enterprise-Id", enterprise_id.to_string()),
        ("X-Tenant-Id", enterprise_id.to_string()),
        ("X-Domain", domain.to_string()),
        ("X-Request-ID", uuid::Uuid::new_v4().simple().to_string()),
        ("X-Request-Trace-Id", uuid::Uuid::new_v4().to_string()),
    ];
    if !department_info.is_empty() {
        if let Ok(hv) = HeaderValue::from_str(department_info) {
            headers.insert("X-Department-Info", hv);
        }
    }
    for (k, v) in fixed {
        if let Ok(hv) = HeaderValue::from_str(&v) {
            headers.insert(k, hv);
        }
    }
    headers
}

/// 格式化时间戳为 Python time.strftime("%Y-%m-%d %H:%M:%S") 格式
fn format_saved_at(ts: u64) -> String {
    let secs = ts as i64;
    let days = secs / 86400;
    let rem = secs % 86400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let (y, mo, d) = civil_from_days(days);
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", y, mo, d, h, m, s)
}

/// 天数 → (年, 月, 日)，Howard Hinnant 算法（UTC）
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}
