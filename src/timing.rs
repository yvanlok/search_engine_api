use std::time::{ Duration, Instant };
use serde_json::json;

#[derive(Default, Clone)]
pub struct RequestTiming {
    pub start: Option<Instant>,
    pub lemmatisation: Option<Duration>,
    pub initial_database_query: Option<Duration>,
    pub tf_idf_calculation: Option<Duration>,
    pub link_fetching: Option<Duration>,
    pub results_formatting: Option<Duration>,
    pub total_search_function: Option<Duration>,
    pub turnstile_validation: Option<Duration>,
}

pub fn format_timing_info(timing: &RequestTiming, total_request_time: Duration) -> serde_json::Value {
    json!({
        "total_request": format!("{:?}", total_request_time),
        "total_search_function": format!("{:?}", timing.total_search_function.unwrap_or_default()),
        "lemmatisation": format!("{:?}", timing.lemmatisation.unwrap_or_default()),
        "initial_database_query": format!("{:?}", timing.initial_database_query.unwrap_or_default()),
        "tf_idf_calculation": format!("{:?}", timing.tf_idf_calculation.unwrap_or_default()),
        "link_fetching": format!("{:?}", timing.link_fetching.unwrap_or_default()),
        "results_formatting": format!("{:?}", timing.results_formatting.unwrap_or_default()),
        "turnstile_validation": format!("{:?}", timing.turnstile_validation.unwrap_or_default()),
        "other_operations": format!("{:?}", total_request_time - timing.total_search_function.unwrap_or_default()),
    })
}
