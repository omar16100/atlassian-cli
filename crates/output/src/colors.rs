use colored::Colorize;
use std::env;
use std::io::{self, IsTerminal};

/// Detects whether colors should be used based on environment variables and TTY detection
pub fn should_use_colors() -> bool {
    // NO_COLOR takes precedence (https://no-color.org/)
    if env::var("NO_COLOR").is_ok() {
        return false;
    }

    // TERM=dumb means no color support
    if env::var("TERM")
        .map(|t| t.eq_ignore_ascii_case("dumb"))
        .unwrap_or(false)
    {
        return false;
    }

    // CLICOLOR_FORCE overrides TTY detection
    if env::var("CLICOLOR_FORCE").is_ok() {
        return true;
    }

    // Auto-detect TTY
    io::stdout().is_terminal()
}

/// Status formatter with color support
pub struct StatusFormatter {
    use_colors: bool,
}

impl StatusFormatter {
    /// Create a new status formatter with color detection
    pub fn new() -> Self {
        Self {
            use_colors: should_use_colors(),
        }
    }

    /// Create a status formatter with explicit color setting (useful for testing)
    pub fn with_colors(use_colors: bool) -> Self {
        Self { use_colors }
    }

    /// Format a status string with appropriate color and icon
    pub fn format(&self, status: &str, icon: &str) -> String {
        if !self.use_colors {
            return format!("{} {}", status, icon);
        }

        let status_upper = status.to_uppercase();
        match status_upper.as_str() {
            "SUCCESSFUL" | "COMPLETED" => {
                format!("{} {}", status.green().bold(), icon)
            }
            "FAILED" | "ERROR" => {
                format!("{} {}", status.red().bold(), icon)
            }
            "IN_PROGRESS" | "RUNNING" => {
                format!("{} {}", status.yellow().bold(), icon)
            }
            "STOPPED" => {
                format!("{} {}", status.bright_red().bold(), icon)
            }
            "PENDING" | "NOT_RUN" => {
                format!("{} {}", status.cyan(), icon)
            }
            "PAUSED" => {
                format!("{} {}", status.magenta(), icon)
            }
            _ => {
                format!("{} {}", status.white(), icon)
            }
        }
    }

    /// Format just the status text without an icon
    pub fn format_status(&self, status: &str) -> String {
        if !self.use_colors {
            return status.to_string();
        }

        let status_upper = status.to_uppercase();
        match status_upper.as_str() {
            "SUCCESSFUL" | "COMPLETED" => status.green().bold().to_string(),
            "FAILED" | "ERROR" => status.red().bold().to_string(),
            "IN_PROGRESS" | "RUNNING" => status.yellow().bold().to_string(),
            "STOPPED" => status.bright_red().bold().to_string(),
            "PENDING" | "NOT_RUN" => status.cyan().to_string(),
            "PAUSED" => status.magenta().to_string(),
            _ => status.white().to_string(),
        }
    }
}

impl Default for StatusFormatter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Environment variables are process-global, so the tests below cannot run
    // concurrently: EnvGuard restores state when a test finishes, but it cannot stop a
    // sibling test from mutating the same variable mid-assertion. Two vectors existed,
    // both hitting test_clicolor_force_enables_colors, which needs NO_COLOR unset and
    // TERM not "dumb":
    //   - test_no_color_env_disables_colors and
    //     test_no_color_takes_precedence_over_clicolor_force set NO_COLOR=1
    //   - test_term_dumb_disables_colors sets TERM=dumb, which should_use_colors()
    //     checks before CLICOLOR_FORCE
    // Every test that touches the environment must take this lock first.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    // Poison-tolerant on purpose: if one env test fails while holding the lock, the
    // other three should still report their own result rather than cascade.
    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    // Helper to save and restore environment variables
    struct EnvGuard {
        vars: Vec<(String, Option<String>)>,
    }

    impl EnvGuard {
        fn new() -> Self {
            Self { vars: Vec::new() }
        }

        fn set(&mut self, key: &str, value: &str) {
            let old_value = env::var(key).ok();
            self.vars.push((key.to_string(), old_value));
            env::set_var(key, value);
        }

        fn remove(&mut self, key: &str) {
            let old_value = env::var(key).ok();
            self.vars.push((key.to_string(), old_value));
            env::remove_var(key);
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (key, value) in self.vars.iter().rev() {
                match value {
                    Some(v) => env::set_var(key, v),
                    None => env::remove_var(key),
                }
            }
        }
    }

    #[test]
    fn test_no_color_env_disables_colors() {
        let _env = env_lock();
        let mut guard = EnvGuard::new();
        guard.set("NO_COLOR", "1");
        assert!(!should_use_colors());
    }

    #[test]
    fn test_term_dumb_disables_colors() {
        let _env = env_lock();
        let mut guard = EnvGuard::new();
        guard.remove("NO_COLOR");
        guard.remove("CLICOLOR_FORCE");
        guard.set("TERM", "dumb");
        assert!(!should_use_colors());
    }

    #[test]
    fn test_clicolor_force_enables_colors() {
        let _env = env_lock();
        let mut guard = EnvGuard::new();
        guard.remove("NO_COLOR");
        guard.set("CLICOLOR_FORCE", "1");
        assert!(should_use_colors());
    }

    #[test]
    fn test_no_color_takes_precedence_over_clicolor_force() {
        let _env = env_lock();
        let mut guard = EnvGuard::new();
        guard.set("NO_COLOR", "1");
        guard.set("CLICOLOR_FORCE", "1");
        assert!(!should_use_colors());
    }

    #[test]
    fn test_formatter_successful_status() {
        let formatter = StatusFormatter::with_colors(false);
        let result = formatter.format("SUCCESSFUL", "✅");
        assert!(result.contains("SUCCESSFUL"));
        assert!(result.contains("✅"));
    }

    #[test]
    fn test_formatter_failed_status() {
        let formatter = StatusFormatter::with_colors(false);
        let result = formatter.format("FAILED", "❌");
        assert!(result.contains("FAILED"));
        assert!(result.contains("❌"));
    }

    #[test]
    fn test_formatter_in_progress_status() {
        let formatter = StatusFormatter::with_colors(false);
        let result = formatter.format("IN_PROGRESS", "🔄");
        assert!(result.contains("IN_PROGRESS"));
        assert!(result.contains("🔄"));
    }

    #[test]
    fn test_formatter_pending_status() {
        let formatter = StatusFormatter::with_colors(false);
        let result = formatter.format("PENDING", "⏳");
        assert!(result.contains("PENDING"));
        assert!(result.contains("⏳"));
    }

    #[test]
    fn test_formatter_stopped_status() {
        let formatter = StatusFormatter::with_colors(false);
        let result = formatter.format("STOPPED", "⏹");
        assert!(result.contains("STOPPED"));
        assert!(result.contains("⏹"));
    }

    #[test]
    fn test_formatter_paused_status() {
        let formatter = StatusFormatter::with_colors(false);
        let result = formatter.format("PAUSED", "⏸");
        assert!(result.contains("PAUSED"));
        assert!(result.contains("⏸"));
    }

    #[test]
    fn test_formatter_unknown_status() {
        let formatter = StatusFormatter::with_colors(false);
        let result = formatter.format("UNKNOWN", "❓");
        assert!(result.contains("UNKNOWN"));
        assert!(result.contains("❓"));
    }

    #[test]
    fn test_formatter_with_colors_enabled() {
        let formatter = StatusFormatter::with_colors(true);
        let result = formatter.format("SUCCESSFUL", "✅");
        // When colors are enabled, the result contains ANSI codes
        // We can't easily test the exact ANSI codes, but we can verify content is present
        assert!(result.contains("✅"));
    }

    #[test]
    fn test_format_status_without_icon() {
        let formatter = StatusFormatter::with_colors(false);
        let result = formatter.format_status("FAILED");
        assert_eq!(result, "FAILED");
    }

    #[test]
    fn test_formatter_case_insensitive() {
        let formatter = StatusFormatter::with_colors(false);
        let result1 = formatter.format("successful", "✅");
        let result2 = formatter.format("SUCCESSFUL", "✅");
        let result3 = formatter.format("Successful", "✅");

        assert!(result1.contains("successful"));
        assert!(result2.contains("SUCCESSFUL"));
        assert!(result3.contains("Successful"));
    }
}
