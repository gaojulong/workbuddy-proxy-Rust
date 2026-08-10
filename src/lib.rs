//! WorkBuddy Proxy 核心库
//! 拆分 lib.rs 以便单元测试引用（binary crate 无法从 tests/ 直接引用）

pub mod config;
pub mod jwt;
pub mod models;
pub mod notify;
pub mod proxy;
pub mod routes;
pub mod token;
