mod app_arguments;
mod app_config;
mod helpers;

use self::{app_arguments::AppArguments, app_config::Config, helpers::deserialize_url};
use reqwest::{
    header::{self},
    redirect::Policy,
    Client, Method, Url,
};
use serde::Deserialize;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use structopt::StructOpt;

const PROJECT_HEADER_KEY: &str = "X-Project-Name";
const TOKEN_HEADER_KEY: &str = "X-Api-Token";

////////////////////////////////////////////////////////////////////////////////////////////////

struct RequestBuilder {
    http_client: Client,
    url: Url,
    project: String,
    token: String,
}
impl RequestBuilder {
    fn new(http_client: Client, url: Url, project: String, token: String) -> Self {
        Self {
            http_client,
            url,
            project,
            token,
        }
    }

    fn prepare_with_token(&self, method: Method, path: &str) -> reqwest::RequestBuilder {
        // Формируем url
        let url = self.url.join(path).expect("Invalid join path");

        self.http_client
            .request(method, url)
            .header(PROJECT_HEADER_KEY, self.project.clone()) // Добавляем имя проекта
            .header(TOKEN_HEADER_KEY, self.token.clone()) // Добавляем токен
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////

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

////////////////////////////////////////////////////////////////////////////////////////////////

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

//////////////////////////////////////////////////////////////////////////////////////////////////////

#[tokio::main]
async fn main() {
    let config = {
        let arguments = AppArguments::from_args();
        Config::parse_from_file(arguments.config)
    };

    let http_client = Client::builder()
        .redirect(Policy::limited(4))
        .tcp_keepalive(Duration::from_secs(180))
        .build()
        .expect("Http client build failed");

    // Обходим все указанные проекты в аргументах
    for project in config.projects {
        let request_builder = RequestBuilder::new(http_client.clone(), config.url.clone(), project.name, project.api_token);

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

        {
            let response = request_builder
                .prepare_with_token(Method::POST, "upload_file/")
                .header(header::CONTENT_TYPE, mime::TEXT_PLAIN.essence_str())
                .query(&[
                    ("slack_send", "true")
                ])
                .body("Custom test data")
                .send()
                .await
                .expect("Request execute failed");
            assert!(response.status().is_success(), "Simple POST uploading failed");

            let text = response.text().await.expect("Response receiving failed");
            println!("Response: {}", text);
            check_valid_response(&text);
        }

        {
            let response = request_builder
                .prepare_with_token(Method::POST, "upload_file/")
                .header(header::CONTENT_TYPE, mime::TEXT_PLAIN.essence_str())
                .query(&[
                    ("slack_send", "true"),
                    ("slack_text_prefix", "Custom prefix text from query:"),
                ])
                .body("Custom test data")
                .send()
                .await
                .expect("Request execute failed");
            assert!(response.status().is_success(), "Simple POST uploading failed");

            let text = response.text().await.expect("Response receiving failed");
            println!("Response: {}", text);
            check_valid_response(&text);
        }

        // Проверка статуса приложения (снаружи недоступно)
        {
            // let response = request_builder
            //     .prepare_with_token(Method::GET, "health/")
            //     .send()
            //     .await
            //     .expect("Request execute failed");
            // assert!(response.status().is_success(), "Health check failed");
        }
    }

    // TODO: Metrics, но стнаружи недоступно
}
