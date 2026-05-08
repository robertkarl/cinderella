/// MemoryMonitor — page-out rate polling and SystemHealth state machine.
///
/// A Tokio background task that polls `vm_stat` for page-out rates, receives
/// macOS memory pressure FFI events, and emits `SystemHealth` state transitions.

use crate::memory_ffi::PressureLevel;
use tokio::sync::{mpsc, watch};

// ── Constants (hardcoded conservative defaults for v1) ────────────────

/// How often to poll vm_stat (seconds).
const POLL_INTERVAL_SECS: u64 = 5;

/// Page-out delta per poll window that triggers Warning (after sustained).
const PAGEOUT_WARN_THRESHOLD: u64 = 100;

/// Page-out delta that triggers immediate Critical.
const PAGEOUT_CRITICAL_THRESHOLD: u64 = 1000;

/// Consecutive polls above warn threshold to enter Warning.
const WARN_SUSTAIN_POLLS: usize = 3;

/// Consecutive near-zero polls to recover Warning → Normal.
const RECOVERY_SUSTAIN_POLLS: usize = 6;

/// Consecutive near-zero polls to offer Promotion (~2 min at 5s poll).
const PROMOTION_SUSTAIN_POLLS: usize = 24;

/// Threshold for "near-zero" page-outs.
const NEAR_ZERO_PAGEOUTS: u64 = 5;

// ── SystemHealth enum ─────────────────────────────────────────────────

/// Current health state of the system's memory subsystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemHealth {
    Normal,
    Warning,
    Critical,
    PromotionAvailable,
}

// ── Metrics ───────────────────────────────────────────────────────────

/// Snapshot of memory metrics at a point in time.
#[derive(Debug, Clone)]
pub struct Metrics {
    pub pageout_delta: u64,
    pub swap_used_mb: f64,
    pub swap_total_mb: f64,
    pub last_tok_per_sec: Option<f64>,
    pub pressure_event: Option<PressureLevel>,
}

// ── HealthEvent ───────────────────────────────────────────────────────

/// Emitted when the system health state changes.
#[derive(Debug, Clone)]
pub struct HealthEvent {
    pub health: SystemHealth,
    pub metrics: Metrics,
}

// ── SwapUsage ─────────────────────────────────────────────────────────

/// Parsed swap usage from sysctl vm.swapusage.
#[derive(Debug, Clone, PartialEq)]
pub struct SwapUsage {
    pub total_mb: f64,
    pub used_mb: f64,
    pub free_mb: f64,
}

impl Default for SwapUsage {
    fn default() -> Self {
        Self {
            total_mb: 0.0,
            used_mb: 0.0,
            free_mb: 0.0,
        }
    }
}

// ── Parsing functions ─────────────────────────────────────────────────

/// Parse the "Pageouts:" line from `vm_stat` output.
///
/// vm_stat output looks like:
/// ```text
/// Mach Virtual Memory Statistics: (page size of 16384 bytes)
/// Pages free:                                3445.
/// Pages active:                            387541.
/// ...
/// Pageouts:                                  1234.
/// ```
pub fn parse_pageouts(output: &str) -> Option<u64> {
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Pageouts:") {
            // Extract the number, stripping trailing period and whitespace
            let value_part = trimmed.strip_prefix("Pageouts:")?;
            let cleaned = value_part.trim().trim_end_matches('.');
            return cleaned.parse::<u64>().ok();
        }
    }
    None
}

/// Parse `sysctl vm.swapusage` output.
///
/// Format: "total = 6144.00M  used = 1024.00M  free = 5120.00M"
pub fn parse_swap_usage(output: &str) -> SwapUsage {
    fn extract_mb(s: &str, key: &str) -> f64 {
        s.split(key)
            .nth(1)
            .and_then(|after| {
                let after = after.trim().trim_start_matches('=').trim();
                // Take until 'M' or end
                let num_str: String = after
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '.')
                    .collect();
                num_str.parse::<f64>().ok()
            })
            .unwrap_or(0.0)
    }

    SwapUsage {
        total_mb: extract_mb(output, "total"),
        used_mb: extract_mb(output, "used"),
        free_mb: extract_mb(output, "free"),
    }
}

// ── HealthStateMachine ────────────────────────────────────────────────

/// State machine that tracks memory health and emits transitions.
pub struct HealthStateMachine {
    state: SystemHealth,
    on_smaller_model: bool,
    /// Counter for consecutive polls above warn threshold (for Normal → Warning).
    warn_count: usize,
    /// Counter for consecutive near-zero polls (for recovery / promotion).
    calm_count: usize,
}

impl HealthStateMachine {
    pub fn new() -> Self {
        Self {
            state: SystemHealth::Normal,
            on_smaller_model: false,
            warn_count: 0,
            calm_count: 0,
        }
    }

    /// Return the current health state.
    pub fn current(&self) -> SystemHealth {
        self.state
    }

    /// Set whether we're currently running on a smaller (non-best) model.
    pub fn set_on_smaller_model(&mut self, on_smaller: bool) {
        self.on_smaller_model = on_smaller;
    }

    /// Reset state machine to Normal (called after a model swap completes
    /// or when a promotion is dismissed).
    pub fn reset_to_normal(&mut self) {
        self.state = SystemHealth::Normal;
        self.warn_count = 0;
        self.calm_count = 0;
    }

    /// Core state transition logic. Returns `Some(HealthEvent)` on transitions.
    pub fn update(&mut self, metrics: Metrics) -> Option<HealthEvent> {
        let prev = self.state;

        // ── Priority 1: CRITICAL pressure event from any state ────────
        if let Some(PressureLevel::Critical) = metrics.pressure_event {
            self.state = SystemHealth::Critical;
            self.warn_count = 0;
            self.calm_count = 0;
            if prev != SystemHealth::Critical {
                return Some(HealthEvent {
                    health: self.state,
                    metrics,
                });
            }
            return None;
        }

        // ── Priority 2: pageout_delta >= CRITICAL_THRESHOLD ───────────
        if metrics.pageout_delta >= PAGEOUT_CRITICAL_THRESHOLD {
            self.state = SystemHealth::Critical;
            self.warn_count = 0;
            self.calm_count = 0;
            if prev != SystemHealth::Critical {
                return Some(HealthEvent {
                    health: self.state,
                    metrics,
                });
            }
            return None;
        }

        // ── State-specific logic ──────────────────────────────────────
        let is_near_zero = metrics.pageout_delta <= NEAR_ZERO_PAGEOUTS;
        let is_above_warn = metrics.pageout_delta >= PAGEOUT_WARN_THRESHOLD;

        match self.state {
            SystemHealth::Normal => {
                if is_above_warn {
                    self.warn_count += 1;
                    self.calm_count = 0;
                    if self.warn_count >= WARN_SUSTAIN_POLLS {
                        self.state = SystemHealth::Warning;
                        self.warn_count = 0;
                        return Some(HealthEvent {
                            health: self.state,
                            metrics,
                        });
                    }
                } else if is_near_zero && self.on_smaller_model {
                    self.warn_count = 0;
                    self.calm_count += 1;
                    if self.calm_count >= PROMOTION_SUSTAIN_POLLS {
                        self.state = SystemHealth::PromotionAvailable;
                        self.calm_count = 0;
                        return Some(HealthEvent {
                            health: self.state,
                            metrics,
                        });
                    }
                } else {
                    // Between thresholds — reset both counters
                    self.warn_count = 0;
                    self.calm_count = 0;
                }
                None
            }
            SystemHealth::Warning => {
                if is_near_zero {
                    self.calm_count += 1;
                    if self.calm_count >= RECOVERY_SUSTAIN_POLLS {
                        self.state = SystemHealth::Normal;
                        self.calm_count = 0;
                        return Some(HealthEvent {
                            health: self.state,
                            metrics,
                        });
                    }
                } else {
                    self.calm_count = 0;
                }
                None
            }
            // Critical and PromotionAvailable only transition via reset_to_normal()
            SystemHealth::Critical | SystemHealth::PromotionAvailable => None,
        }
    }
}

// ── run() — Tokio background task ─────────────────────────────────────

/// Run the memory monitor background task.
///
/// Polls `vm_stat` and `sysctl vm.swapusage` on a timer, and listens for
/// macOS memory pressure FFI events. Emits `HealthEvent` on state transitions.
pub async fn run(
    health_tx: watch::Sender<Option<HealthEvent>>,
    tok_per_sec_rx: watch::Receiver<Option<f64>>,
    mut pressure_rx: mpsc::UnboundedReceiver<PressureLevel>,
    on_smaller_model: bool,
) {
    let mut machine = HealthStateMachine::new();
    machine.set_on_smaller_model(on_smaller_model);
    let mut prev_pageouts: Option<u64> = None;
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(POLL_INTERVAL_SECS));

    loop {
        tokio::select! {
            _ = interval.tick() => {
                // Poll vm_stat
                let vm_stat_output = match tokio::process::Command::new("vm_stat")
                    .output()
                    .await
                {
                    Ok(out) => String::from_utf8_lossy(&out.stdout).to_string(),
                    Err(_) => continue,
                };

                // Poll sysctl vm.swapusage
                let swap_output = match tokio::process::Command::new("sysctl")
                    .arg("-n")
                    .arg("vm.swapusage")
                    .output()
                    .await
                {
                    Ok(out) => String::from_utf8_lossy(&out.stdout).to_string(),
                    Err(_) => String::new(),
                };

                let current_pageouts = parse_pageouts(&vm_stat_output).unwrap_or(0);
                let pageout_delta = match prev_pageouts {
                    Some(prev) => current_pageouts.saturating_sub(prev),
                    None => 0, // First poll, no delta yet
                };
                prev_pageouts = Some(current_pageouts);

                let swap = parse_swap_usage(&swap_output);
                let tok_per_sec = *tok_per_sec_rx.borrow();

                let metrics = Metrics {
                    pageout_delta,
                    swap_used_mb: swap.used_mb,
                    swap_total_mb: swap.total_mb,
                    last_tok_per_sec: tok_per_sec,
                    pressure_event: None,
                };

                if let Some(event) = machine.update(metrics) {
                    let _ = health_tx.send(Some(event));
                }
            }

            Some(pressure) = pressure_rx.recv() => {
                let tok_per_sec = *tok_per_sec_rx.borrow();
                let metrics = Metrics {
                    pageout_delta: 0,
                    swap_used_mb: 0.0,
                    swap_total_mb: 0.0,
                    last_tok_per_sec: tok_per_sec,
                    pressure_event: Some(pressure),
                };

                if let Some(event) = machine.update(metrics) {
                    let _ = health_tx.send(Some(event));
                }
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Sample outputs for parsing tests ──────────────────────────────

    const VM_STAT_OUTPUT: &str = "\
Mach Virtual Memory Statistics: (page size of 16384 bytes)
Pages free:                                3445.
Pages active:                            387541.
Pages inactive:                          385041.
Pages speculative:                          832.
Pages throttled:                              0.
Pages wired down:                         97057.
Pages purgeable:                          11763.
\"Translation faults\":                 478440877.
Pages copy-on-write:                   15786498.
Pages zero filled:                    197aboroso886.
Pages reactivated:                      1653100.
Pages purged:                            662498.
File-backed pages:                       172484.
Anonymous pages:                         600930.
Pages stored in compressor:              629498.
Pages occupied by compressor:            195473.
Decompressions:                         7598245.
Compressions:                          10458032.
Pageins:                               11023857.
Pageouts:                                  1234.
Swapins:                                      0.
Swapouts:                                     0.";

    const VM_STAT_NO_PAGEOUTS: &str = "\
Mach Virtual Memory Statistics: (page size of 16384 bytes)
Pages free:                                3445.
Pages active:                            387541.";

    const SYSCTL_SWAP: &str = "total = 6144.00M  used = 1024.00M  free = 5120.00M";

    // ── parse_pageouts tests ──────────────────────────────────────────

    #[test]
    fn parse_pageouts_real_output() {
        assert_eq!(parse_pageouts(VM_STAT_OUTPUT), Some(1234));
    }

    #[test]
    fn parse_pageouts_missing_line() {
        assert_eq!(parse_pageouts(VM_STAT_NO_PAGEOUTS), None);
    }

    #[test]
    fn parse_pageouts_empty() {
        assert_eq!(parse_pageouts(""), None);
    }

    // ── parse_swap_usage tests ────────────────────────────────────────

    #[test]
    fn parse_swap_usage_real_output() {
        let swap = parse_swap_usage(SYSCTL_SWAP);
        assert_eq!(swap.total_mb, 6144.0);
        assert_eq!(swap.used_mb, 1024.0);
        assert_eq!(swap.free_mb, 5120.0);
    }

    #[test]
    fn parse_swap_usage_empty() {
        let swap = parse_swap_usage("");
        assert_eq!(swap.total_mb, 0.0);
        assert_eq!(swap.used_mb, 0.0);
        assert_eq!(swap.free_mb, 0.0);
    }

    // ── Helper to build a Metrics snapshot ────────────────────────────

    fn metrics(pageout_delta: u64) -> Metrics {
        Metrics {
            pageout_delta,
            swap_used_mb: 0.0,
            swap_total_mb: 0.0,
            last_tok_per_sec: None,
            pressure_event: None,
        }
    }

    fn metrics_with_pressure(pressure: PressureLevel) -> Metrics {
        Metrics {
            pageout_delta: 0,
            swap_used_mb: 0.0,
            swap_total_mb: 0.0,
            last_tok_per_sec: None,
            pressure_event: Some(pressure),
        }
    }

    // ── State machine tests ───────────────────────────────────────────

    #[test]
    fn normal_to_warning_sustained_pageouts() {
        let mut sm = HealthStateMachine::new();
        assert_eq!(sm.current(), SystemHealth::Normal);

        // First two polls above threshold: no transition yet
        assert!(sm.update(metrics(150)).is_none());
        assert!(sm.update(metrics(200)).is_none());
        assert_eq!(sm.current(), SystemHealth::Normal);

        // Third consecutive poll above threshold: transitions to Warning
        let event = sm.update(metrics(150));
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.health, SystemHealth::Warning);
        assert_eq!(sm.current(), SystemHealth::Warning);
    }

    #[test]
    fn warning_to_critical_pressure_event() {
        let mut sm = HealthStateMachine::new();
        // Get to Warning first
        for _ in 0..WARN_SUSTAIN_POLLS {
            sm.update(metrics(150));
        }
        assert_eq!(sm.current(), SystemHealth::Warning);

        // Critical pressure event
        let event = sm.update(metrics_with_pressure(PressureLevel::Critical));
        assert!(event.is_some());
        assert_eq!(event.unwrap().health, SystemHealth::Critical);
        assert_eq!(sm.current(), SystemHealth::Critical);
    }

    #[test]
    fn warning_to_normal_recovery() {
        let mut sm = HealthStateMachine::new();
        // Get to Warning
        for _ in 0..WARN_SUSTAIN_POLLS {
            sm.update(metrics(150));
        }
        assert_eq!(sm.current(), SystemHealth::Warning);

        // First 5 near-zero polls: still Warning
        for _ in 0..(RECOVERY_SUSTAIN_POLLS - 1) {
            assert!(sm.update(metrics(0)).is_none());
        }
        assert_eq!(sm.current(), SystemHealth::Warning);

        // 6th near-zero poll: recovery to Normal
        let event = sm.update(metrics(0));
        assert!(event.is_some());
        assert_eq!(event.unwrap().health, SystemHealth::Normal);
        assert_eq!(sm.current(), SystemHealth::Normal);
    }

    #[test]
    fn normal_to_promotion_on_smaller_model() {
        let mut sm = HealthStateMachine::new();
        sm.set_on_smaller_model(true);

        // 23 near-zero polls: no promotion yet
        for _ in 0..(PROMOTION_SUSTAIN_POLLS - 1) {
            assert!(sm.update(metrics(0)).is_none());
        }
        assert_eq!(sm.current(), SystemHealth::Normal);

        // 24th near-zero poll: PromotionAvailable
        let event = sm.update(metrics(0));
        assert!(event.is_some());
        assert_eq!(event.unwrap().health, SystemHealth::PromotionAvailable);
        assert_eq!(sm.current(), SystemHealth::PromotionAvailable);
    }

    #[test]
    fn no_promotion_when_on_best_model() {
        let mut sm = HealthStateMachine::new();
        // on_smaller_model defaults to false

        for _ in 0..(PROMOTION_SUSTAIN_POLLS + 10) {
            assert!(sm.update(metrics(0)).is_none());
        }
        assert_eq!(sm.current(), SystemHealth::Normal);
    }

    #[test]
    fn critical_directly_from_normal_severe_pageouts() {
        let mut sm = HealthStateMachine::new();
        assert_eq!(sm.current(), SystemHealth::Normal);

        let event = sm.update(metrics(1500));
        assert!(event.is_some());
        assert_eq!(event.unwrap().health, SystemHealth::Critical);
        assert_eq!(sm.current(), SystemHealth::Critical);
    }

    #[test]
    fn critical_from_pressure_event_in_normal() {
        let mut sm = HealthStateMachine::new();
        assert_eq!(sm.current(), SystemHealth::Normal);

        let event = sm.update(metrics_with_pressure(PressureLevel::Critical));
        assert!(event.is_some());
        assert_eq!(event.unwrap().health, SystemHealth::Critical);
        assert_eq!(sm.current(), SystemHealth::Critical);
    }

    #[test]
    fn critical_only_resets_via_reset_to_normal() {
        let mut sm = HealthStateMachine::new();
        // Enter Critical
        sm.update(metrics(1500));
        assert_eq!(sm.current(), SystemHealth::Critical);

        // Further updates don't transition out
        for _ in 0..20 {
            assert!(sm.update(metrics(0)).is_none());
        }
        assert_eq!(sm.current(), SystemHealth::Critical);

        // Manual reset
        sm.reset_to_normal();
        assert_eq!(sm.current(), SystemHealth::Normal);
    }

    #[test]
    fn promotion_only_resets_via_reset_to_normal() {
        let mut sm = HealthStateMachine::new();
        sm.set_on_smaller_model(true);

        // Enter PromotionAvailable
        for _ in 0..PROMOTION_SUSTAIN_POLLS {
            sm.update(metrics(0));
        }
        assert_eq!(sm.current(), SystemHealth::PromotionAvailable);

        // Further updates don't transition out
        for _ in 0..20 {
            assert!(sm.update(metrics(0)).is_none());
        }
        assert_eq!(sm.current(), SystemHealth::PromotionAvailable);

        sm.reset_to_normal();
        assert_eq!(sm.current(), SystemHealth::Normal);
    }

    #[test]
    fn metrics_snapshot_included_in_health_event() {
        let mut sm = HealthStateMachine::new();

        let m = Metrics {
            pageout_delta: 1500,
            swap_used_mb: 512.0,
            swap_total_mb: 4096.0,
            last_tok_per_sec: Some(23.5),
            pressure_event: None,
        };

        let event = sm.update(m).unwrap();
        assert_eq!(event.health, SystemHealth::Critical);
        assert_eq!(event.metrics.pageout_delta, 1500);
        assert_eq!(event.metrics.swap_used_mb, 512.0);
        assert_eq!(event.metrics.swap_total_mb, 4096.0);
        assert_eq!(event.metrics.last_tok_per_sec, Some(23.5));
        assert!(event.metrics.pressure_event.is_none());
    }

    #[test]
    fn warn_count_resets_on_below_threshold_poll() {
        let mut sm = HealthStateMachine::new();

        // Two polls above threshold
        sm.update(metrics(150));
        sm.update(metrics(150));

        // One poll below (but not near-zero) — resets the counter
        sm.update(metrics(50));

        // Need WARN_SUSTAIN_POLLS consecutive again
        sm.update(metrics(150));
        sm.update(metrics(150));
        assert_eq!(sm.current(), SystemHealth::Normal);

        // Third consecutive
        let event = sm.update(metrics(150));
        assert!(event.is_some());
        assert_eq!(event.unwrap().health, SystemHealth::Warning);
    }

    #[test]
    fn recovery_counter_resets_on_non_near_zero() {
        let mut sm = HealthStateMachine::new();
        // Get to Warning
        for _ in 0..WARN_SUSTAIN_POLLS {
            sm.update(metrics(150));
        }
        assert_eq!(sm.current(), SystemHealth::Warning);

        // 4 near-zero polls
        for _ in 0..4 {
            sm.update(metrics(0));
        }
        // Interrupt with a non-near-zero
        sm.update(metrics(50));

        // Need RECOVERY_SUSTAIN_POLLS consecutive again
        for _ in 0..(RECOVERY_SUSTAIN_POLLS - 1) {
            assert!(sm.update(metrics(0)).is_none());
        }
        assert_eq!(sm.current(), SystemHealth::Warning);

        let event = sm.update(metrics(0));
        assert!(event.is_some());
        assert_eq!(event.unwrap().health, SystemHealth::Normal);
    }

    #[test]
    fn warn_pressure_does_not_trigger_critical() {
        let mut sm = HealthStateMachine::new();

        // A Warn-level pressure event should not trigger Critical
        let event = sm.update(metrics_with_pressure(PressureLevel::Warn));
        assert!(event.is_none());
        assert_eq!(sm.current(), SystemHealth::Normal);
    }
}
