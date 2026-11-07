//! Who is speaking, from the audio level each microphone packet carries (RFC 6464: -dBov, 0 is the loudest).
//!
//! A person counts as speaking while a packet louder than the threshold arrived in the last moments; the short hold keeps
//! a highlight from flickering between words.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Quieter than this (dBov) is room noise, not a voice.
pub const THRESHOLD: i8 = -50;
/// How long a voice stays "speaking" after its last loud packet.
pub const HOLD: Duration = Duration::from_millis(700);

#[derive(Default)]
pub struct Speakers {
    heard: HashMap<u64, Instant>,
}

impl Speakers {
    /// A microphone packet of `connection` arrived with this level.
    pub fn hear(&mut self, connection: u64, level: i8, now: Instant) {
        if level >= THRESHOLD {
            self.heard.insert(connection, now);
        }
    }

    pub fn forget(&mut self, connection: u64) {
        self.heard.remove(&connection);
    }

    /// The connections speaking at `now`, sorted. Those whose hold ran out are dropped.
    pub fn active(&mut self, now: Instant) -> Vec<u64> {
        self.heard.retain(|_, at| now.saturating_duration_since(*at) < HOLD);
        let mut ids: Vec<u64> = self.heard.keys().copied().collect();
        ids.sort_unstable();
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_voice_counts_for_a_moment_then_stops() {
        let start = Instant::now();
        let mut speakers = Speakers::default();
        speakers.hear(1, -20, start);
        speakers.hear(2, -70, start); // too quiet
        assert_eq!(speakers.active(start), vec![1]);
        assert_eq!(speakers.active(start + Duration::from_millis(500)), vec![1]);
        assert!(speakers.active(start + HOLD + Duration::from_millis(1)).is_empty());
    }

    #[test]
    fn a_new_packet_extends_it_and_leaving_ends_it() {
        let start = Instant::now();
        let mut speakers = Speakers::default();
        speakers.hear(3, -30, start);
        speakers.hear(3, -30, start + Duration::from_millis(600));
        assert_eq!(speakers.active(start + Duration::from_millis(1100)), vec![3]);
        speakers.forget(3);
        assert!(speakers.active(start + Duration::from_millis(1100)).is_empty());
    }
}
