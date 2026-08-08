/**
 * PM2 进程守护配置（Rust 版）
 *
 * 前置：已安装 Node.js，且 npm i -g pm2
 * 构建：cargo build --release（产物: target/release/wb-proxy）
 *
 * 启动：pm2 start ecosystem.config.cjs
 * 停止：pm2 stop workbuddy-proxy
 * 日志：pm2 logs workbuddy-proxy
 */
const isWin = process.platform === "win32";

module.exports = {
  apps: [{
    name: "workbuddy-proxy",
    cwd: __dirname,
    script: isWin ? "wb-proxy.exe" : "./target/release/wb-proxy",
    interpreter: "none", // 原生二进制，无需解释器
    instances: 1,
    exec_mode: "fork",
    autorestart: true,
    watch: false,
    max_restarts: 50,
    min_uptime: "5s",
    restart_delay: 3000,
    max_memory_restart: "256M", // Rust 版内存占用远低于 Python 版
    env: {
      RUST_LOG: "info",
    },
  }],
};
