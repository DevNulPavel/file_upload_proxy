use crate::error::{ErrorWithStatusAndDesc, WrapErrorWithStatusAndDesc};
use hyper::{
    body::Body as BodyStruct,
    http::{header, Method, StatusCode},
    Response,
};
use lazy_static::lazy_static;
use prometheus::{
    exponential_buckets, gather, register_histogram_vec, register_int_counter, register_int_counter_vec, Encoder, HistogramTimer,
    HistogramVec, IntCounter, IntCounterVec, TextEncoder,
};

lazy_static! {
    /// Сколько всего было запросов с самого начала работы
    static ref TOTAL_REQUESTS_COUNT: IntCounter = register_int_counter!("total_http_requests", "Total HTTP requests count").unwrap();

    /// Распределение возвращаемых кодов по каждому пути и методу
    static ref HTTP_RETURN_CODES: IntCounterVec = register_int_counter_vec!(
        "http_return_codes",
        "Return codes for all HTTP requests",
        &["api_path", "method", "status_code"]
    )
    .unwrap();

    /// Гистограма распределения времени на запрос
    pub static ref HTTP_RESPONSE_TIME_SECONDS: HistogramVec = register_histogram_vec!(
        "http_response_time_seconds",
        "HTTP response times",
        &["api_path", "method"],
        vec![
            0.0005, 0.0008, 0.00085, 0.0009, 0.00095, 0.001,
            0.00105, 0.0011, 0.00115, 0.0012, 0.0015,
            0.002, 0.003, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0,
            1.5, 2.0, 2.5, 3.0, 5.0, 6.0, 8.0, 10.0
        ]
    )
    .unwrap();

    /// Суммарный объем выгруженных данных
    static ref TOTAL_BYTES_UPLOADED: IntCounter = register_int_counter!("total_bytes_uploaded", "Total bytes uploaded to google storrage").unwrap();

    /// Гистограма распределения объема выгружаемых данных
    pub static ref TOTAL_BYTES_UPLOADED_SIZE: HistogramVec = register_histogram_vec!(
            "total_bytes_uploaded_size",
            "Google storage upload size histogram",
            &["success"],
            {
                let mut v = vec![0.0f64];
                v.extend(exponential_buckets(1.0, 1024.0, 6).unwrap());
                v
            }
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

/// Подсчет распределения времени выполнения запроса
pub fn count_request_time(api_path: &str, method: &Method) -> HistogramTimer {
    HTTP_RESPONSE_TIME_SECONDS
        .with_label_values(&[api_path, method.as_str()])
        .start_timer()
}

/// Подсчитываем выгруженные данные в Google Storage
pub fn count_uploaded_size(data_size: u64, success: bool) {
    // Общий объем данных
    TOTAL_BYTES_UPLOADED.inc_by(data_size);

    // Распределение на отдельную выгрузку
    let status = if success { "ok" } else { "fail" };
    TOTAL_BYTES_UPLOADED_SIZE.with_label_values(&[status]).observe(data_size as f64);
}

/// Обработчик отдачи статистики для Prometheus
pub async fn prometheus_metrics() -> Result<Response<BodyStruct>, ErrorWithStatusAndDesc> {
    // Получаем данные из Prometheus
    let metric_families = gather();

    // Кодируем в байты
    let mut buffer = vec![];
    let encoder = TextEncoder::new();
    encoder
        .encode(&metric_families, &mut buffer)
        .wrap_err_with_500_desc("Prometheus text encoding failed".into())?;

    // Создаем ответ
    let resp = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime::TEXT_PLAIN.essence_str())
        .body(BodyStruct::from(buffer))
        .wrap_err_with_500()?;

    Ok(resp)
}
