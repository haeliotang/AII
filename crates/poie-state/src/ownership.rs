//! Command ownership and leases (SAP §6.1; SAP-T7, T8, T12, T13).
//!
//! At most one site may hold command ownership for an executor at a time. A
//! local executor is owned by its site; a remote owner must hold a lease with an
//! expiry. Lease decisions include an assumed clock-skew bound `B` (each site's
//! offset from reference time): a holder stops dispatching at `until - B`, a
//! taker waits until `until + B`, and a skew larger than the bound is detected
//! and reported instead of being treated as a valid lease. Every new holder
//! gets a higher lease epoch (fencing token).

use std::collections::BTreeMap;
use std::fmt;

use crate::action::{Action, Reversibility};

/// Assumed upper bound on each site's clock offset from reference time
/// (SAP §6.1 rule 3; T8). Two sites may therefore differ by up to `2 * B`.
pub const ASSUMED_CLOCK_SKEW_BOUND_MS: i64 = 2_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lease {
    pub executor: String,
    pub site: String,
    pub until_ms: i64,
    /// Monotonic lease epoch (fencing token) for this executor (SAP §6.1).
    pub epoch: u64,
    /// The predecessor's unresolved actions are not reconciled yet: only
    /// `idempotent` actions may be dispatched (SAP §6.1 rule 4).
    pub pending_handover: bool,
}

/// What the new holder knows about the previous holder's actions on the
/// executor when it takes over an expired lease (SAP §6.1 rule 4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Handover {
    /// The predecessor's log was reconciled; these of its actions on the
    /// executor are still `dispatched` / `acknowledged` / `outcome_unknown`.
    Reconciled { unresolved: Vec<String> },
    /// The predecessor's log is not reachable.
    Unreconciled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwnershipError {
    AlreadyLeased {
        executor: String,
        holder: String,
        until_ms: i64,
    },
    NotHeld {
        executor: String,
        site: String,
    },
    LeaseExpired {
        executor: String,
        until_ms: i64,
        now_ms: i64,
    },
    ClockSkew {
        offset_ms: i64,
        bound_ms: i64,
    },
    /// The previous holder's lease has not passed `until + skew_bound` yet.
    TakeoverTooEarly {
        executor: String,
        until_ms: i64,
        now_ms: i64,
        bound_ms: i64,
    },
    /// Another site's lease expired; it must be taken over, not granted.
    TakeoverRequired {
        executor: String,
        holder: String,
    },
    /// The action's validity outlives this site's dispatch deadline.
    ValidityBeyondLease {
        executor: String,
        valid_until_ms: i64,
        deadline_ms: i64,
    },
    /// Taken over with unresolved predecessor actions: only idempotent
    /// actions may be dispatched until reconciled.
    PendingHandover {
        executor: String,
    },
}

impl fmt::Display for OwnershipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OwnershipError::AlreadyLeased { executor, holder, until_ms } => {
                write!(f, "executor {executor:?} is leased by {holder:?} until {until_ms}")
            }
            OwnershipError::NotHeld { executor, site } => {
                write!(f, "site {site:?} does not hold command ownership for {executor:?}")
            }
            OwnershipError::LeaseExpired { executor, until_ms, now_ms } => {
                write!(f, "lease for {executor:?} expired at {until_ms}, now {now_ms}")
            }
            OwnershipError::ClockSkew { offset_ms, bound_ms } => {
                write!(f, "clock offset {offset_ms} ms exceeds the assumed bound {bound_ms} ms")
            }
            OwnershipError::TakeoverTooEarly { executor, until_ms, now_ms, bound_ms } => write!(
                f,
                "lease for {executor:?} expires at {until_ms}; takeover needs now > until + {bound_ms}, now {now_ms}"
            ),
            OwnershipError::TakeoverRequired { executor, holder } => {
                write!(f, "lease for {executor:?} held by {holder:?} expired; use the takeover path")
            }
            OwnershipError::ValidityBeyondLease { executor, valid_until_ms, deadline_ms } => write!(
                f,
                "action valid until {valid_until_ms} outlives the dispatch deadline {deadline_ms} for {executor:?}"
            ),
            OwnershipError::PendingHandover { executor } => write!(
                f,
                "{executor:?} was taken over with unresolved predecessor actions; only idempotent actions until reconciled"
            ),
        }
    }
}

impl std::error::Error for OwnershipError {}

/// Site-local view of command ownership (SAP §6.1).
///
/// Two ways to change hands, kept apart (SAP §6.1 rules 2 and 3):
///
/// - **negotiated hand-over**: the holder releases ([`release`] /
///   [`release_with_pending`]), then another site is granted ([`grant`]) —
///   no expiry wait;
/// - **expiry takeover** ([`take_over_expired`]): without a release record,
///   only after `until + skew_bound`, and conservative while the
///   predecessor's unresolved actions are not reconciled.
///
/// Every new holder gets a higher lease epoch, the fencing token commands
/// carry so the executor can refuse a superseded owner (SAP §6.1 rule 5).
///
/// [`release`]: OwnershipRegistry::release
/// [`release_with_pending`]: OwnershipRegistry::release_with_pending
/// [`grant`]: OwnershipRegistry::grant
/// [`take_over_expired`]: OwnershipRegistry::take_over_expired
#[derive(Debug)]
pub struct OwnershipRegistry {
    leases: BTreeMap<String, Lease>,
    skew_bound_ms: i64,
    /// Highest epoch ever issued per executor.
    epochs: BTreeMap<String, u64>,
    /// Unresolved actions listed in the last release record per executor.
    released_pending: BTreeMap<String, Vec<String>>,
}

impl Default for OwnershipRegistry {
    fn default() -> Self {
        OwnershipRegistry::new()
    }
}

impl OwnershipRegistry {
    pub fn new() -> OwnershipRegistry {
        OwnershipRegistry::with_skew_bound(ASSUMED_CLOCK_SKEW_BOUND_MS)
    }

    pub fn with_skew_bound(skew_bound_ms: i64) -> OwnershipRegistry {
        OwnershipRegistry {
            leases: BTreeMap::new(),
            skew_bound_ms,
            epochs: BTreeMap::new(),
            released_pending: BTreeMap::new(),
        }
    }

    pub fn skew_bound_ms(&self) -> i64 {
        self.skew_bound_ms
    }

    pub fn lease(&self, executor: &str) -> Option<&Lease> {
        self.leases.get(executor)
    }

    fn next_epoch(&mut self, executor: &str) -> u64 {
        let epoch = self.epochs.entry(executor.to_string()).or_insert(0);
        *epoch += 1;
        *epoch
    }

    /// Negotiated grant (SAP §6.1 rule 2) or renewal by the current holder.
    ///
    /// Granting to another site requires the executor to be unowned or
    /// released. A live lease of another site is `AlreadyLeased`; an expired
    /// one is `TakeoverRequired` — expiry is a different path
    /// ([`OwnershipRegistry::take_over_expired`]). If the release record listed
    /// unresolved actions, the new lease starts restricted (rule 4).
    pub fn grant(&mut self, executor: &str, site: &str, until_ms: i64, now_ms: i64) -> Result<(), OwnershipError> {
        if let Some(existing) = self.leases.get_mut(executor) {
            if existing.site == site {
                existing.until_ms = until_ms;
                return Ok(());
            }
            if now_ms > existing.until_ms + self.skew_bound_ms {
                return Err(OwnershipError::TakeoverRequired {
                    executor: executor.to_string(),
                    holder: existing.site.clone(),
                });
            }
            return Err(OwnershipError::AlreadyLeased {
                executor: executor.to_string(),
                holder: existing.site.clone(),
                until_ms: existing.until_ms,
            });
        }
        let pending = self.released_pending.remove(executor).is_some_and(|pending| !pending.is_empty());
        let epoch = self.next_epoch(executor);
        self.leases.insert(
            executor.to_string(),
            Lease {
                executor: executor.to_string(),
                site: site.to_string(),
                until_ms,
                epoch,
                pending_handover: pending,
            },
        );
        Ok(())
    }

    /// Releases ownership held by `site` with no unresolved actions on the
    /// executor (SAP §6.1 rule 2).
    pub fn release(&mut self, executor: &str, site: &str) -> Result<(), OwnershipError> {
        self.release_with_pending(executor, site, Vec::new())
    }

    /// Releases ownership; the release record lists the holder's actions on the
    /// executor that are still `dispatched` / `acknowledged` /
    /// `outcome_unknown`. The receiver starts restricted until they are
    /// reconciled (SAP §6.1 rules 2 and 4).
    pub fn release_with_pending(
        &mut self,
        executor: &str,
        site: &str,
        pending: Vec<String>,
    ) -> Result<(), OwnershipError> {
        match self.leases.get(executor) {
            Some(lease) if lease.site == site => {
                self.leases.remove(executor);
                self.released_pending.insert(executor.to_string(), pending);
                Ok(())
            }
            _ => Err(OwnershipError::NotHeld { executor: executor.to_string(), site: site.to_string() }),
        }
    }

    /// Expiry takeover (SAP §6.1 rule 3). Allowed only once the taker's own
    /// clock is past `until + skew_bound`, so that with both clocks within the
    /// bound the holder (which stops at `until - skew_bound`) and the taker
    /// never overlap. Returns the new lease epoch.
    ///
    /// Unless the predecessor's log was reconciled and none of its actions on
    /// the executor is unresolved, the new lease is restricted to `idempotent`
    /// actions until [`OwnershipRegistry::mark_reconciled`] (rule 4).
    pub fn take_over_expired(
        &mut self,
        executor: &str,
        site: &str,
        until_ms: i64,
        now_ms: i64,
        handover: Handover,
    ) -> Result<u64, OwnershipError> {
        match self.leases.get(executor) {
            None => {
                self.grant(executor, site, until_ms, now_ms)?;
                return Ok(self.leases[executor].epoch);
            }
            Some(existing) if existing.site == site => {
                self.grant(executor, site, until_ms, now_ms)?;
                return Ok(self.leases[executor].epoch);
            }
            Some(existing) => {
                if now_ms <= existing.until_ms + self.skew_bound_ms {
                    return Err(OwnershipError::TakeoverTooEarly {
                        executor: executor.to_string(),
                        until_ms: existing.until_ms,
                        now_ms,
                        bound_ms: self.skew_bound_ms,
                    });
                }
            }
        }
        let pending_handover = match handover {
            Handover::Unreconciled => true,
            Handover::Reconciled { unresolved } => !unresolved.is_empty(),
        };
        let epoch = self.next_epoch(executor);
        self.released_pending.remove(executor);
        self.leases.insert(
            executor.to_string(),
            Lease { executor: executor.to_string(), site: site.to_string(), until_ms, epoch, pending_handover },
        );
        Ok(epoch)
    }

    /// Clears the conservative restriction once the predecessor's actions on
    /// the executor are reconciled and resolved (SAP §6.1 rule 4).
    pub fn mark_reconciled(&mut self, executor: &str, site: &str) -> Result<(), OwnershipError> {
        match self.leases.get_mut(executor) {
            Some(lease) if lease.site == site => {
                lease.pending_handover = false;
                Ok(())
            }
            _ => Err(OwnershipError::NotHeld { executor: executor.to_string(), site: site.to_string() }),
        }
    }

    /// The last instant `site` may dispatch to `executor`:
    /// `lease.until - skew_bound` (SAP §4 item 3, §6.1 rule 3).
    pub fn dispatch_deadline_ms(&self, executor: &str, site: &str) -> Option<i64> {
        self.leases.get(executor).filter(|lease| lease.site == site).map(|lease| lease.until_ms - self.skew_bound_ms)
    }

    /// Whether `site` may dispatch to `executor` at `now_ms`.
    ///
    /// The clock offset must be within the assumed bound; otherwise the lease is
    /// not judged valid at all (T8). The holder stops dispatching once
    /// `now + skew_bound` would pass the nominal expiry.
    pub fn may_dispatch(
        &self,
        executor: &str,
        site: &str,
        now_ms: i64,
        clock_offset_ms: i64,
    ) -> Result<(), OwnershipError> {
        if clock_offset_ms.abs() > self.skew_bound_ms {
            return Err(OwnershipError::ClockSkew { offset_ms: clock_offset_ms, bound_ms: self.skew_bound_ms });
        }
        let Some(lease) = self.leases.get(executor) else {
            return Err(OwnershipError::NotHeld { executor: executor.to_string(), site: site.to_string() });
        };
        if lease.site != site {
            return Err(OwnershipError::NotHeld { executor: executor.to_string(), site: site.to_string() });
        }
        if now_ms > lease.until_ms - self.skew_bound_ms {
            return Err(OwnershipError::LeaseExpired {
                executor: executor.to_string(),
                until_ms: lease.until_ms,
                now_ms,
            });
        }
        Ok(())
    }

    /// Whether `site` may dispatch `action` to `executor` now (SAP §4 item 3,
    /// §6.1). On success returns the lease epoch the command must carry.
    ///
    /// Beyond [`OwnershipRegistry::may_dispatch`]: the action's `valid_until`
    /// must not pass the dispatch deadline `until - skew_bound`, so a command
    /// stuck in transit expires before any successor may take over; and while
    /// a hand-over is pending, only `idempotent` actions may be dispatched.
    pub fn check_dispatch(
        &self,
        executor: &str,
        site: &str,
        now_ms: i64,
        clock_offset_ms: i64,
        action: &Action,
    ) -> Result<u64, OwnershipError> {
        self.may_dispatch(executor, site, now_ms, clock_offset_ms)?;
        let lease = &self.leases[executor];
        let deadline_ms = lease.until_ms - self.skew_bound_ms;
        if action.valid_until.0 > deadline_ms {
            return Err(OwnershipError::ValidityBeyondLease {
                executor: executor.to_string(),
                valid_until_ms: action.valid_until.0,
                deadline_ms,
            });
        }
        if lease.pending_handover && action.reversibility != Reversibility::Idempotent {
            return Err(OwnershipError::PendingHandover { executor: executor.to_string() });
        }
        Ok(lease.epoch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lease_holder_stops_before_expiry_and_a_new_holder_takes_over() {
        let mut registry = OwnershipRegistry::new();
        registry.grant("robot-m1", "site-a", 10_000, 1_000).unwrap();
        assert_eq!(registry.may_dispatch("robot-m1", "site-a", 5_000, 0), Ok(()));
        assert!(matches!(
            registry.may_dispatch("robot-m1", "site-a", 9_000, 0),
            Err(OwnershipError::LeaseExpired { .. })
        ));
        // Issue #108: a taker waits until `until + B` and uses the takeover path.
        assert!(matches!(
            registry.take_over_expired("robot-m1", "site-b", 30_000, 12_000, Handover::Unreconciled),
            Err(OwnershipError::TakeoverTooEarly { .. })
        ));
        registry.take_over_expired("robot-m1", "site-b", 30_000, 12_001, Handover::Unreconciled).unwrap();
        assert!(matches!(registry.may_dispatch("robot-m1", "site-a", 12_001, 0), Err(OwnershipError::NotHeld { .. })));
        assert_eq!(registry.may_dispatch("robot-m1", "site-b", 20_000, 0), Ok(()));
    }

    #[test]
    fn a_live_lease_cannot_be_taken_by_another_site() {
        let mut registry = OwnershipRegistry::new();
        registry.grant("robot-m1", "site-a", 10_000, 1_000).unwrap();
        assert!(matches!(
            registry.grant("robot-m1", "site-b", 20_000, 2_000),
            Err(OwnershipError::AlreadyLeased { .. })
        ));
    }

    #[test]
    fn excessive_clock_skew_is_detected_not_treated_as_valid() {
        let mut registry = OwnershipRegistry::new();
        registry.grant("robot-m1", "site-a", 10_000, 1_000).unwrap();
        assert_eq!(registry.may_dispatch("robot-m1", "site-a", 5_000, 500), Ok(()));
        assert!(matches!(
            registry.may_dispatch("robot-m1", "site-a", 5_000, 5_000),
            Err(OwnershipError::ClockSkew { .. })
        ));
    }

    #[test]
    fn a_release_with_pending_actions_restricts_the_receiver_until_reconciled() {
        let mut registry = OwnershipRegistry::new();
        registry.grant("pump-p", "site-a", 10_000, 0).unwrap();
        registry.release_with_pending("pump-p", "site-a", vec!["a-dose".to_string()]).unwrap();
        registry.grant("pump-p", "site-b", 20_000, 1_000).unwrap();
        let lease = registry.lease("pump-p").unwrap();
        assert!(lease.pending_handover);
        assert_eq!(lease.epoch, 2);
        assert_eq!(registry.dispatch_deadline_ms("pump-p", "site-b"), Some(18_000));
        registry.mark_reconciled("pump-p", "site-b").unwrap();
        assert!(!registry.lease("pump-p").unwrap().pending_handover);
    }

    #[test]
    fn renewal_keeps_the_epoch() {
        let mut registry = OwnershipRegistry::new();
        registry.grant("robot-m1", "site-a", 10_000, 0).unwrap();
        registry.grant("robot-m1", "site-a", 20_000, 5_000).unwrap();
        let lease = registry.lease("robot-m1").unwrap();
        assert_eq!((lease.epoch, lease.until_ms), (1, 20_000));
    }
}
