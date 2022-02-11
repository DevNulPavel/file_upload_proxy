use crate::{
    app_config::SlackConfig,
    error::{ErrorWithStatusAndDesc, WrapErrorWithStatusAndDesc},
};
use hyper::http::StatusCode;
use image::Luma;
use qrcode::QrCode;
use slack_client_lib::{
    // SlackUserMessageTarget,
    SlackChannelMessageTarget,
    SlackClient,
    SlackThreadImageTarget,
};
use tracing::Instrument;

///////////////////////////////////////////////////////////////////////////

pub fn create_qr_data(qr_text: &str) -> Result<Vec<u8>, eyre::Error> {
    // Encode some data into bits.
    let code = QrCode::new(qr_text.as_bytes())?; // Конвертация ошибки произойдет автоматически

    // Рендерим картинку
    let image_obj = code.render::<Luma<u8>>().build();

    // Ширина и высота
    let width = image_obj.width();
    let height = image_obj.height();

    // Фактический вектор с данными
    let mut png_image_data: Vec<u8> = Vec::new();

    // Создаем курсор на мутабельный вектор, курсор
    let png_image_data_cursor = std::io::Cursor::new(&mut png_image_data);

    // Создаем буффер с мутабельной ссылкой на вектор
    // Можно сразу передавать &mut png_image_data вместо курсора, но с курсором нагляднее
    let png_image_buffer = std::io::BufWriter::with_capacity(2048, png_image_data_cursor);

    // Конвертим
    image::png::PngEncoder::new(png_image_buffer).encode(&image_obj, width, height, image::ColorType::L8)?;

    Ok(png_image_data)
}

///////////////////////////////////////////////////////////////////////////

pub struct SlackLinkSender {
    client: SlackClient,
    targets: Vec<String>,
    qr_code: bool,
    default_text_before: Option<String>,
}

impl SlackLinkSender {
    pub fn new(http_client: reqwest::Client, config: SlackConfig) -> SlackLinkSender {
        let client = SlackClient::new(http_client, config.token);

        SlackLinkSender {
            client,
            targets: config.targets,
            qr_code: config.qr_code,
            default_text_before: config.default_text_before,
        }
    }

    /// Выдаем в слак нашу ссылку
    pub async fn post_link(&self, link: &str, text_prefix: Option<String>) -> Result<(), ErrorWithStatusAndDesc> {
        // Формируем текст сообщения
        let text = if let Some(mut text) = text_prefix.or_else(|| self.default_text_before.clone()) {
            text.push('<');
            text.push_str(link);
            text.push_str("|link>");
            text
        } else {
            format!("Download file url: <{link}|link>")
        };

        // Футура ожидания сообщений от всех таргетов
        let futures_iter = self.targets.iter().map(|target| {
            // debug!("Send message to target: {} -> {}", text, target);
            self.client
                .send_message(&text, SlackChannelMessageTarget::new(target))
                .in_current_span()
        });

        // Делаем запрос выгрузки в каждый таргет сообщения
        let send_results = futures::future::try_join_all(futures_iter).in_current_span().await.map_err(|err| {
            ErrorWithStatusAndDesc::new_with_status_desc(StatusCode::INTERNAL_SERVER_ERROR, format!("Slack error: {}", err).into())
        })?;

        // Отправляем QR код в треды
        if self.qr_code {
            let qr_code_image = create_qr_data(link).wrap_err_with_500_desc("QR code create failed".into())?;

            let qr_send_iter = send_results.iter().filter_map(|v| v.as_ref()).map(|message| {
                self.client
                    .send_image(
                        qr_code_image.clone(),
                        None,
                        SlackThreadImageTarget::new(message.get_channel_id(), message.get_thread_id()),
                    )
                    .in_current_span()
            });

            futures::future::try_join_all(qr_send_iter).in_current_span().await.map_err(|err| {
                ErrorWithStatusAndDesc::new_with_status_desc(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Slack qr send error: {}", err).into(),
                )
            })?;
        }

        Ok(())
    }
}
