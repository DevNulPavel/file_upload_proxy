use super::{google::GoogleUploader, slack::SlackLinkSender};
use crate::{
    app_config::ProjectConfig,
    error::{ErrorWithStatusAndDesc, WrapErrorWithStatusAndDesc},
    types::HttpClient,
};
use hyper::{
    body::Body as BodyStruct,
    http::{header, StatusCode},
    Response,
};
use tracing::Instrument;

///////////////////////////////////////////////////////////////////////////

pub struct Project {
    api_token: String,
    google_uploader: GoogleUploader,
    slack_link_sender: Option<SlackLinkSender>,
}

impl Project {
    /// Создаем объект отдельного проекта
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

    /// Сверка токена
    pub fn check_token(&self, token: &str) -> bool {
        self.api_token.eq(token)
    }

    /// Выполнение отгрузки на данном проекте
    pub async fn upload(
        &self,
        file_name: String,
        body: BodyStruct,
        link_to_slack: bool,
        slack_text_prefix: Option<String>,
        request_id: &str,
    ) -> Result<Response<BodyStruct>, ErrorWithStatusAndDesc> {
        // Заранее проверим перед выгрузкой: можем ли мы постить в слак если хотят этого?
        let slack_sender = if link_to_slack {
            if self.slack_link_sender.is_some() {
                self.slack_link_sender.as_ref()
            } else {
                return Err(ErrorWithStatusAndDesc::new_with_status_desc(
                    StatusCode::BAD_REQUEST,
                    "Slack posting is not configured for this application".into(),
                ));
            }
        } else {
            None
        };

        // Загружаем в Storage
        let download_link = self.google_uploader.upload(file_name.as_str(), body).in_current_span().await?;

        // Дублируем ссылку в Slack если нужно
        if let Some(slack) = slack_sender {
            slack.post_link(&download_link, slack_text_prefix).in_current_span().await?;
        }

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
