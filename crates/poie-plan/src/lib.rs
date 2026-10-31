//! Joint planning for production, maintenance and recovery (M8.1).
//!
//! The planner turns structural candidates plus time constraints into a
//! `proposed` action sequence with preconditions, budgets and validity windows.
//! It never dispatches: [`sap::validate_plan_through_sap`] runs every action
//! through the SAP lifecycle (proposed → validated → authorized) and the six
//! dispatch checks, so there is no path that bypasses authorization.

pub mod plan;
pub mod sap;

pub use plan::{plan, GoalFailure, Plan, PlanError, PlannedAction, PlanningGoal, PlanningRequest};
pub use sap::{validate_plan_through_sap, PlanValidationError, SapValidation};
