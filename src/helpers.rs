use eyre::{ContextCompat, WrapErr};
use hyper::{
    body::Body as BodyStruct,
    http::{header, HeaderMap, StatusCode},
    Response,
};
use mime::Mime;

pub fn get_content_length(headers: &HeaderMap) -> Result<Option<usize>, eyre::Error> {
    let content_length: Option<usize> = match headers.get(header::CONTENT_LENGTH) {
        Some(val) => {
            let num = val
                .to_str()
                .wrap_err("Content-Length string convert failed")?
                .parse::<usize>()
                .wrap_err("Content Length parse failed")?;
            Some(num)
        }
        None => None,
    };
    Ok(content_length)
}

pub fn get_content_type(headers: &HeaderMap) -> Result<Option<Mime>, eyre::Error> {
    let header_val = match headers.get(header::CONTENT_TYPE) {
        Some(val) => val,
        None => return Ok(None),
    };
    let content_type_mime: Mime = header_val
        .to_str()
        .wrap_err("Content type header to string convert failed")?
        .parse()
        .wrap_err("Mime parse failed")?;
    Ok(Some(content_type_mime))
}

/// Получаем произвольный header и парсим в строку
pub fn get_str_header<'a>(headers: &'a HeaderMap, key: &str) -> Result<Option<&'a str>, eyre::Error> {
    let header_val = match headers.get(key) {
        Some(val) => val,
        None => return Ok(None),
    };

    let val = header_val
        .to_str()
        .wrap_err_with(|| format!("Header {} to string convert failed", key))?;

    Ok(Some(val))
}

/// Получаем произвольный header и парсим в строку
pub fn get_required_str_header<'a>(headers: &'a HeaderMap, key: &str) -> Result<&'a str, eyre::Error> {
    let header_val = headers.get(key).wrap_err_with(|| format!("Header {} is missing", key))?;

    let val = header_val
        .to_str()
        .wrap_err_with(|| format!("Header {} to string convert failed", key))?;

    Ok(val)
}

/*pub fn response_with_status_and_empty_body(status: StatusCode) -> Response<BodyStruct> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_LENGTH, 0)
        .body(BodyStruct::empty())
        .expect("Static fail response create failed") // Статически создаем ответ, здесь не критично
}*/

pub fn response_with_status_and_error(status: StatusCode, err_desc: &str) -> Response<BodyStruct> {
    let error_json = format!(r#"{{"desc": "{}"}}"#, err_desc);
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, mime::APPLICATION_JSON.essence_str())
        .header(header::CONTENT_LENGTH, error_json.as_bytes().len())
        .body(BodyStruct::from(error_json))
        .expect("Static fail response create failed") // Статически создаем ответ, здесь не критично
}

pub fn response_with_status_desc_and_trace_id(status: StatusCode, err_desc: &str, trace_id: &str) -> Response<BodyStruct> {
    let error_json = format!(r#"{{"request_id": "{}", "desc": "{}"}}"#, trace_id, err_desc);
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, mime::APPLICATION_JSON.essence_str())
        .header(header::CONTENT_LENGTH, error_json.as_bytes().len())
        .header(header::CONNECTION, "close")
        .body(BodyStruct::from(error_json))
        .expect("Static fail response create failed") // Статически создаем ответ, здесь не критично
}
