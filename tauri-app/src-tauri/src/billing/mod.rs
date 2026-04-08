pub mod clinical_features;
pub mod diagnostic_codes;
pub mod ohip_codes;
pub mod rule_engine;
pub mod time_tracking;
pub mod types;

pub use clinical_features::ClinicalFeatures;
pub use rule_engine::{map_features_to_billing, map_features_to_billing_with_context, RuleEngineContext};
pub use time_tracking::{calculate_daily_caps, calculate_direct_care_time, calculate_monthly_caps};
pub use types::{
    BillingCode, BillingConfidence, BillingDaySummary, BillingMonthSummary, BillingRecord,
    BillingStatus, CapWarning, DailyCapStatus, MonthlyCapStatus, TimeEntry,
};
