use crate::{
    error::{ErrorWithStatusAndDesc, WrapErrorWithStatusAndDesc},
    helpers::{get_content_length, get_content_type, get_required_str_header},
    types::App,
};
use async_compression::tokio::bufread::GzipEncoder;
use futures::StreamExt;
use hyper::{body::Body as BodyStruct, http::status::StatusCode, Request, Response};
use serde::Deserialize;
use tokio_util::io::{ReaderStream, StreamReader};
use tracing::{debug, info, Instrument};

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

fn gzip_body(body: BodyStruct) -> BodyStruct {
    let body_stream = body.map(|v| v.map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput)));
    let reader = StreamReader::new(body_stream);
    let compressor = GzipEncoder::new(reader);
    let out_stream = ReaderStream::new(compressor);
    BodyStruct::wrap_stream(out_stream)
}

fn build_name_and_body(req: Request<BodyStruct>, input_filename: Option<String>) -> Result<(String, BodyStruct), ErrorWithStatusAndDesc> {
    // Макрос форматирования имени
    macro_rules! format_name {
        ($format: literal) => {
            format!($format, uuid::Uuid::new_v4())
        };
    }

    // Получаем body и метаданные отдельно
    let (src_parts, src_body) = req.into_parts();

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
pub async fn file_upload(app: &App, req: Request<BodyStruct>, request_id: &str) -> Result<Response<BodyStruct>, ErrorWithStatusAndDesc> {
    info!("File uploading");

    // NGINX сейчас может добавлять заголовки при проксировании
    // X-Real-IP
    // X-Forwarded-For

    // Ищем проект
    let project = {
        // Получаем токен из запроса и проверяем
        let token = get_required_str_header(req.headers(), "X-Api-Token")
            .wrap_err_with_status_desc(StatusCode::UNAUTHORIZED, "Api token parsing failed".into())?;

        // Ищем необходимый нам проект в зависимости от переданных данных
        app
            .projects
            .get(token)
            .wrap_err_with_400_desc("Requested project is missing".into())?
    };

    // Один раз распарсим query строку
    #[derive(Debug, Deserialize, Default)]
    struct Query {
        filename: Option<String>,
        slack_send: Option<bool>,
        slack_text_prefix: Option<String>,
    }
    let Query {
        filename,
        slack_send,
        slack_text_prefix,
    } = if let Some(query_text) = req.uri().query() {
        serde_qs::from_str::<Query>(query_text).wrap_err_with_400_desc("Query parsing error".into())?
    } else {
        Default::default()
    };

    // Получаем размер данных исходных чисто для логов
    let data_length = get_content_length(req.headers())
        .wrap_err_with_status_desc(StatusCode::LENGTH_REQUIRED, "Content-Length header parsing failed".into())?
        .wrap_err_with_status_desc(StatusCode::LENGTH_REQUIRED, "Content-Length header is missing".into())?;
    debug!("Content-Length: {}", data_length);

    // В зависимости от типа контента определяем имя файла конечно и body конечного
    let (result_file_name, result_body) = build_name_and_body(req, filename)?;

    // Выполняем выгрузку c помощью указанного проекта
    project
        .upload(
            result_file_name,
            result_body,
            slack_send.unwrap_or(false),
            slack_text_prefix,
            request_id,
        )
        .in_current_span()
        .await
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    #[test]
    fn test_query_deserealize() {
        {
            #[derive(Debug, Deserialize, Default)]
            struct Query<'a> {
                filename: Option<&'a str>,
                slack_send: Option<bool>,
            }

            let text = r#"filename=test&slack_send=true"#;
            let q = serde_qs::from_str::<Query>(text).unwrap();
            assert_eq!(q.filename.unwrap(), "test");
            assert!(q.slack_send.unwrap());
        }
    }
}
