use hyper::http::{Method, StatusCode};
use lazy_static::lazy_static;
use prometheus::{self, register_int_counter, register_int_counter_vec, IntCounter, IntCounterVec};

lazy_static! {
    static ref TOTAL_REQUESTS_COUNT: IntCounter = register_int_counter!("total_http_requests", "Total HTTP requests count").unwrap();
    static ref HTTP_RETURN_CODES: IntCounterVec = register_int_counter_vec!(
        "http_return_codes",
        "Return codes for all HTTP requests",
        &["api_path", "method", "status_code"]
    )
    .unwrap();
}

/// Подсчитываем количество успешных и фейловых кодов при работе отгрузчика на основе статуса
pub fn count_response_status(api_path: &str, method: &Method, status: &StatusCode) {
    HTTP_RETURN_CODES
        .with_label_values(&[api_path, method.as_str(), status.as_str()])
        .inc();
}

/// Подсчет общего количества запросов
pub fn count_request() {
    TOTAL_REQUESTS_COUNT.inc();
}
