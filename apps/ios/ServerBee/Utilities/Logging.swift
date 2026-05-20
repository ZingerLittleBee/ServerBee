import OSLog

/// Centralized `os.Logger` instances for the app. Use these instead of `print`
/// so that:
///   - Release builds do not spam stdout/syslog with debug-level lines.
///   - Logs are categorized in Console.app (`subsystem:com.serverbee.mobile`).
///   - Sensitive interpolations can opt into `privacy: .public` explicitly.
enum AppLog {
    private static let subsystem = "com.serverbee.mobile"

    static let ws = Logger(subsystem: subsystem, category: "ws")
    static let api = Logger(subsystem: subsystem, category: "api")
    static let auth = Logger(subsystem: subsystem, category: "auth")
    static let push = Logger(subsystem: subsystem, category: "push")
    static let ui = Logger(subsystem: subsystem, category: "ui")
    static let viewModel = Logger(subsystem: subsystem, category: "viewmodel")
}
