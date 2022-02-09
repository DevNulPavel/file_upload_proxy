use crate::{
    app_config::{GoogleStorageConfig, ProjectConfig, SlackConfig},
    auth_token_provider::AuthTokenProvider,
    error::{WrapErrorWithStatusAndDesc, ErrorWithStatusAndDesc},
    prometheus::count_uploaded_size,
    types::HttpClient,
};
use eyre::WrapErr;
use hyper::{
    body::{Body as BodyStruct, aggregate, to_bytes, Buf},
    http::{
        uri::{Authority, Uri},
        StatusCode,
        method::Method,
        header,
    },
    Request, Response,
};
use serde::Deserialize;
use futures::StreamExt;
use slack_client_lib::SlackClient;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use tracing::{debug, error, Instrument};

///////////////////////////////////////////////////////////////////////////

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
    // id: String,
    name: String,
    bucket: String,
    // #[serde(rename = "selfLink")]
    // self_link: String,
    // #[serde(rename = "md5Hash")]
    // md5: String,
    // #[serde(rename = "mediaLink")]
    // link: String,
}

async fn parse_response_body(response: Response<BodyStruct>) -> Result<UploadResultData, ErrorWithStatusAndDesc> {
    let body_data = aggregate(response)
        .in_current_span()
        .await
        .wrap_err_with_status_desc(StatusCode::INTERNAL_SERVER_ERROR, "Google cloud response receive failed".into())?;

    let info = serde_json::from_reader::<_, UploadResultData>(body_data.reader())
        .wrap_err_with_status_desc(StatusCode::INTERNAL_SERVER_ERROR, "Google cloud response parsing failed".into())?;

    Ok(info)
}

struct GoogleUploader {
    http_client: HttpClient,
    token_provider: AuthTokenProvider,
    target_bucket: String,
}

impl GoogleUploader {
    fn new(http_client: HttpClient, google_config: GoogleStorageConfig) -> Result<GoogleUploader, eyre::Error> {
        // Создаем провайдер для токенов
        let token_provider = AuthTokenProvider::new(
            http_client.clone(),
            &google_config.credentials_file,
            "https://www.googleapis.com/auth/devstorage.read_write",
        )
        .wrap_err("Token provider create failed")?;

        Ok(GoogleUploader {
            http_client,
            target_bucket: google_config.bucket_name,
            token_provider,
        })
    }

    async fn upload(&self, filename: &str, body: BodyStruct) -> Result<String, ErrorWithStatusAndDesc> {
        // Получаем токен для Google API
        let token = self
            .token_provider
            .get_token()
            .in_current_span()
            .await
            .wrap_err_with_status_desc(StatusCode::UNAUTHORIZED, "Google cloud token receive failed".into())?;

        // Специальный счетчик выгружаемых байт
        // Подсчитываем объем данных уже после компрессии
        let bytes_upload_counter = Arc::new(AtomicU64::new(0));
        let result_body = body.map({
            let bytes_upload_counter = bytes_upload_counter.clone();
            move |v| {
                if let Ok(data) = &v {
                    bytes_upload_counter.fetch_add(data.len() as u64, Ordering::Relaxed);
                };
                v
            }
        });

        // Адрес запроса
        let uri = build_upload_uri(&self.target_bucket, filename).wrap_err_with_500()?;
        debug!("Request uri: {}", uri);

        // Объект запроса
        let request = build_upload_request(uri, token, BodyStruct::wrap_stream(result_body)).wrap_err_with_500()?;
        debug!("Request object: {:?}", request);

        // Объект ответа
        let response = self
            .http_client
            .request(request)
            .in_current_span()
            .await
            .wrap_err_with_status_desc(StatusCode::INTERNAL_SERVER_ERROR, "Google cloud error".into())?;
        debug!("Google response: {:?}", response);

        // Статус
        let status = response.status();
        debug!("Response status: {:?}", status);

        // Обрабатываем в зависимости от ответа
        if status.is_success() {
            // Подсчет выгруженных конечных данных
            count_uploaded_size(bytes_upload_counter.load(Ordering::Acquire), true);

            // Данные парсим
            let info = parse_response_body(response).in_current_span().await?;
            debug!("Uploading result: {:?}", info);

            // Ссылка для загрузки c поддержкой проверки пермишенов на скачивание
            let download_link = format!("https://storage.cloud.google.com/{}/{}", info.bucket, info.name);

            Ok(download_link)
        } else {
            // Подсчет выгруженных конечных данных
            count_uploaded_size(bytes_upload_counter.load(Ordering::Acquire), true);

            // Данные
            let body_data = to_bytes(response)
                .in_current_span()
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
}

///////////////////////////////////////////////////////////////////////////

struct SlackLinkSender {
    client: SlackClient,
    targets: Vec<String>,
    qr_code: bool,
    text_before: Option<String>,
}

impl SlackLinkSender {
    fn new(http_client: reqwest::Client, config: SlackConfig) -> SlackLinkSender {
        let client = SlackClient::new(http_client, config.token);

        SlackLinkSender {
            client,
            targets: config.targets,
            qr_code: config.qr_code,
            text_before: config.text_before,
        }
    }
}

///////////////////////////////////////////////////////////////////////////

pub struct Project {
    api_token: String,
    google_uploader: GoogleUploader,
    slack_link_sender: Option<SlackLinkSender>,
}

impl Project {
    pub fn new(
        config: ProjectConfig,
        http_client_low_level: HttpClient,
        http_client_high_level: reqwest::Client,
    ) -> Result<Project, eyre::Error> {
        let google_uploader = GoogleUploader::new(http_client_low_level, config.google_storage_target)?;

        let slack_link_sender = config.slack_link_dub.map(|conf| SlackLinkSender::new(http_client_high_level, conf));

        Ok(Project {
            api_token: config.api_token,
            google_uploader,
            slack_link_sender,
        })
    }

    pub fn check_token(&self, token: &str) -> bool {
        self.api_token.eq(token)
    }

    pub async fn upload(&self, file_name: String, body: BodyStruct, request_id: &str) -> Result<Response<BodyStruct>, ErrorWithStatusAndDesc> {
        // Загружаем в Storage
        let download_link = self.google_uploader.upload(file_name.as_str(), body).in_current_span().await?;

        // Формируем ответ
        let json_text = format!(r#"{{"link": "{}", "request_id": "{}"}}"#, download_link, request_id);
        let response = Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime::APPLICATION_JSON.essence_str())
            .header(header::CONTENT_LENGTH, json_text.as_bytes().len())
            .body(BodyStruct::from(json_text))
            .wrap_err_with_500()?;

        Ok(response)
    }
}
