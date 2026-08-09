#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
WorkBuddy 模型对齐工具
======================
从本机 WorkBuddy 日志提取"CLI 界面真实可选的模型"，与 src/models.rs 对比，
输出 [新增 / 删除 / 保留] 报告；--apply 时自动重写 src/models.rs 的 MODELS 常量。

数据源（跨平台自动探测）：
  - renderer.log  : renderer 进程日志，含 `declaredModels=[...]` —— CLI 界面实际可选模型【唯一权威源】
  - main.log      : 主进程日志，含 `[buildResolvedProductConfig] resolved ids:` —— 基础配置模型【交叉验证用】

用法：
  python3 scripts/sync_models.py            # 只出报告（默认，安全）
  python3 scripts/sync_models.py --apply    # 出报告 + 重写 src/models.rs 的 MODELS 常量
  python3 scripts/sync_models.py -v         # 详细日志

注意事项：
  1. CLI 模型以 renderer.log 的 declaredModels 为权威（WorkBuddy UI 真实可选），
     main.log 的基础配置只做交叉验证，不作为对齐依据。
  2. declaredModels 可能有多批历史记录（旧版/新版），必须取最新一条，且与
     main.log 基础配置有交集才认为可信，否则拒绝输出。
  3. 结果需人工甄别：对话 LLM 模型保留；kling-*(视频)/hunyuan-image-*(图片)/
     default/codewise-* 等非对话或内部路由模型请手动跳过。
"""

import argparse
import os
import re
import subprocess
import sys
from pathlib import Path

# ---------------------------------------------------------------------------
# 路径探测
# ---------------------------------------------------------------------------

def _mac_log_dir() -> Path:
    return Path.home() / "Library" / "Logs" / "WorkBuddy"

def _win_log_dir() -> Path:
    local = os.environ.get("LOCALAPPDATA", "")
    if local:
        return Path(local) / "WorkBuddy" / "logs"
    return Path()

def _linux_log_dir() -> Path:
    home = Path.home()
    for p in (
        home / ".config" / "WorkBuddy" / "logs",
        home / ".local" / "share" / "WorkBuddy" / "logs",
    ):
        if p.exists():
            return p
    return Path()

def _detect_log_dir() -> Path:
    if sys.platform == "darwin":
        return _mac_log_dir()
    if sys.platform == "win32":
        return _win_log_dir()
    return _linux_log_dir()

def _find_log(log_dir: Path, name: str) -> Path:
    """在日志目录及其子目录中查找指定日志文件（含 .log / .log.1 / 日期后缀）"""
    if not log_dir.exists():
        return Path()
    # 精确匹配优先
    for f in log_dir.rglob(f"{name}*"):
        if f.is_file():
            return f
    return Path()

# ---------------------------------------------------------------------------
# 日志解析
# ---------------------------------------------------------------------------

# 每条日志是 JSON 行，message 是数组，最后一项可能含 {"suppressed":N}
_RAW_RE = re.compile(r'"message":\s*(\[[^\]]*\])')

def _extract_from_message(message_str: str, pattern: re.Pattern, timestamp: str):
    """在 message 数组字符串中找 pattern，返回 (模型列表, 时间戳)；找不到返回 None"""
    m = pattern.search(message_str)
    if not m:
        return None
    return m, timestamp

def _parse_declared_models(text: str) -> list:
    """
    从 renderer.log 提取 declaredModels=[...]，返回模型 ID 列表。
    可靠规则：
      - 必须是合法的 [id1,id2,...] 形式
      - 每个 ID 匹配 ^[A-Za-z0-9._-]+$（剔除 {"suppressed":N} 等脏数据）
      - 若解析不出合法 ID 返回空
    """
    # 找所有 declaredModels=[...]（可能跨行，先整段匹配）
    results = []
    for m in re.finditer(r'declaredModels=\[([^\]]*)\]', text):
        raw = m.group(1)
        ids = []
        for item in raw.split(","):
            item = item.strip().strip('"')
            if not item:
                continue
            # 剔除 {"suppressed":N} 这类 JSON 对象残留
            if item.startswith("{") or item.endswith("}"):
                continue
            if not re.fullmatch(r"[A-Za-z0-9._-]+", item):
                continue
            ids.append(item)
        if ids:
            results.append(ids)
    return results

def _parse_resolved_ids(text: str) -> list:
    """从 main.log 提取 [buildResolvedProductConfig] resolved ids: ...，返回模型 ID 列表"""
    for m in re.finditer(r'resolved ids:\s*([^\n]+)', text):
        raw = m.group(1)
        ids = []
        for item in raw.split(","):
            item = item.strip().strip('"')
            if not item:
                continue
            if item.startswith("{") or item.endswith("}"):
                continue
            if not re.fullmatch(r"[A-Za-z0-9._-]+", item):
                continue
            ids.append(item)
        if ids:
            return ids
    return []

def _line_timestamp(line: str) -> str:
    m = re.search(r'"timestamp":"([^"]+)"', line)
    return m.group(1) if m else ""

def _extract_cli_models(renderer_text: str, main_resolved: set, gold: set = None) -> list:
    """
    从 renderer.log 提取 CLI 可选模型，并做可靠性校验。

    选择规则（按优先级）：
      1. 若提供金标准集合（已确认可用的模型，如 pi 正在用的 deepseek-v4-pro/hy3/glm-5.2 等），
         选命中金标准最多的批次——这是最可靠的锚点（金标准必然是 CLI 真实模型）。
      2. 否则与 main.log 基础配置交集最大的批次。
    校验：命中数必须 > 0，否则判定数据源可疑返回 []。
    """
    candidates = _parse_declared_models(renderer_text)
    if not candidates:
        return []

    anchor = gold if gold else main_resolved
    best = None
    best_hits = -1
    for cand in candidates:
        hits = len(set(cand) & set(anchor))
        if hits > best_hits:
            best_hits = hits
            best = cand
    if best is None or best_hits == 0:
        return []
    return best

# ---------------------------------------------------------------------------
# 模型 ID 分类
# ---------------------------------------------------------------------------

# 非对话模型关键词：这些即使上游有也不应出现在对话模型列表
_NON_CHAT_HINTS = ("kling-", "image", "completion", "default", "codewise", "auto")

def _is_likely_chat(model_id: str) -> bool:
    """粗判：是否疑似对话模型（仅提示用，不自动过滤，最终靠人工甄别）"""
    return not any(h in model_id for h in _NON_CHAT_HINTS)

# ---------------------------------------------------------------------------
# 与 src/models.rs 对比
# ---------------------------------------------------------------------------

def _repo_root() -> Path:
    return Path(__file__).resolve().parent.parent

def _parse_models_rs(text: str):
    """解析 src/models.rs 的 MODELS 常量，返回 (id->显示名 dict, 行号)"""
    m = re.search(r"pub const MODELS: &\[\(&str, &str\)\] = &\[(.*?)\];", text, re.S)
    if not m:
        return {}, None
    body = m.group(1)
    entries = {}
    for e in re.finditer(r'\("([^"]+)", "([^"]+)"\)', body):
        entries[e.group(1)] = e.group(2)
    return entries, m

# ---------------------------------------------------------------------------
# 报告输出
# ---------------------------------------------------------------------------

def _print_section(title: str, items: list, marker: str):
    print(f"\n{'='*70}\n{title}\n{'='*70}")
    if not items:
        print("  (无)")
        return
    for it in items:
        print(f"  {marker} {it}")

# ---------------------------------------------------------------------------
# 重写 MODELS 常量
# ---------------------------------------------------------------------------

def _rewrite_models_rs(path: Path, cli_models: list, old_entries: dict) -> bool:
    """
    最小侵入式更新：只替换 src/models.rs 中 MODELS 常量的内容，
    其余（CURSOR_TO_WB_MAP / REASONING_MODELS / 函数）原样保留。
    - 保留旧显示名；新 ID 用 ID 占位并加 TODO 注释
    - 保持 DeepSeek/GLM/Hunyuan/Kimi/MiniMax 分组注释（按前缀归类，每个模型只归一组）
    """
    groups = [
        ("DeepSeek", lambda i: i.startswith("deepseek")),
        ("GLM", lambda i: i.startswith("glm")),
        ("Hunyuan", lambda i: i.startswith(("hunyuan", "hy"))),
        ("Kimi", lambda i: i.startswith("kimi")),
        ("MiniMax", lambda i: i.startswith("minimax")),
        ("其他", lambda i: True),
    ]

    assigned = set()
    body_lines = []
    for gname, pred in groups:
        items = sorted([i for i in cli_models if pred(i) and i not in assigned])
        if not items:
            continue
        for i in items:
            assigned.add(i)
        body_lines.append(f"    // {gname}")
        for mid in items:
            old = old_entries.get(mid)
            if old:
                body_lines.append(f'    ("{mid}", "{old}"),')
            else:
                body_lines.append(f'    ("{mid}", "{mid}"), // TODO: 确认显示名')

    new_body = "\n".join(body_lines)

    text = path.read_text(encoding="utf-8")
    # 只替换 MODELS 常量数组内容
    pattern = re.compile(r"pub const MODELS: &\[\(&str, &str\)\] = &\[(.*?)\];", re.S)
    if not pattern.search(text):
        return False
    new_text = pattern.sub(lambda m: f"pub const MODELS: &[(&str, &str)] = &[\n{new_body}\n];", text)
    path.write_text(new_text, encoding="utf-8")
    return True

# ---------------------------------------------------------------------------
# 主流程
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(description="从 WorkBuddy 日志对齐模型")
    parser.add_argument("--apply", action="store_true", help="对齐后自动重写 src/models.rs")
    parser.add_argument("-v", "--verbose", action="store_true", help="详细日志")
    args = parser.parse_args()

    root = _repo_root()
    log_dir = _detect_log_dir()
    if args.verbose:
        print(f"[info] 日志目录: {log_dir}")

    # 1. 读 main.log（交叉验证用）
    main_log = _find_log(log_dir, "main.log")
    main_resolved = []
    if main_log.exists():
        main_text = main_log.read_text(errors="ignore")
        main_resolved = _parse_resolved_ids(main_text)
    if not main_resolved:
        print("[错误] 无法从 main.log 提取 resolved ids（WorkBuddy 可能未运行，或日志路径/格式已变化）")
        print(f"       查找路径: {log_dir}")
        sys.exit(1)

    # 2. 读 renderer.log（权威源）
    renderer_log = _find_log(log_dir, "renderer.log")
    if not renderer_log.exists():
        print("[错误] 找不到 renderer.log，无法获取 CLI 模型")
        sys.exit(1)
    renderer_text = renderer_log.read_text(errors="ignore")
    # 金标准：pi 正在使用的、已验证可用的模型（代码里也存在的、上游 CL I 批次里命中数最多的锚）
    # 优先用代码中已存在的模型作为锚点（它们都是人工验证过的）
    _, _ = None, None
    old_entries_early, _ = _parse_models_rs((root / "src" / "models.rs").read_text(encoding="utf-8"))
    gold = set(old_entries_early.keys())
    cli_models = _extract_cli_models(renderer_text, set(main_resolved), gold=gold)
    if not cli_models:
        print("[错误] 未能从 renderer.log 提取可信的 CLI 模型（declaredModels 与基础配置无交集）")
        print("       请确认 WorkBuddy 正常运行、且日志是最新一次启动产生的")
        sys.exit(1)

    if args.verbose:
        print(f"[info] main.log 基础配置模型: {len(main_resolved)} 个")
        print(f"[info] renderer.log CLI 可选模型: {len(cli_models)} 个 -> {cli_models}")

    # 3. 读取现有 models.rs
    models_rs = root / "src" / "models.rs"
    text = models_rs.read_text(encoding="utf-8")
    old_entries, _ = _parse_models_rs(text)
    old_set = set(old_entries)

    cli_set = set(cli_models)
    to_add = sorted(cli_set - old_set)
    to_del = sorted(old_set - cli_set)
    keep = sorted(cli_set & old_set)

    # 4. 报告
    print(f"\n对齐基准: WorkBuddy CLI 可选模型 {len(cli_models)} 个")
    _print_section("➕ 新增（上游 CLI 有、代码没有）", to_add, "+")
    _print_section("➖ 删除（代码有、上游 CLI 没有）", to_del, "-")
    _print_section("✅ 保留（两边都有）", keep, "=")

    # 非对话提示
    non_chat = [i for i in cli_models if not _is_likely_chat(i)]
    if non_chat:
        print(f"\n[提示] 以下疑似非对话/内部模型，请人工确认是否保留:")
        for i in non_chat:
            print(f"        · {i}")

    # 5. 应用
    if args.apply:
        # 备份
        backup = models_rs.with_suffix(".rs.bak")
        backup.write_text(text, encoding="utf-8")
        if _rewrite_models_rs(models_rs, cli_models, old_entries):
            print(f"\n[完成] 已重写 {models_rs.relative_to(root)}")
            print(f"       备份: {backup.name}")
            print("       请人工检查: 1) TODO 显示名  2) CURSOR_TO_WB_MAP/REASONING_MODELS")
            print("       然后: cargo test && cargo build --release")
    else:
        print("\n(仅报告，未修改文件。加 --apply 落盘)")

if __name__ == "__main__":
    main()
