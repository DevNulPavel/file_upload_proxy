use super::{service_account::ServiceAccountData, token_data::TokenData};
use crate::{
    helpers::{get_content_length, get_content_type},
    types::HttpClient,
};
use chrono::{Duration, Utc};
use eyre::WrapErr;
use hyper::{
    body::{to_bytes, Body as BodyStruct},
    http::{header, uri::Authority},
    Method, Request, Uri,
};
use mime::Mime;
use rsa::{pkcs8::FromPrivateKey, PaddingScheme, RsaPrivateKey};
use sha2::Digest;
use std::str::FromStr;
use tracing::trace;

// #[instrument(level = "error", skip(service_acc_data, scopes))]
fn build_jwt_string(service_acc_data: &ServiceAccountData, scopes: &str, duration: Duration) -> Result<String, eyre::Error> {
    // Header
    /*let jwt_header = r#"{"alg":"RS256","typ":"JWT"}"#;
    trace!(%jwt_header);
    let jwt_header = base64::encode(jwt_header);*/
    // Уже вычисленный вариант
    let jwt_header = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9";

    // Claims
    let current_time = Utc::now();
    let expire_time = current_time
        .checked_add_signed(duration)
        .ok_or_else(|| eyre::eyre!("Expire time calc err"))?;
    let jwt_claims = format!(
        r###"{{"iss":"{}","scope":"{}","aud":"{}","exp":{},"iat":{}}}"###,
        service_acc_data.client_email,
        scopes,
        service_acc_data.token_uri,
        expire_time.timestamp(),
        current_time.timestamp()
    );
    trace!(%jwt_claims);
    let jwt_claims = base64::encode(jwt_claims);

    // Исходная строка для подписи
    let jwt_string_for_signature = format!("{}.{}", jwt_header, jwt_claims);
    trace!(%jwt_string_for_signature);

    // Приватный ключ читаем
    // Вроде бы как метод шифрования записан в самом ключе, поэтому используем pkcs8 способ чтения закрытого ключа
    let private_key = RsaPrivateKey::from_pkcs8_pem(&service_acc_data.private_key).wrap_err("Private key parsing failed")?;
    private_key.validate().wrap_err("Private key is invalid")?;

    // Вычисляем подпись
    // Sign the UTF-8 representation of the input using SHA256withRSA (also known as RSASSA-PKCS1-V1_5-SIGN with the SHA-256 hash function) with the private key obtained from the Google API Console.
    let padding = PaddingScheme::new_pkcs1v15_sign(Some(rsa::Hash::SHA2_256));
    let signature = private_key
        .sign(padding, sha2::Sha256::digest(jwt_string_for_signature.as_bytes()).as_slice())
        .wrap_err("Sign failed")?;

    // Base64 подписи
    let base_64_signature = base64::encode(signature);
    trace!(%base_64_signature);

    // Результат
    Ok(format!("{}.{}", jwt_string_for_signature, base_64_signature))
}

// #[instrument(level = "error", skip(http_client, service_acc_data, scopes))]
pub async fn get_token_data(
    http_client: &HttpClient,
    service_acc_data: &ServiceAccountData,
    scopes: &str,
    duration: Duration,
) -> Result<TokenData, eyre::Error> {
    // Все обязательно кодируем в base64
    let jwt_result = build_jwt_string(service_acc_data, scopes, duration).wrap_err("JWT string create")?;
    trace!(%jwt_result);

    // Адрес запроса
    // TODO: Replace to data from service account
    let uri = Uri::builder()
        .scheme("https")
        .authority(Authority::from_str("oauth2.googleapis.com").wrap_err("Authority parse error")?)
        .path_and_query("/token")
        .build()
        .wrap_err("Uri build failed")?;
    trace!(?uri);

    // Form data - это аналог query строки, но в body
    // Значения разделяются с помощью &, каждый параметр должен быть urlencoded
    let body_data = {
        //let grand_type = urlencoding::encode("urn:ietf:params:oauth:grant-type:jwt-bearer");
        let grand_type = "urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer"; // Уже посчитанный вариант
        let assertion = urlencoding::encode(&jwt_result);
        format!("grant_type={}&assertion={}", grand_type, assertion)
    };
    trace!("Request body: {}", body_data);

    // Объект запроса
    // https://docs.aws.amazon.com/AmazonS3/latest/API/API_PutObject.html
    let request = Request::builder()
        .method(Method::POST)
        .version(hyper::Version::HTTP_2)
        .uri(uri)
        // Добавляется само если флаг выше true,
        // TODO: Что-то не так с установкой значения host, если выставить, то фейлится запрос
        // Может быть дело в регистре?
        // .header(header::HOST, "oauth2.googleapis.com")
        .header(header::CONTENT_LENGTH, body_data.len())
        .header(header::ACCEPT, mime::APPLICATION_JSON.essence_str())
        .header(header::USER_AGENT, "hyper")
        .header(header::CONTENT_TYPE, mime::APPLICATION_WWW_FORM_URLENCODED.essence_str())
        .body(BodyStruct::from(body_data))
        .wrap_err("Request build error")?;
    trace!(?request);

    /*let response = backoff::future::retry(backoff::ExponentialBackoff::default(), || {
        async move {
            match http_client.request(request).await {
                Ok(response) => {
                    if response.status().is_success() {
                        Ok(response)
                    } else {
                        warn!("Token request failed with status: {}", response.status());
                        Err(backoff::Error::Transient(eyre::eyre!("Invalid status")))
                    }
                }
                Err(err) => Err(backoff::Error::Permanent(eyre::Error::new(err))),
            }
        }
        .in_current_span()
    })
    .await
    .wrap_err("Http response error")?;
    trace!(?response);*/

    // Объект ответа
    let response = http_client.request(request).await.wrap_err("Http response error")?;
    trace!(?response);

    // Статус HTTP
    let status = response.status();

    // Получаем длину контента
    let content_length: Option<usize> = get_content_length(response.headers()).wrap_err("Content type receive err")?;
    trace!(?content_length);

    // Получаем тип контента
    let content_type_mime: Option<Mime> = get_content_type(response.headers()).wrap_err("Content type receive err")?;
    trace!(?content_type_mime);

    // Данные
    let body_data = to_bytes(response).await.wrap_err("Body data receive")?;
    trace!(?body_data);

    // В зависимости от статуса обрабатыаем иначе
    let token_data = if status.is_success() {
        // Работаем с ответом
        if let Some(content_type_mime) = content_type_mime {
            if content_type_mime.essence_str() == mime::APPLICATION_JSON.essence_str() {
                let token_data = TokenData::try_parse_from_data(&body_data).wrap_err("Body parsing failed")?;
                trace!(?token_data);
                token_data
            } else {
                return Err(eyre::eyre!("Wrong conten type: {:?}", content_type_mime));
            }
        } else {
            return Err(eyre::eyre!("Missing content type"));
        }
    } else {
        return Err(eyre::eyre!("Invalid token request"));
    };

    Ok(token_data)
}
