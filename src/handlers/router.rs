use super::file_upload::file_upload;
use crate::{error::ErrorWithStatusAndDesc, types::App};
use hyper::{
    body::Body as BodyStruct,
    http::{method::Method, status::StatusCode},
    Request, Response,
};
use tracing::{error, info, Instrument};

// Специальная обертка чтобы сообщить на уровень выше нужно ли подсчитывать данный запрос
// pub struct RequestProcessResult{
//     pub response: Response<BodyStruct>,
//     pub allow_metric_count: bool
// }
// impl From<Response<BodyStruct>> for RequestProcessResult {
//     fn from(res: Response<BodyStruct>) -> Self {
//         RequestProcessResult { response: res, allow_metric_count: true }
//     }
// }

// Трассировка настраивается уровнем выше
// #[instrument(level = "error")]
pub async fn handle_request(
    app: &App,
    path: &str,
    method: &Method,
    req: Request<BodyStruct>,
    request_id: &str,
) -> Result<Response<BodyStruct>, ErrorWithStatusAndDesc> {
    // debug!("Request processing begin");
    info!("Full request info: {:?}", req);

    // Обрабатываем путь и метод
    match (method, path) {
        // Выгружаем данные в Cloud
        (&Method::POST, "/upload_file") => file_upload(app, req, request_id).in_current_span().await.map(Into::into),

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
