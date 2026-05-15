//! 退出码常量，与 PLAN.md §6.1 对齐。
//!
//! `0` 成功；`1` 用户错误；`2` 写入/IO 失败；`3` 校验失败；`4` 部分提交回滚。

/// 成功。
pub const SUCCESS: u8 = 0;

/// 用户错误：参数错误、不可逆 revert、锚点未命中、解析失败等。
pub const USAGE: u8 = 1;

/// 写入或 IO 失败：tmpfile 创建失败、目录不可写、Windows 文件锁等。
pub const IO: u8 = 2;

/// 校验失败：`--was` 不匹配、`--check` 阶段一发现问题、二进制文件拒绝等。
pub const CHECK: u8 = 3;

/// apply 阶段二部分提交后回滚（区分"全失败"vs"部分回滚"）。
pub const PARTIAL: u8 = 4;
