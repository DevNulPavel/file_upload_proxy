mod app_arguments;

use self::app_arguments::AppArguments;
use reqwest::{
    header::{self},
    redirect::Policy,
    Client, Method, Url,
};
use serde::{de::Error, Deserialize, Deserializer};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use structopt::StructOpt;

const TOKEN_HEADER_KEY: &str = "X-Api-Token";

struct RequestBuilder<'a> {
    arguments: &'a AppArguments,
    http_client: Client,
}
impl<'a> RequestBuilder<'a> {
    fn new(http_client: Client, arguments: &'a AppArguments) -> Self {
        Self { arguments, http_client }
    }

    fn prepare(&self, method: Method, path: &str) -> reqwest::RequestBuilder {
        let url = self.arguments.uploader_api_url.join(path).expect("Invalid join path");
        self.http_client.request(method, url)
    }

    fn prepare_with_token(&self, method: Method, path: &str) -> reqwest::RequestBuilder {
        let url = self.arguments.uploader_api_url.join(path).expect("Invalid join path");
        self.http_client
            .request(method, url)
            .header(TOKEN_HEADER_KEY, self.arguments.uploader_api_token.clone())
    }
}

fn deserialize_url<'de, D>(data: D) -> Result<Url, D::Error>
where
    D: Deserializer<'de>,
{
    let text: &str = Deserialize::deserialize(data)?;
    Url::parse(text).map_err(Error::custom)
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum Response {
    Ok {
        #[allow(dead_code)]
        #[serde(deserialize_with = "deserialize_url")]
        link: Url,
        #[allow(dead_code)]
        request_id: String,
    },
    Error {
        desc: String,
        request_id: Option<String>,
    },
}

fn check_valid_response(text: &str) {
    match serde_json::from_str::<Response>(text).expect("Simple POST: json parsing error") {
        Response::Ok { .. } => {
            //println!("Response link: {}, request_id: {}", link, request_id);
        }
        Response::Error { desc, request_id } => {
            panic!("Server error response with desc: {} request_id: {:?}", desc, request_id);
        }
    }
}

#[tokio::main]
async fn main() {
    let arguments = AppArguments::from_args();
    // println!("{:?}", arguments);

    let http_client = Client::builder()
        .redirect(Policy::limited(4))
        .tcp_keepalive(Duration::from_secs(180))
        .build()
        .expect("Http client build failed");

    let request_builder = RequestBuilder::new(http_client.clone(), &arguments);

    // Запрос должен быть с ошибкой
    {
        let response = request_builder
            .prepare_with_token(Method::GET, "upload_file/")
            .send()
            .await
            .expect("Request execute failed");
        assert!(response.status().is_client_error(), "GET request is not supported");
    }

    // Обычная выгрузка
    // Запрос должен вернуть нормальную ссылку
    {
        let test_data = b"TEST_DATA";

        let response = request_builder
            .prepare_with_token(Method::POST, "upload_file/")
            .header(header::CONTENT_TYPE, mime::TEXT_PLAIN.essence_str())
            .body(test_data.as_slice())
            .send()
            .await
            .expect("Request execute failed");
        assert!(response.status().is_success(), "Simple POST uploading failed");

        let text = response.text().await.expect("Response receiving failed");
        println!("Response: {}", text);

        check_valid_response(&text);
    }

    // Проверка указания конкретного имени через заголовок
    // Запрос должен вернуть нормальную ссылку
    {
        let response = request_builder
            .prepare_with_token(Method::POST, "upload_file/")
            .header(header::CONTENT_TYPE, mime::TEXT_PLAIN.essence_str())
            .header(
                "X-Filename",
                format!("file_{}_1.txt", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis()),
            )
            .body("Custom test data")
            .send()
            .await
            .expect("Request execute failed");
        assert!(response.status().is_success(), "Simple POST uploading failed");

        let text = response.text().await.expect("Response receiving failed");
        println!("Response: {}", text);
        check_valid_response(&text);
    }

    // Проверка указания конкретного имени через заголовок
    // Запрос должен вернуть нормальную ссылку
    {
        let time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
        let filename = format!("file_{}_2.txt", time);
        
        let response = request_builder
            .prepare_with_token(Method::POST, "upload_file/")
            .header(header::CONTENT_TYPE, mime::TEXT_PLAIN.essence_str())
            .query(&[("filename", &filename)])
            .body("Custom test data")
            .send()
            .await
            .expect("Request execute failed");
        assert!(response.status().is_success(), "Simple POST uploading failed");

        let text = response.text().await.expect("Response receiving failed");
        println!("Response: {}", text);
        check_valid_response(&text);
    }

    // TODO: Health
    // TODO: Metrics
}
