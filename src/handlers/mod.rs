mod file_upload;
mod prometheus_metrics;

use self::{file_upload::file_upload, prometheus_metrics::prometheus_metrics};
use crate::{
    error::{ErrorWithStatusAndDesc, WrapErrorWithStatusAndDesc},
    types::App,
};
use hyper::{
    body::Body as BodyStruct,
    http::{method::Method, status::StatusCode},
    Request, Response,
};
use tracing::{error, info};

// Трассировка настраивается уровнем выше
// #[instrument(level = "error")]
pub async fn handle_request(app: &App, req: Request<BodyStruct>, request_id: &str) -> Result<Response<BodyStruct>, ErrorWithStatusAndDesc> {
    // debug!("Request processing begin");
    info!("Full request info: {:?}", req);

    let method = req.method();
    let path = req.uri().path().trim_end_matches('/');
    match (method, path) {
        // Выгружаем данные в Cloud
        (&Method::POST, "/upload_file") => file_upload(app, req, request_id).await,

        // Работоспособность сервиса
        (&Method::GET, "/health") => {
            // Пустой ответ со статусом 200
            let resp = hyper::Response::builder()
                .status(StatusCode::OK)
                .body(BodyStruct::empty())
                .wrap_err_with_500()?;
            Ok(resp)
        }

        // Запрашиваем метрики для Prometheus
        (&Method::GET, "/prometheus_metrics") => prometheus_metrics().await,

        // Любой другой запрос
        _ => {
            error!("Invalid request");
            Err(ErrorWithStatusAndDesc::new_with_status_desc(
                StatusCode::BAD_REQUEST,
                "Wrong path or method".into(),
            ))
        }
    }
}
