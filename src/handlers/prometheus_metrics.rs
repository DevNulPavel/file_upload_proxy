use crate::error::{ErrorWithStatusAndDesc, WrapErrorWithStatusAndDesc};
use hyper::{
    body::Body as BodyStruct,
    http::{header, status::StatusCode},
    Response,
};
use prometheus::{gather, Encoder, TextEncoder};

// Пока достаточно самого верхнего контекста трассировки чтобы не захламлять вывод логов
// #[instrument(level = "error", skip(app, req))]
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
