#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
WorkBuddy 模型图片支持检测工具
==============================
用真实 token 直接请求 WorkBuddy 上游 API，给每个模型发一张带图片的请求，
根据回复判断该模型是否真正支持图片（能"看到"图片内容）。

背景：
  - WorkBuddy 网关层给模型加了图片支持（原生 DeepSeek 不支持图片，但走 WorkBuddy
    能识别），但适配并不完善——实测 kimi-k3-1 能收到图片却解析不出内容。
  - 因此"图片支持"不能只看 product.json 的 supportsImages 字段，必须实测。

用法：
  python3 scripts/check_vision.py                     # 测试默认模型列表
  python3 scripts/check_vision.py --model glm-5.2     # 只测指定模型
  python3 scripts/check_vision.py --all               # 测试 /v1/models 列出的所有模型

依赖：Python3 标准库（urllib / ssl / base64 / struct / zlib）
前置：data/token.json 存在且未过期（代理启动时自动生成）
"""

import argparse
import base64
import json
import re
import struct
import ssl
import sys
import urllib.error
import urllib.request
import zlib
from pathlib import Path

# ---------------------------------------------------------------------------
# 配置
# ---------------------------------------------------------------------------

UPSTREAM = "https://copilot.tencent.com/v2/chat/completions"

# 默认测试的模型（pi 配置里的 8 个）
DEFAULT_MODELS = [
    "deepseek-v4-pro",
    "deepseek-v4-flash",
    "glm-5.2",
    "glm-5.1",
    "glm-5v-turbo",
    "hy3",
    "kimi-k3-1",
    "kimi-k2.7",
    "kimi-k2.6",
    "minimax-m3",
]

# ---------------------------------------------------------------------------
# token / 请求
# ---------------------------------------------------------------------------

def _load_token() -> str:
    path = Path(__file__).resolve().parent.parent / "data" / "token.json"
    if not path.exists():
        print(f"[错误] 找不到 {path}，请先启动代理生成 token")
        sys.exit(1)
    with open(path) as f:
        d = json.load(f)
    token = d.get("access_token", "")
    if not token:
        print("[错误] token.json 中 access_token 为空")
        sys.exit(1)
    return token

def _claims(token: str) -> dict:
    payload_b64 = token.split(".")[1]
    payload_b64 += "=" * (-len(payload_b64) % 4)
    return json.loads(base64.urlsafe_b64decode(payload_b64))

def _make_ssl_ctx():
    ctx = ssl.create_default_context()
    ctx.check_hostname = False
    ctx.verify_mode = ssl.CERT_NONE
    return ctx

def _build_headers(token: str, claims: dict) -> dict:
    domain = claims.get("iss", "").replace("https://", "").split("/")[0]
    return {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {token}",
        "X-User-Id": claims.get("sub", ""),
        "X-Enterprise-Id": "",
        "X-Tenant-Id": "",
        "X-Domain": domain,
        "Accept": "text/event-stream",
        "X-IDE-Name": "CodeBuddyIDE",
        "X-Product-Version": "4.8.1",
    }

def make_test_png(size: int = 100) -> str:
    """生成左红右绿两色图（默认 100x100，够大保证所有模型都能识别），返回 base64

    注意：不要用 2x2 四色图——实测对极小图片（2x2）解析能力不同：
    deepseek/glm/minimax 能看，hy3/kimi 需更大图。用 100x100 更符合真实场景。
    """
    def chunk(t, d):
        c = t + d
        return struct.pack(">I", len(d)) + c + struct.pack(">I", zlib.crc32(c) & 0xffffffff)

    # 左半红右半绿
    pixels = [(255, 0, 0) if x < size // 2 else (0, 255, 0)
              for y in range(size) for x in range(size)]
    raw = b""
    for y in range(size):
        raw += b"\x00"
        for x in range(size):
            raw += bytes(pixels[y * size + x])
    ihdr = struct.pack(">IIBBBBB", size, size, 8, 2, 0, 0, 0)
    png = (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr)
           + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))
    return base64.b64encode(png).decode()


# 干净测试的提问：不泄漏任何颜色信息（防止模型顺着提示词瞎编）
_CLEAN_QUESTION = "请描述这张图片"
# 真正看到图的标志：回复里同时出现红、绿（左红右绿两色图）
_MULTI_COLOR = ("红", "绿")
# 没看到图片的标志
_NOT_SEEN = (
    "没有收到任何图片", "看不到", "没有收到图片", "未收到图片", "无法识别图片",
    "没有上传", "无法加载", "图片似乎没有", "看不到实际", "没有成功上传",
    "can't see", "cannot see", "no image", "not see", "did not receive",
)

# ---------------------------------------------------------------------------
# 测试单个模型
# ---------------------------------------------------------------------------

def test_model(model: str, token: str, claims: dict, png_b64: str) -> dict:
    body = {
        "model": model,
        "stream": True,
        "messages": [
            {
                "role": "user",
                "content": [
                    {"type": "text", "text": _CLEAN_QUESTION},
                    {"type": "image_url", "image_url": {"url": f"data:image/png;base64,{png_b64}"}},
                ],
            }
        ],
    }
    req = urllib.request.Request(
        UPSTREAM,
        data=json.dumps(body).encode(),
        headers=_build_headers(token, claims),
    )
    ctx = _make_ssl_ctx()
    try:
        resp = urllib.request.urlopen(req, timeout=30, context=ctx)
        data = resp.read().decode()
        contents = re.findall(r'"content":"([^"]*)"', data)
        reply = "".join(contents).strip()
        return {"model": model, "ok": True, "reply": reply[:200], "status": resp.status}
    except urllib.error.HTTPError as e:
        err = e.read().decode()[:200]
        return {"model": model, "ok": False, "reply": f"HTTP {e.code}: {err}", "status": e.code}
    except Exception as e:
        return {"model": model, "ok": False, "reply": f"{type(e).__name__}: {str(e)[:100]}", "status": 0}

def judge(result: dict) -> str:
    """根据回复判断是否真正看到图片（干净测试，无提示词泄漏）"""
    reply = result.get("reply", "")
    if not result.get("ok"):
        return "❌ 请求失败"
    # 明确说没看到图片
    if any(h in reply for h in _NOT_SEEN):
        return "❌ 收到图但看不到内容"
    # 真正看到四色图：必须同时识别出红绿蓝多个颜色（防止瞎猜一个颜色）
    found = sum(1 for c in _MULTI_COLOR if c in reply)
    if found >= 2:
        return "✅ 支持图片（识别出多色）"
    # 只猜了一个颜色 → 不可靠
    if found == 1 or any(c in reply for c in ("黑", "白", "灰")):
        return "❌ 疑似看不到图片（只猜单一颜色）"
    return "❓ 回复无法判断"

# ---------------------------------------------------------------------------
# 主流程
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(description="检测 WorkBuddy 模型的图片支持")
    parser.add_argument("--model", help="只测试指定模型（可重复）")
    parser.add_argument("--all", action="store_true", help="测试 /v1/models 列出的所有模型")
    args = parser.parse_args()

    token = _load_token()
    claims = _claims(token)
    png_b64 = make_test_png()

    if args.model:
        models = [args.model]
    elif args.all:
        # 从本地代理的 /v1/models 拿模型列表（需要代理在跑）
        try:
            import urllib.request as ur
            with ur.urlopen("http://127.0.0.1:19090/v1/models", timeout=5) as r:
                data = json.load(r)
            models = [m["id"] for m in data.get("data", []) if m.get("id")]
            models = [m for m in models if not any(x in m for x in ("codewise", "completion", "default"))]
        except Exception:
            print("[错误] --all 需要本地代理在 19090 端口运行")
            sys.exit(1)
    else:
        models = DEFAULT_MODELS

    print(f"测试 {len(models)} 个模型的图片支持（2x2 四色图）...\n")
    results = []
    for m in models:
        r = test_model(m, token, claims, png_b64)
        results.append(r)
        print(f"  {m:22s} {judge(r)}")
        if r["reply"]:
            print(f"      回复: {r['reply'][:80]}")

    # 汇总
    ok = [r for r in results if judge(r).startswith("✅")]
    print(f"\n===== 汇总: {len(ok)}/{len(results)} 支持图片 =====")
    print("支持图片:", ", ".join(r["model"] for r in results if judge(r).startswith("✅")) or "(无)")
    print("不支持:", ", ".join(r["model"] for r in results if judge(r).startswith("❌")) or "(无)")
    print("存疑:", ", ".join(r["model"] for r in results if judge(r).startswith("❓")) or "(无)")

if __name__ == "__main__":
    main()
