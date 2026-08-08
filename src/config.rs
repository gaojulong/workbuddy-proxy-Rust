use std::env;
use std::path::PathBuf;

/// 与 Python 版 server.py 完全一致的配置加载逻辑：
/// 已有环境变量优先（不覆盖）→ .env → 默认值
#[derive(Debug, Clone)]
pub struct Config {
    pub proxy_port: u16,
    pub proxy_api_key: String,
    pub wb_api_base: String,
    pub cdp_url: String,
    pub wb_timeout: u64,
    pub wb_reasoning_timeout: u64,
    pub wb_version: String,
    pub wb_token: String,
    pub wb_refresh_token: String,
    pub wb_user_id: String,
    pub wb_enterprise_id: String,
    pub wb_domain: String,
    pub base_dir: PathBuf,
}

impl Config {
    pub fn load() -> Self {
        // .env 加载（不覆盖已有环境变量），与 Python dotenv(override=False) 一致
        // base_dir 使用可执行文件所在目录（而非编译时路径），
        // 保证分发后 .env / data/ 与程序放一起，任何位置运行都正确
        let base_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."));
        let _ = dotenvy::from_path(base_dir.join(".env"));

        let get = |key: &str, default: &str| -> String {
            env::var(key).unwrap_or_else(|_| default.to_string())
        };

        let proxy_port = get("PROXY_PORT", "19090").parse().unwrap_or(19090);
        let wb_timeout: u64 = get("WB_TIMEOUT", "120").parse().unwrap_or(120);
        let wb_reasoning_timeout: u64 = get("WB_REASONING_TIMEOUT", "300").parse().unwrap_or(300);

        Config {
            proxy_port,
            proxy_api_key: get("PROXY_API_KEY", "wb-proxy-key"),
            wb_api_base: get("WB_API_BASE", "https://copilot.tencent.com"),
            cdp_url: get("CDP_URL", "http://127.0.0.1:9222"),
            wb_timeout,
            wb_reasoning_timeout,
            wb_version: get("WB_VERSION", ""),
            wb_token: get("WB_TOKEN", ""),
            wb_refresh_token: get("WB_REFRESH_TOKEN", ""),
            wb_user_id: get("WB_USER_ID", ""),
            wb_enterprise_id: get("WB_ENTERPRISE_ID", ""),
            wb_domain: get("WB_DOMAIN", ""),
            base_dir,
        }
    }

    /// 与 Python `_detect_wb_version()` 一致：扫描候选路径读 genieVersion
    pub fn detect_wb_version(&self) -> String {
        let mut candidates: Vec<PathBuf> = Vec::new();

        // macOS
        candidates.push(PathBuf::from("/Applications/WorkBuddy.app/Contents/Resources/app/product.json"));
        // Windows 常见位置
        if let Ok(local) = env::var("LOCALAPPDATA") {
            candidates.push(PathBuf::from(format!(
                "{}\\Programs\\WorkBuddy\\resources\\app\\product.json", local
            )));
        }
        if let Ok(pf) = env::var("ProgramFiles") {
            candidates.push(PathBuf::from(format!(
                "{}\\WorkBuddy\\resources\\app\\product.json", pf
            )));
        }
        if let Ok(pf) = env::var("ProgramFiles(x86)") {
            candidates.push(PathBuf::from(format!(
                "{}\\WorkBuddy\\resources\\app\\product.json", pf
            )));
        }
        if let Ok(app) = env::var("APPDATA") {
            candidates.push(PathBuf::from(format!(
                "{}\\WorkBuddy\\resources\\app\\product.json", app
            )));
        }
        // Linux
        if let Ok(home) = env::var("HOME") {
            candidates.push(PathBuf::from(format!(
                "{}/.local/share/WorkBuddy/resources/app/product.json", home
            )));
        }
        candidates.push(PathBuf::from("/opt/WorkBuddy/resources/app/product.json"));

        for p in candidates {
            if let Ok(text) = std::fs::read_to_string(&p) {
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(v) = data.get("genieVersion").and_then(|x| x.as_str()) {
                        if !v.is_empty() {
                            tracing::info!(
                                "Detected WorkBuddy {} at {:?}",
                                v,
                                p.parent().unwrap_or(p.as_path())
                            );
                            return v.to_string();
                        }
                    }
                }
            }
        }
        String::new()
    }

    /// token.json 路径（与 Python 版一致：BASE_DIR/data/token.json）
    pub fn token_file(&self) -> PathBuf {
        self.base_dir.join("data").join("token.json")
    }

    pub fn final_wb_version(&self) -> String {
        if !self.wb_version.is_empty() {
            self.wb_version.clone()
        } else {
            let detected = self.detect_wb_version();
            if !detected.is_empty() {
                detected
            } else {
                "4.8.1".to_string()
            }
        }
    }
}
