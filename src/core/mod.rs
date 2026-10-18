//! 业务核心：定位 / 内容 / 原子写入 / diff / revert / ops。

pub mod atomic;
pub mod content;
pub mod diff;
pub mod edit;
pub mod location;
pub mod ops;
pub mod read;
pub mod revert;
pub mod transaction;

#[cfg(test)]
mod contract_tests;
