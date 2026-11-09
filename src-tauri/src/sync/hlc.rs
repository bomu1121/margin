//! Hybrid logical clock used to give every synced row a deterministic winner.
//!
//! The encoded form is lexicographically ordered exactly like the tuple
//! `(ms, counter, device)`, so the client can compare strings without parsing.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Hlc {
    pub ms: u64,
    pub counter: u32,
    pub device: String,
}

impl Hlc {
    pub fn encode(&self) -> String {
        format!("{:013}-{:05}-{}", self.ms, self.counter, self.device)
    }

    pub fn parse(value: &str) -> Option<Self> {
        let mut parts = value.splitn(3, '-');
        let ms = parts.next()?.parse::<u64>().ok()?;
        let counter = parts.next()?.parse::<u32>().ok()?;
        let device = parts.next()?.to_string();
        if device.is_empty() {
            return None;
        }
        Some(Self { ms, counter, device })
    }

    #[allow(dead_code)]
    pub fn newer(&self, other: &Self) -> bool {
        (self.ms, self.counter, &self.device) > (other.ms, other.counter, &other.device)
    }
}

/// A process-local clock. It never goes backwards, even if the wall clock does.
pub struct HlcClock {
    device: String,
    state: Mutex<(u64, u32)>,
}

impl HlcClock {
    pub fn new(device: String, last: Option<&Hlc>) -> Self {
        let state = last.map(|h| (h.ms, h.counter)).unwrap_or((0, 0));
        Self { device, state: Mutex::new(state) }
    }

    pub fn device(&self) -> &str {
        &self.device
    }

    pub fn now(&self) -> Hlc {
        let wall = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let mut guard = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let (ms, counter) = if wall > guard.0 {
            (wall, 0)
        } else {
            (guard.0, guard.1.saturating_add(1))
        };
        *guard = (ms, counter);
        Hlc { ms, counter, device: self.device.clone() }
    }

    /// Pull the local clock at least up to a remote event. A local edit made
    /// after observing a remote one therefore wins over it, which is what a
    /// user expects from "I changed it last".
    pub fn observe(&self, remote: &Hlc) {
        let mut guard = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if (remote.ms, remote.counter) > *guard {
            *guard = (remote.ms, remote.counter);
        }
    }

    pub fn last(&self) -> Hlc {
        let guard = self.state.lock().unwrap_or_else(|error| error.into_inner());
        Hlc { ms: guard.0, counter: guard.1, device: self.device.clone() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_round_trips_and_sorts_like_the_tuple() {
        let a = Hlc { ms: 1000, counter: 0, device: "aaa".into() };
        let b = Hlc { ms: 1000, counter: 1, device: "aaa".into() };
        let c = Hlc { ms: 1000, counter: 1, device: "bbb".into() };
        assert!(a.encode() < b.encode());
        assert!(b.encode() < c.encode());
        assert_eq!(Hlc::parse(&a.encode()).unwrap(), a);
    }

    #[test]
    fn clock_never_goes_backwards() {
        let clock = HlcClock::new("dev".into(), None);
        let first = clock.now();
        let second = clock.now();
        assert!(second.newer(&first));
        clock.observe(&Hlc { ms: first.ms + 10_000, counter: 3, device: "other".into() });
        let third = clock.now();
        assert!(third.newer(&Hlc { ms: first.ms + 10_000, counter: 3, device: "other".into() }));
    }

    #[test]
    fn parse_rejects_garbage() {
        assert!(Hlc::parse("").is_none());
        assert!(Hlc::parse("nope").is_none());
        assert!(Hlc::parse("1-2-").is_none());
    }
}
