//! Randomized HLC invariants: tick/observe monotonicity, causal advancement,
//! no regression after a remote merge, and agreement with an independent
//! specification oracle. Includes the documented saturation edge case.

use poie_federation::hlc::HlcClock;
use poie_state::record::Hlc;

const SEED: u64 = 0x706f_6965_686c_6300; // "poiehlc"

#[derive(Clone, Debug)]
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> SplitMix64 {
        SplitMix64 { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn index(&mut self, bound: usize) -> usize {
        (self.next_u64() % bound as u64) as usize
    }

    fn range_i64(&mut self, low: i64, high: i64) -> i64 {
        low + (self.next_u64() % (high - low + 1) as u64) as i64
    }
}

fn order(hlc: Hlc) -> (i64, u32) {
    (hlc.physical_ms, hlc.logical)
}

#[derive(Clone, Debug)]
struct Oracle {
    last: Hlc,
}

impl Oracle {
    fn new(physical_ms: i64) -> Oracle {
        Oracle { last: Hlc::new(physical_ms, 0) }
    }

    fn tick(&mut self, physical_ms: i64) -> Hlc {
        self.last = if physical_ms > self.last.physical_ms {
            Hlc::new(physical_ms, 0)
        } else {
            Hlc::new(self.last.physical_ms, self.last.logical.saturating_add(1))
        };
        self.last
    }

    fn observe(&mut self, remote: &Hlc, physical_ms: i64) -> Hlc {
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

#[test]
fn randomized_ticks_and_observations_match_the_hlc_specification() {
    let mut rng = SplitMix64::new(SEED);
    for trial in 0..80 {
        let start = rng.range_i64(0, 5_000);
        let mut clock = HlcClock::new(start);
        let mut oracle = Oracle::new(start);
        let mut previous = clock.last();
        let mut remote_clock = HlcClock::new(rng.range_i64(0, 5_000));
        for step in 0..60 {
            let now = rng.range_i64(0, 10_000);
            let expect_decrease_allowed = order(previous) == (i64::MAX, u32::MAX); // never in these ranges
            if rng.index(3) == 0 {
                let remote = remote_clock.last();
                let observed = clock.observe(&remote, now);
                let expected = oracle.observe(&remote, now);
                assert_eq!(observed, expected, "trial {trial}/{step}: observe disagrees with the oracle");
                assert_eq!(clock.last(), expected, "trial {trial}/{step}: observe state disagrees");
                assert!(order(observed) > order(remote), "trial {trial}/{step}: observe did not exceed the remote HLC");
                assert!(order(observed) > order(previous), "trial {trial}/{step}: observe went backwards");
                assert!(observed.physical_ms >= now, "trial {trial}/{step}: observe is behind wall time");
                assert!(!expect_decrease_allowed);
                previous = observed;
            } else {
                let ticked = clock.tick(now);
                let expected = oracle.tick(now);
                assert_eq!(ticked, expected, "trial {trial}/{step}: tick disagrees with the oracle");
                assert_eq!(clock.last(), expected, "trial {trial}/{step}: tick state disagrees");
                assert!(order(ticked) > order(previous), "trial {trial}/{step}: tick went backwards");
                assert!(ticked.physical_ms >= now, "trial {trial}/{step}: tick is behind wall time");
                previous = ticked;
            }
            assert_eq!(clock.last(), oracle.last, "trial {trial}/{step}: clock diverged from the oracle");
            remote_clock.tick(rng.range_i64(0, 10_000));
        }

        // Reproducibility: the same operation sequence yields the same clock.
        let mut replay = HlcClock::new(start);
        let mut replay_oracle = Oracle::new(start);
        for step in 0..60 {
            let now = rng.range_i64(0, 10_000);
            if step % 3 == 0 {
                let remote = Hlc::new(now.saturating_sub(1), 2);
                replay.observe(&remote, now);
                replay_oracle.observe(&remote, now);
            } else {
                replay.tick(now);
                replay_oracle.tick(now);
            }
            assert_eq!(replay.last(), replay_oracle.last, "trial {trial}/{step}: replay diverged");
        }
    }
}

#[test]
fn logical_clock_saturates_instead_of_wrapping() {
    let mut clock = HlcClock::new(1_000);
    let remote = Hlc::new(1_000, u32::MAX);
    let observed = clock.observe(&remote, 1_000);
    assert_eq!(observed, Hlc::new(1_000, u32::MAX), "saturation must not wrap the logical counter");
    let ticked = clock.tick(500);
    assert_eq!(ticked, Hlc::new(1_000, u32::MAX), "saturated tick must not wrap the logical counter");
}
