pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DEFAULT_SERVER_PORT: u16 = 9527;
pub const DEFAULT_REPORT_INTERVAL: u32 = 3;
pub const PROTOCOL_VERSION: u32 = 1;

pub const SESSION_TTL_SECS: i64 = 86400;
pub const HEARTBEAT_INTERVAL_SECS: u64 = 30;
pub const OFFLINE_THRESHOLD_SECS: u64 = 30;

pub const MAX_WS_MESSAGE_SIZE: usize = 1024 * 1024;
pub const MAX_TASK_OUTPUT_SIZE: usize = 512 * 1024;
pub const MAX_BINARY_FRAME_SIZE: usize = 64 * 1024;
pub const MAX_COMMAND_SIZE: usize = 8 * 1024;
pub const MAX_CONCURRENT_COMMANDS: usize = 5;
pub const MAX_TERMINAL_SESSIONS: usize = 3;
pub const TERMINAL_IDLE_TIMEOUT_SECS: u64 = 600;
pub const DEFAULT_COMMAND_TIMEOUT_SECS: u32 = 300;

pub const RECORDS_RETENTION_DAYS: u32 = 7;
pub const RECORDS_HOURLY_RETENTION_DAYS: u32 = 90;
pub const GPU_RECORDS_RETENTION_DAYS: u32 = 7;
pub const PING_RECORDS_RETENTION_DAYS: u32 = 7;
pub const AUDIT_LOGS_RETENTION_DAYS: u32 = 180;

pub const ALERT_DEBOUNCE_SECS: u64 = 300;
pub const ALERT_SAMPLE_MINUTES: u32 = 10;
pub const ALERT_TRIGGER_RATIO: f64 = 0.7;

pub const API_KEY_PREFIX: &str = "sb_";
pub const API_KEY_PREFIX_LEN: usize = 8;
