//! Backs off from a storage provider that has started throttling requests.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Jianguoyun lifts its temporary block after several minutes. Retrying sooner
/// only extends it, so automatic reads wait this long once throttled.
pub const RATE_LIMIT_COOLDOWN: Duration = Duration::from_secs(10 * 60);

pub struct Cooldown(Mutex<Option<Instant>>);

impl Cooldown {
    pub const fn new() -> Self {
        Self(Mutex::new(None))
    }

    pub fn trip(&self, now: Instant) {
        *self.0.lock().expect("cooldown lock") = Some(now + RATE_LIMIT_COOLDOWN);
    }

    pub fn clear(&self) {
        *self.0.lock().expect("cooldown lock") = None;
    }

    pub fn remaining(&self, now: Instant) -> Option<Duration> {
        let mut until = self.0.lock().expect("cooldown lock");
        let remaining = (*until)?
            .checked_duration_since(now)
            .filter(|remaining| !remaining.is_zero());
        if remaining.is_none() {
            *until = None;
        }
        remaining
    }
}

impl Default for Cooldown {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared by every storage operation in the process because the provider limits
/// the account, not an individual operation.
pub(super) static PROVIDER: Cooldown = Cooldown::new();

/// Time left before automatic cloud reads may resume. Explicit user actions are
/// not blocked and clear the cooldown as soon as the provider answers normally.
pub fn rate_limit_remaining() -> Option<Duration> {
    PROVIDER.remaining(Instant::now())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tripped_cooldown_lasts_ten_minutes_and_can_be_cleared() {
        let cooldown = Cooldown::new();
        let start = Instant::now();
        assert_eq!(cooldown.remaining(start), None);

        cooldown.trip(start);
        assert_eq!(cooldown.remaining(start), Some(RATE_LIMIT_COOLDOWN));
        assert_eq!(
            cooldown.remaining(start + Duration::from_secs(9 * 60)),
            Some(Duration::from_secs(60))
        );
        assert_eq!(cooldown.remaining(start + RATE_LIMIT_COOLDOWN), None);

        cooldown.trip(start);
        cooldown.clear();
        assert_eq!(cooldown.remaining(start), None);
    }
}
