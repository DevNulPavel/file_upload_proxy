use crate::{
    error::{ErrorWithStatusAndDesc, WrapErrorWithStatusAndDesc},
    helpers::{get_content_length, get_content_type, get_str_header},
    types::App,
};
use async_compression::tokio::bufread::GzipEncoder;
use futures::StreamExt;
use hyper::{
    body::{aggregate, to_bytes, Body as BodyStruct, Buf},
    http::{
        header,
        method::Method,
        status::StatusCode,
        uri::{Authority, Uri},
    },
    Request, Response,
};
use serde::Deserialize;
use serde_json::from_reader as json_from_reader;
use tokio_util::io::{ReaderStream, StreamReader};
use tracing::{debug, error, info};

/////////////////////////////////////////////////////////////////////////////////////////////////////////

/// A wrapper around any type that implements [`Stream`](futures::Stream) to be
/// compatible with async_compression's Stream based encoders
/*#[pin_project]
#[derive(Debug)]
pub struct CompressableBody<S, E>
where
    E: std::error::Error,
    S: Stream<Item = Result<Bytes, E>>,
{
    #[pin]
    body: S,
}

impl<S, E> Stream for CompressableBody<S, E>
where
    E: std::error::Error,
    S: Stream<Item = Result<Bytes, E>>,
{
    type Item = std::io::Result<Bytes>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use std::io::{Error, ErrorKind};

        let pin = self.project();
        S::poll_next(pin.body, cx).map_err(|_| Error::from(ErrorKind::InvalidData))
    }
}
impl From<BodyStruct> for CompressableBody<BodyStruct, hyper::Error> {
    fn from(body: BodyStruct) -> Self {
        CompressableBody { body }
    }
}*/

/////////////////////////////////////////////////////////////////////////////////////////////////////////

fn build_upload_uri(bucket_name: &str, file_name: &str) -> Result<Uri, hyper::http::Error> {
    Uri::builder()
        .scheme("https")
        .authority(Authority::from_static("storage.googleapis.com"))
        .path_and_query(format!(
            "/upload/storage/v1/b/{}/o?name={}&uploadType=media&fields={}",
            urlencoding::encode(bucket_name),
            urlencoding::encode(file_name),
            urlencoding::encode("id,name,bucket,selfLink,md5Hash,mediaLink") // Только нужные поля в ответе сервера, https://cloud.google.com/storage/docs/json_api/v1/objects#resource
        ))
        .build()
}

fn build_upload_request(uri: Uri, token: String, body: BodyStruct) -> Result<Request<BodyStruct>, hyper::http::Error> {
    Request::builder()
        .method(Method::POST)
        .version(hyper::Version::HTTP_2)
        .uri(uri)
        // TODO: Что-то не так с установкой значения host, если выставить, то фейлится запрос
        // Может быть дело в регистре?
        // .header(header::HOST, "oauth2.googleapis.com")
        .header(header::USER_AGENT, "hyper")
        // .header(header::CONTENT_LENGTH, data_length)
        .header(header::ACCEPT, mime::APPLICATION_JSON.essence_str())
        .header(header::AUTHORIZATION, format!("Bearer {}", token))
        .header(header::CONTENT_TYPE, mime::APPLICATION_OCTET_STREAM.essence_str())
        .body(body)
}

// Описание
// https://cloud.google.com/storage/docs/json_api/v1/objects#resource
#[derive(Debug, Deserialize)]
struct UploadResultData {
    id: String,
    name: String,
    bucket: String,

    #[serde(rename = "selfLink")]
    self_link: String,

    #[serde(rename = "md5Hash")]
    md5: String,

    #[serde(rename = "mediaLink")]
    link: String,
}

async fn parse_response_body(response: Response<BodyStruct>) -> Result<UploadResultData, ErrorWithStatusAndDesc> {
    let body_data = aggregate(response)
        .await
        .wrap_err_with_status_desc(StatusCode::INTERNAL_SERVER_ERROR, "Google cloud response receive failed".into())?;

    let info = json_from_reader::<_, UploadResultData>(body_data.reader())
        .wrap_err_with_status_desc(StatusCode::INTERNAL_SERVER_ERROR, "Google cloud response parsing failed".into())?;

    Ok(info)
}

fn gzip_body(body: BodyStruct) -> BodyStruct {
    let body_stream = body.map(|v| v.map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput)));
    let reader = StreamReader::new(body_stream);
    let compressor = GzipEncoder::new(reader);
    let out_stream = ReaderStream::new(compressor);
    BodyStruct::wrap_stream(out_stream)
}

fn build_name_and_body(req: Request<BodyStruct>) -> Result<(String, BodyStruct), ErrorWithStatusAndDesc> {
    // Макрос форматирования имени
    macro_rules! format_name {
        ($format: literal) => {
            format!($format, uuid::Uuid::new_v4())
        };
    }

    // Получаем body и метаданные отдельно
    let (src_parts, src_body) = req.into_parts();

    // Может быть имя у нас уже передано было в запросе в Header?
    let input_filename = match get_str_header(&src_parts.headers, "X-Filename")
        .wrap_err_with_status_desc(StatusCode::BAD_REQUEST, "Filename parsing failed".into())?
    {
        // Передаем как есть
        val @ Some(_) => val.map(|v| v.to_owned()),
        None => {
            // Либо имя у нас передано в query?
            match src_parts.uri.query() {
                Some(query_str) => {
                    #[derive(Debug, Deserialize)]
                    struct Query {
                        filename: String,
                    }

                    serde_qs::from_str::<Query>(query_str).ok().map(|v| v.filename)
                }
                None => None,
            }
        }
    };

    // Получаем теперь имя
    let (name, body) = match input_filename {
        // Если имя было передано, тогда сами не сжимаем ничего, сохраняем все как есть
        // Пользователь тут лучше знает
        Some(name) => (name, src_body),
        None => {
            // Опциональный тип контента
            let content_type = get_content_type(&src_parts.headers)
                .wrap_err_with_status_desc(StatusCode::BAD_REQUEST, "Content type parsing failed".into())?;

            // Формат стандартного имени
            let default_name_gen = || format_name!("{:x}.bin.gz");

            // Создаем Body новый и генератор имени
            match content_type {
                Some(mime) => match mime.type_() {
                    // .txt file
                    mime::TEXT => (format_name!("{:x}.txt.gz"), gzip_body(src_body)),
                    // .json file
                    mime::JSON => (format_name!("{:x}.json.gz"), gzip_body(src_body)),
                    // other
                    mime::APPLICATION => match mime.subtype().as_str() {
                        // zip file уже сжатый
                        "zip" => (format_name!("{:x}.zip"), src_body),
                        // gz file уже сжатый
                        "gz" => (format_name!("{:x}.gz"), src_body),
                        // Прочие
                        _ => (default_name_gen(), gzip_body(src_body)),
                    },
                    // Прочие
                    _ => (default_name_gen(), gzip_body(src_body)),
                },
                // Прочие
                _ => (default_name_gen(), gzip_body(src_body)),
            }
        }
    };

    Ok((name, body))
}

// Пока достаточно самого верхнего контекста трассировки чтобы не захламлять вывод логов
// #[instrument(level = "error", skip(app, req))]
async fn file_upload(app: &App, req: Request<BodyStruct>, request_id: &str) -> Result<Response<BodyStruct>, ErrorWithStatusAndDesc> {
    info!("File uploading");

    // NGINX сейчас может добавлять заголовки при проксировании
    // X-Real-IP
    // X-Forwarded-For

    // Получаем токен из запроса и проверяем
    let token = req
        .headers()
        .get("X-Api-Token")
        .wrap_err_with_status_desc(StatusCode::UNAUTHORIZED, "Api token is missing".into())
        .and_then(|val| {
            std::str::from_utf8(val.as_bytes()).wrap_err_with_status_desc(StatusCode::UNAUTHORIZED, "Api token parsing failed".into())
        })?;
    if token != app.app_arguments.uploader_api_token {
        return Err(ErrorWithStatusAndDesc::new_with_status_desc(
            StatusCode::UNAUTHORIZED,
            "Invalid api token".into(),
        ));
    }

    // Получаем размер данных исходных чисто для логов
    let data_length = get_content_length(req.headers())
        .wrap_err_with_status_desc(StatusCode::LENGTH_REQUIRED, "Content-Length header parsing failed".into())?
        .wrap_err_with_status_desc(StatusCode::LENGTH_REQUIRED, "Content-Length header is missing".into())?;
    debug!("Content-Length: {}", data_length);

    // Получаем токен для Google API
    let token = app
        .token_provider
        .get_token()
        .await
        .wrap_err_with_status_desc(StatusCode::UNAUTHORIZED, "Google cloud token receive failed".into())?;

    // В зависимости от типа контента определяем имя файла конечно и body конечного
    let (result_file_name, result_body) = build_name_and_body(req)?;

    // Адрес запроса
    let uri = build_upload_uri(&app.app_arguments.google_bucket_name, &result_file_name).wrap_err_with_500()?;
    debug!("Request uri: {}", uri);

    // Объект запроса
    let request = build_upload_request(uri, token, result_body).wrap_err_with_500()?;
    debug!("Request object: {:?}", request);

    // Объект ответа
    let response = app
        .http_client
        .request(request)
        .await
        .wrap_err_with_status_desc(StatusCode::INTERNAL_SERVER_ERROR, "Google cloud error".into())?;
    debug!("Google response: {:?}", response);

    // Статус
    let status = response.status();
    debug!("Response status: {:?}", status);

    // Обрабатываем в зависимости от ответа
    if status.is_success() {
        // Данные парсим
        let info = parse_response_body(response).await?;
        debug!("Uploading result: {:?}", info);

        // Ссылка для загрузки c поддержкой проверки пермишенов на скачивание
        let download_link = format!("https://storage.cloud.google.com/{}/{}", info.bucket, info.name);

        // Формируем ответ
        let json_text = format!(r#"{{"link": "{}", "request_id": "{}"}}"#, download_link, request_id);
        let response = Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime::APPLICATION_JSON.essence_str())
            .header(header::CONTENT_LENGTH, json_text.as_bytes().len())
            .body(BodyStruct::from(json_text))
            .wrap_err_with_500()?;

        Ok(response)
    } else {
        // Данные
        let body_data = to_bytes(response)
            .await
            .wrap_err_with_status_desc(StatusCode::INTERNAL_SERVER_ERROR, "Google cloud response receive failed".into())?;
        error!("Upload fail result: {:?}", body_data);

        // Если есть внятный ответ - пробрасываем его
        match std::str::from_utf8(&body_data).ok() {
            Some(text) => {
                error!("Upload fail result text: {}", text);
                let resp = format!("Google error response: {}", text);
                Err(ErrorWithStatusAndDesc::new_with_status_desc(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    resp.into(),
                ))
            }
            None => Err(ErrorWithStatusAndDesc::new_with_status_desc(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Google uploading failed".into(),
            )),
        }
    }
}

// Трассировка настраивается уровнем выше
// #[instrument(level = "error")]
pub async fn handle_request(app: &App, req: Request<BodyStruct>, request_id: &str) -> Result<Response<BodyStruct>, ErrorWithStatusAndDesc> {
    // debug!("Request processing begin");
    info!("Full request info: {:?}", req);

    match (req.method(), req.uri().path().trim_end_matches('/')) {
        // Выгружаем данные в Cloud
        (&Method::POST, "/upload_file") => file_upload(app, req, request_id).await,

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
