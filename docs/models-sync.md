# 模型对齐指南（WorkBuddy ↔ 本项目）

WorkBuddy 客户端更新后会新增/下架模型。本项目的模型列表（`src/models.rs`）需要定期
与上游对齐。本文说明**可靠的数据来源**和**一键对齐流程**。

> 背景：2026-08 第一次对齐时只看了 `main.log` 的基础配置（44 个模型 ID），
> 漏掉了 renderer 里 CLI 界面**实际可选**的模型（glm-5.2 / hy3 / minimax-m3 / kimi-k3-1 等），
> 导致误判。本文档的方法已把"CLI 可选模型"作为唯一权威源，避免再次踩坑。

---

## 1. 数据源（权威性排序）

| 优先级 | 数据源 | 文件 | 含义 | 角色 |
|---|---|---|---|---|
| **★ 权威** | `renderer.log` | `~/Library/Logs/WorkBuddy/renderer.log` | `declaredModels=[...]`，WorkBuddy UI **实际下发给用户可选的模型** | 对齐依据 |
| 交叉验证 | `main.log` | `~/Library/Logs/WorkBuddy/main.log` | `[buildResolvedProductConfig] resolved ids: ...`，基础配置模型（可能包含非 CLI 的内部/后备模型） | 校验 CLI 列表可信度 |

**Windows**：`%LOCALAPPDATA%\WorkBuddy\logs\`
**Linux**：`~/.config/WorkBuddy/logs/` 或 `~/.local/share/WorkBuddy/logs/`

### 为什么必须以 renderer 的 CLI 模型为准？

- `main.log` 的 44 个基础模型里包含 `default`、`codewise-*`、`completion-gf`、`kling-*` 等
  内部路由/非对话模型，直接对齐会污染列表。
- renderer 的 `declaredModels` 是用户**在聊天界面真正能看到、能选**的模型
  （本次实测 11 个：auto/hy3/glm-5.2/glm-5.1/glm-5v-turbo/minimax-m3/kimi-k3-1/kimi-k2.7/kimi-k2.6/deepseek-v4-flash/deepseek-v4-pro），
  这才是代理应该暴露的集合。

## 2. 一键对齐

```bash
# 仅查看报告（安全，不改文件）
python3 scripts/sync_models.py -v

# 对齐并重写 src/models.rs
python3 scripts/sync_models.py --apply
```

脚本行为：

1. **自动探测日志目录**（macOS/Windows/Linux）
2. 从 `main.log` 提取基础配置模型 → 从 `renderer.log` 提取**与基础配置交集最大**的
   `declaredModels` 批次（自动剔除历史废弃批次）
3. 输出 `[➕新增 / ➖删除 / ✅保留]` 报告
4. `--apply` 时重写 `MODELS` 常量（保留已有显示名，新模型以 ID 占位 + `// TODO: 确认显示名`）
5. 备份原文件为 `src/models.rs.bak`

### 可靠性校验（脚本内置）

- CLI 模型必须与 `main.log` 基础配置**有交集**，否则判定数据源异常并报错退出
- 模型 ID 必须匹配 `^[A-Za-z0-9._-]+$`，剔除 `{"suppressed":N}` 等脏数据
- 日志缺失/格式变化 → 明确报错 + 排查指引，不静默输出

## 3. 对齐后的人工步骤（必须）

脚本只自动维护 `MODELS`，**以下部分需要人工确认**：

| 项 | 说明 |
|---|---|
| 显示名 | 新模型是 ID 占位 + TODO，改成可读名（如 `DeepSeek-V4-Pro`） |
| `CURSOR_TO_WB_MAP` | 检查失效映射（如上游下架的 claude/gemini），手动删除 |
| `REASONING_MODELS` | 新推理模型（如带 thinking/思考的）加入，超时走 300s |
| 非对话模型 | `kling-*`（视频）、`hunyuan-image-*`（图片）、`default`/`codewise-*`（内部）等按需剔除 |

完成后：

```bash
cargo test            # 模型总数断言等测试
cargo build --release
```

## 4. 甄别规则速查

**保留**：能对话的 LLM（deepseek/glm/hunyuan/kimi/minimax 各系列）
**建议剔除**：
- `kling-v3-t2v` / `kling-v3-i2v` — 视频生成
- `hunyuan-image-v3.0` — 文生图
- `default` / `default-1.1` / `default-1.2` / `auto` / `codewise-*` / `completion-gf` — 内部路由/补全专用

## 5. 常见问题

**Q: 报错"无法从 main.log 提取 resolved ids"？**
WorkBuddy 未运行，或日志被清理。先启动 WorkBuddy（带 `--remote-debugging-port=9222` 调试模式），
正常使用一会儿后再跑脚本。

**Q: 报错"declaredModels 与基础配置无交集"？**
日志里有多个历史批次但都和当前基础配置对不上——通常说明 WorkBuddy 已大版本更新，
日志格式或模型体系变了。手动检查 renderer.log 里最新的 `declaredModels` 行确认。

**Q: 对齐后模型数量不对？**
`--apply` 默认**全量覆盖**为 CLI 模型。如果之前人工剔除了部分模型（如视频/图片），
下次对齐会重新加回来——需在甄别步骤手动删。可在报告中先看 `➕新增` 再决定。
