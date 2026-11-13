use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;

use super::budget::{Budget, RoleShares, spend_refusal};
use super::error::LlmError;
use super::usage::{LlmRole, LlmUsageSink, ROLE_COUNT, RoleSpend};
use crate::settings::SettingsOverlay;

pub mod limits;
pub mod reservation;
pub mod window;

#[cfg(test)]
pub mod reservation_tests;
#[cfg(test)]
pub mod spend_tests;
#[cfg(test)]
pub mod tests;

pub use limits::{LimitSource, LimiterLimits};
pub use reservation::Reservation;
use window::RoleWindow;

const RECHECK_CAP: Duration = Duration::from_secs(300);
const MIN_RECHECK: Duration = Duration::from_millis(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LimiterSnapshot {
    pub rate_limit_per_minute: u32,
    pub calls_this_minute: u32,
    pub ceiling_max_calls: u32,
    pub ceiling_window_secs: u64,
    pub calls_in_ceiling_window: u32,
    pub rate_refusals_total: u64,
    pub ceiling_refusals_total: u64,
    pub spend_refusals_total: u64,
    pub refusals_by_role: [u64; ROLE_COUNT],
    pub calls_this_minute_by_role: [u32; ROLE_COUNT],
    pub calls_in_ceiling_window_by_role: [u32; ROLE_COUNT],
}

struct State {
    rate: RoleWindow,
    ceiling: RoleWindow,
}

enum WindowRefusal {
    Ceiling { limit: u32, window_secs: u64 },
    Rate { limit: u32 },
}

impl State {
    fn prepare(&mut self, limits: &LimiterLimits, now: Instant) {
        self.rate.tune(limits.rate_per_window, limits.rate_window);
        self.ceiling
            .tune(limits.ceiling_max_calls, limits.ceiling_window);
        self.rate.roll_if_elapsed(now);
        self.ceiling.roll_if_elapsed(now);
    }

    fn epochs(&self) -> (u64, u64) {
        (self.rate.epoch(), self.ceiling.epoch())
    }

    fn charge(&mut self, role: LlmRole, calls: u32) {
        self.rate.charge(role, calls);
        self.ceiling.charge(role, calls);
    }

    fn charge_one(&mut self, role: LlmRole, shares: &RoleShares) -> Result<(), WindowRefusal> {
        if self.ceiling.available(role, shares) == 0 {
            return Err(WindowRefusal::Ceiling {
                limit: self.ceiling.limit(),
                window_secs: self.ceiling.window().as_secs(),
            });
        }
        if self.rate.available(role, shares) == 0 {
            return Err(WindowRefusal::Rate {
                limit: self.rate.limit(),
            });
        }
        self.charge(role, 1);
        Ok(())
    }

    fn budget(
        &self,
        role: LlmRole,
        shares: &RoleShares,
        spend_blocked: bool,
        now: Instant,
    ) -> Budget {
        if spend_blocked {
            return Budget::exhausted(RECHECK_CAP);
        }
        let rate = self.rate.available(role, shares);
        let ceiling = self.ceiling.available(role, shares);
        let calls_now = rate.min(ceiling);
        let next_slot_in = if calls_now > 0 {
            Duration::ZERO
        } else {
            self.next_slot_in(rate == 0, ceiling == 0, now)
        };
        Budget {
            calls_now,
            window_remaining_calls: ceiling,
            next_slot_in,
        }
    }

    fn next_slot_in(&self, rate_out: bool, ceiling_out: bool, now: Instant) -> Duration {
        let rate_wait = rate_out.then(|| self.rate.remaining(now));
        let ceiling_wait = ceiling_out.then(|| self.ceiling.remaining(now));
        [rate_wait, ceiling_wait]
            .into_iter()
            .flatten()
            .max()
            .unwrap_or(Duration::ZERO)
            .clamp(MIN_RECHECK, RECHECK_CAP)
    }
}

struct RefusalCounters {
    rate: AtomicU64,
    ceiling: AtomicU64,
    spend: AtomicU64,
    by_role: [AtomicU64; ROLE_COUNT],
}

impl RefusalCounters {
    fn new() -> Self {
        Self {
            rate: AtomicU64::new(0),
            ceiling: AtomicU64::new(0),
            spend: AtomicU64::new(0),
            by_role: std::array::from_fn(|_| AtomicU64::new(0)),
        }
    }

    fn count_role(&self, role: LlmRole) {
        if let Some(slot) = self.by_role.get(role.index()) {
            slot.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn by_role_totals(&self) -> [u64; ROLE_COUNT] {
        std::array::from_fn(|i| {
            self.by_role
                .get(i)
                .map_or(0, |slot| slot.load(Ordering::Relaxed))
        })
    }
}

pub struct CallLimiter {
    source: ArcSwap<LimitSource>,
    spend: Mutex<Option<Arc<dyn LlmUsageSink>>>,
    state: Mutex<State>,
    refusals: RefusalCounters,
}

impl CallLimiter {
    #[cfg(any(test, feature = "test-seams"))]
    pub fn new(
        rate_limit_per_minute: u32,
        rate_window: Duration,
        ceiling_max_calls: u32,
        ceiling_window: Duration,
    ) -> Self {
        Self::with_limits(LimiterLimits::fixed(
            rate_limit_per_minute,
            rate_window,
            ceiling_max_calls,
            ceiling_window,
        ))
    }

    pub fn with_limits(limits: LimiterLimits) -> Self {
        let now = Instant::now();
        Self {
            source: ArcSwap::from_pointee(LimitSource::Fixed(limits)),
            spend: Mutex::new(None),
            state: Mutex::new(State {
                rate: RoleWindow::new(limits.rate_per_window, limits.rate_window, now),
                ceiling: RoleWindow::new(limits.ceiling_max_calls, limits.ceiling_window, now),
            }),
            refusals: RefusalCounters::new(),
        }
    }

    pub fn install_runtime(
        &self,
        overlay: Arc<ArcSwap<SettingsOverlay>>,
        sink: Option<Arc<dyn LlmUsageSink>>,
    ) {
        self.source.store(Arc::new(LimitSource::Overlay(overlay)));
        self.set_spend_source(sink);
    }

    pub fn set_spend_source(&self, sink: Option<Arc<dyn LlmUsageSink>>) {
        *self.spend.lock().unwrap_or_else(|e| e.into_inner()) = sink;
    }

    fn limits(&self) -> LimiterLimits {
        self.source.load().resolve()
    }

    fn lock_state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn spend_snapshot(&self) -> RoleSpend {
        let sink = self.spend.lock().unwrap_or_else(|e| e.into_inner()).clone();
        sink.map_or_else(RoleSpend::default, |s| s.spent_today())
    }

    fn spend_refusal_for(
        &self,
        role: LlmRole,
        limits: &LimiterLimits,
    ) -> Option<super::budget::SpendRefusal> {
        if limits.daily_spend_ceiling_usd <= 0.0 {
            return None;
        }
        spend_refusal(
            limits.daily_spend_ceiling_usd,
            &limits.shares,
            &self.spend_snapshot(),
            role,
        )
    }

    pub fn admit(
        &self,
        role: LlmRole,
        reservation: Option<&Reservation>,
    ) -> Result<(), LlmError> {
        self.admit_at(role, reservation, Instant::now())
    }

    fn admit_at(
        &self,
        role: LlmRole,
        reservation: Option<&Reservation>,
        now: Instant,
    ) -> Result<(), LlmError> {
        let limits = self.limits();
        if let Some(refusal) = self.spend_refusal_for(role, &limits) {
            return Err(self.refuse_spend(role, refusal));
        }
        let mut state = self.lock_state();
        state.prepare(&limits, now);
        if reservation.is_some_and(|r| r.consume(role, state.epochs())) {
            return Ok(());
        }
        let outcome = state.charge_one(role, &limits.shares);
        drop(state);
        outcome.map_err(|refusal| self.refuse_window(role, refusal))
    }

    fn refuse_spend(&self, role: LlmRole, refusal: super::budget::SpendRefusal) -> LlmError {
        self.refusals.spend.fetch_add(1, Ordering::Relaxed);
        self.refusals.count_role(role);
        tracing::debug!(
            role = role.label(),
            spent_usd = refusal.spent_usd,
            share_usd = refusal.share_usd,
            ceiling_usd = refusal.ceiling_usd,
            "llm governor refused a call: role spend share and the shared remainder are used up"
        );
        LlmError::SpendCeilingExceeded {
            spent_usd: refusal.spent_usd,
            share_usd: refusal.share_usd,
            ceiling_usd: refusal.ceiling_usd,
        }
    }

    fn refuse_window(&self, role: LlmRole, refusal: WindowRefusal) -> LlmError {
        self.refusals.count_role(role);
        match refusal {
            WindowRefusal::Ceiling { limit, window_secs } => {
                self.refuse_ceiling(role, limit, window_secs)
            }
            WindowRefusal::Rate { limit } => self.refuse_rate(role, limit),
        }
    }

    fn refuse_ceiling(&self, role: LlmRole, limit: u32, window_secs: u64) -> LlmError {
        self.refusals.ceiling.fetch_add(1, Ordering::Relaxed);
        tracing::debug!(
            role = role.label(),
            limit,
            window_secs,
            "llm governor refused a call: call ceiling exhausted for this role"
        );
        LlmError::CeilingExceeded { limit, window_secs }
    }

    fn refuse_rate(&self, role: LlmRole, limit: u32) -> LlmError {
        self.refusals.rate.fetch_add(1, Ordering::Relaxed);
        tracing::debug!(
            role = role.label(),
            limit,
            "llm governor refused a call: rate limit exhausted for this role"
        );
        LlmError::RateLimited { limit }
    }

    pub fn available(&self, role: LlmRole) -> Budget {
        self.available_at(role, Instant::now())
    }

    fn available_at(&self, role: LlmRole, now: Instant) -> Budget {
        let limits = self.limits();
        let blocked = self.spend_refusal_for(role, &limits).is_some();
        let mut state = self.lock_state();
        state.prepare(&limits, now);
        state.budget(role, &limits.shares, blocked, now)
    }

    pub fn reserve(self: &Arc<Self>, role: LlmRole, wanted: u32) -> Reservation {
        let limits = self.limits();
        let blocked = self.spend_refusal_for(role, &limits).is_some();
        let now = Instant::now();
        let mut state = self.lock_state();
        state.prepare(&limits, now);
        let budget = state.budget(role, &limits.shares, blocked, now);
        let granted = wanted.min(budget.calls_now);
        if granted > 0 {
            state.charge(role, granted);
        }
        let epochs = state.epochs();
        drop(state);
        let next_slot_in = if granted > 0 {
            Duration::ZERO
        } else {
            budget.next_slot_in
        };
        Reservation::new(Arc::clone(self), role, granted, epochs, next_slot_in)
    }

    fn release(&self, role: LlmRole, unused: u32, epochs: (u64, u64)) {
        let mut state = self.lock_state();
        state.rate.release(role, unused, epochs.0);
        state.ceiling.release(role, unused, epochs.1);
    }

    pub fn snapshot(&self) -> LimiterSnapshot {
        let limits = self.limits();
        let mut state = self.lock_state();
        state.prepare(&limits, Instant::now());
        LimiterSnapshot {
            rate_limit_per_minute: state.rate.limit(),
            calls_this_minute: state.rate.total_used(),
            ceiling_max_calls: state.ceiling.limit(),
            ceiling_window_secs: state.ceiling.window().as_secs(),
            calls_in_ceiling_window: state.ceiling.total_used(),
            rate_refusals_total: self.refusals.rate.load(Ordering::Relaxed),
            ceiling_refusals_total: self.refusals.ceiling.load(Ordering::Relaxed),
            spend_refusals_total: self.refusals.spend.load(Ordering::Relaxed),
            refusals_by_role: self.refusals.by_role_totals(),
            calls_this_minute_by_role: state.rate.used(),
            calls_in_ceiling_window_by_role: state.ceiling.used(),
        }
    }

    #[cfg(any(test, feature = "test-seams"))]
    pub fn reset(&self) {
        let now = Instant::now();
        let mut state = self.lock_state();
        state.rate.reset(now);
        state.ceiling.reset(now);
    }

    #[cfg(any(test, feature = "test-seams"))]
    pub fn exhaust_rate(&self) {
        self.lock_state().rate.exhaust();
    }
}

pub fn global_limiter() -> Arc<CallLimiter> {
    static GLOBAL: OnceLock<Arc<CallLimiter>> = OnceLock::new();
    GLOBAL
        .get_or_init(|| Arc::new(CallLimiter::with_limits(LimiterLimits::compiled_defaults())))
        .clone()
}

pub fn install_runtime(
    overlay: Arc<ArcSwap<SettingsOverlay>>,
    sink: Option<Arc<dyn LlmUsageSink>>,
) {
    global_limiter().install_runtime(overlay, sink);
}

#[cfg(any(test, feature = "test-seams"))]
pub fn reset_global_for_tests() {
    global_limiter().reset();
}

#[cfg(any(test, feature = "test-seams"))]
pub fn exhaust_global_rate_for_tests() {
    global_limiter().exhaust_rate();
}
