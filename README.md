# WorkBuddy Proxy (Rust)

WorkBuddy → OpenAI 兼容反向代理的 **Rust 重写版**。

功能与 [Python 版 server.py](https://github.com/your/workbuddy-proxy) 100% 兼容，但：
- **单二进制**：~4.3MB（Python venv 约 100MB+），无需安装 Python
- **跨平台**：macOS (arm64/x86_64)、Windows、Linux 均可构建分发
- **高性能**：HTTP/2 + rustls + 异步全链路，连接复用

## 快速开始

```bash
# 构建
cargo build --release

# 运行（自动通过 CDP 提取 WorkBuddy token）
./target/release/wb-proxy

# 可选：复制 .env 修改配置
cp .env.example .env
```

## 配置（.env）

| 变量 | 默认值 | 说明 |
|---|---|---|
| `PROXY_PORT` | 19090 | 监听端口 |
| `PROXY_API_KEY` | wb-proxy-key | 客户端 API Key |
| `WB_API_BASE` | https://copilot.tencent.com | WorkBuddy 上游 |
| `CDP_URL` | http://127.0.0.1:9222 | CDP 调试地址 |
| `WB_TIMEOUT` | 120 | 普通模型超时（秒） |
| `WB_REASONING_TIMEOUT` | 300 | 推理模型超时（秒） |
| `WB_TOKEN` / `WB_REFRESH_TOKEN` | 空 | 手动 token（可选，默认 CDP 自动提取） |

## 接口

| 端点 | 说明 |
|---|---|
| `GET /health` | 健康检查（含 token 状态） |
| `GET /v1/models` | 模型列表（Cursor 别名 + WB 原始模型） |
| `POST /v1/chat/completions` | OpenAI 兼容对话（流式/非流式） |

认证：`Authorization: Bearer <PROXY_API_KEY>` 或 `X-API-Key: <PROXY_API_KEY>`

## 跨平台构建

```bash
# 本机（macOS arm64）
cargo build --release

# 其他平台（GitHub Actions 手动触发，见 .github/workflows/build.yml）
```

## PM2 部署

```bash
npm i -g pm2
pm2 start ecosystem.config.cjs
```

## 测试

```bash
cargo test                       # 单元测试（JWT、模型映射）
python3 ../workbuddy-proxy/tests/test_compatibility.py  # 兼容性验收
```

## 与 Python 版差异

| 项 | Python 版 | Rust 版 |
|---|---|---|
| 部署体积 | ~100MB venv | 4.3MB 单二进制 |
| HTTP/2 | 未启用 | ✅ 启用 |
| TLS | verify=False | rustls 默认校验 |
| 启动速度 | ~1s | ~50ms |
| 内存 | ~100MB | ~20MB |
