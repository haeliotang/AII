//! Command ownership and leases (SAP §6.1; SAP-T7, T8).
//!
//! At most one site may hold command ownership for an executor at a time. A
//! local executor is owned by its site; a remote owner must hold a lease with an
//! expiry. Lease decisions include an assumed clock-skew bound: a holder stops
//! dispatching before the lease's nominal expiry, and a skew larger than the
//! bound is detected and reported instead of being treated as a valid lease.

use std::collections::BTreeMap;
use std::fmt;

/// Assumed upper bound on clock offset between sites (SAP §6.1 rule 3; T8).
pub const ASSUMED_CLOCK_SKEW_BOUND_MS: i64 = 2_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lease {
    pub executor: String,
    pub site: String,
    pub until_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwnershipError {
    AlreadyLeased { executor: String, holder: String, until_ms: i64 },
    NotHeld { executor: String, site: String },
    LeaseExpired { executor: String, until_ms: i64, now_ms: i64 },
    ClockSkew { offset_ms: i64, bound_ms: i64 },
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
        }
    }
}

impl std::error::Error for OwnershipError {}

/// Site-local view of command ownership (SAP §6.1).
#[derive(Debug)]
pub struct OwnershipRegistry {
    leases: BTreeMap<String, Lease>,
    skew_bound_ms: i64,
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
        OwnershipRegistry { leases: BTreeMap::new(), skew_bound_ms }
    }

    /// Grants a lease. A lease may be granted when the executor is unowned or its
    /// previous lease has expired; the transfer itself is expressed by a release
    /// and a new grant written as records (SAP §6.1 rule 2).
    pub fn grant(&mut self, executor: &str, site: &str, until_ms: i64, now_ms: i64) -> Result<(), OwnershipError> {
        if let Some(existing) = self.leases.get(executor) {
            if now_ms <= existing.until_ms && existing.site != site {
                return Err(OwnershipError::AlreadyLeased {
                    executor: executor.to_string(),
                    holder: existing.site.clone(),
                    until_ms: existing.until_ms,
                });
            }
        }
        self.leases
            .insert(executor.to_string(), Lease { executor: executor.to_string(), site: site.to_string(), until_ms });
        Ok(())
    }

    /// Releases ownership held by `site`.
    pub fn release(&mut self, executor: &str, site: &str) -> Result<(), OwnershipError> {
        match self.leases.get(executor) {
            Some(lease) if lease.site == site => {
                self.leases.remove(executor);
                Ok(())
            }
            _ => Err(OwnershipError::NotHeld { executor: executor.to_string(), site: site.to_string() }),
        }
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
        registry.grant("robot-m1", "site-b", 30_000, 12_000).unwrap();
        assert!(matches!(registry.may_dispatch("robot-m1", "site-a", 12_000, 0), Err(OwnershipError::NotHeld { .. })));
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
}
