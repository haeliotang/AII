//! Hybrid logical clock (M12.1).

use poie_state::record::Hlc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HlcClock {
    last: Hlc,
}

impl HlcClock {
    pub fn new(physical_ms: i64) -> HlcClock {
        HlcClock { last: Hlc::new(physical_ms, 0) }
    }

    pub fn last(&self) -> Hlc {
        self.last
    }

    /// Local event at `physical_ms`.
    pub fn tick(&mut self, physical_ms: i64) -> Hlc {
        if physical_ms > self.last.physical_ms {
            self.last = Hlc::new(physical_ms, 0);
        } else {
            self.last = Hlc::new(self.last.physical_ms, self.last.logical.saturating_add(1));
        }
        self.last
    }

    /// Receives a remote HLC and returns the updated local HLC.
    pub fn observe(&mut self, remote: &Hlc, physical_ms: i64) -> Hlc {
        let physical = physical_ms.max(self.last.physical_ms).max(remote.physical_ms);
        let logical = if physical == self.last.physical_ms && physical == remote.physical_ms {
            self.last.logical.max(remote.logical).saturating_add(1)
        } else if physical == self.last.physical_ms {
            self.last.logical.saturating_add(1)
        } else if physical == remote.physical_ms {
            remote.logical.saturating_add(1)
        } else {
            0
        };
        self.last = Hlc::new(physical, logical);
        self.last
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_ticks_are_monotone() {
        let mut clock = HlcClock::new(1_000);
        assert_eq!(clock.tick(1_000), Hlc::new(1_000, 1));
        assert_eq!(clock.tick(1_000), Hlc::new(1_000, 2));
        assert_eq!(clock.tick(2_000), Hlc::new(2_000, 0));
    }

    #[test]
    fn remote_observations_advance_the_clock() {
        let mut clock = HlcClock::new(1_000);
        let remote = Hlc::new(1_500, 3);
        assert_eq!(clock.observe(&remote, 1_200), Hlc::new(1_500, 4));
        assert_eq!(clock.observe(&remote, 1_400), Hlc::new(1_500, 5));
    }
}
