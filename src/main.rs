mod app_arguments;
mod auth_token_provider;
mod error;
mod handlers;
mod helpers;
mod oauth2;
mod prometheus;
mod types;

use self::{
    app_arguments::AppArguments,
    auth_token_provider::AuthTokenProvider,
    handlers::handle_request,
    helpers::{response_with_status_and_error, response_with_status_desc_and_trace_id},
    prometheus::{count_request, count_request_time, count_response_status, prometheus_metrics},
    types::{App, HttpClient},
};
use error::{ErrorWithStatusAndDesc, WrapErrorWithStatusAndDesc};
use eyre::WrapErr;
use futures::FutureExt;
use hyper::{
    body::Body as BodyStruct,
    http::{Method, StatusCode},
    server::{conn::AddrStream, Server},
    service::{make_service_fn, service_fn},
    Client, Request, Response,
};
use hyper_rustls::HttpsConnector;
use std::{convert::Infallible, net::SocketAddr, process::exit, sync::Arc};
use structopt::StructOpt;
use tracing::{debug, error, Instrument};

//////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////

fn initialize_logs() -> Result<(), eyre::Error> {
    use tracing_subscriber::prelude::*;

    /*let level = match arguments.verbose {
        0 => tracing::Level::ERROR,
        1 => tracing::Level::WARN,
        2 => tracing::Level::INFO,
        3 => tracing::Level::DEBUG,
        4 => tracing::Level::TRACE,
        _ => {
            panic!("Verbose level must be in [0, 4] range");
        }
    };
    // Фильтрация на основе настроек
    let filter = tracing_subscriber::filter::LevelFilter::from_level(level);*/

    // Фильтрация на основе окружения
    let filter = tracing_subscriber::filter::EnvFilter::from_default_env();

    // Логи в stdout
    let stdoud_sub = tracing_subscriber::fmt::layer().with_writer(std::io::stdout);

    // Error layer для формирования слоя ошибки по запросу
    let error_layer = tracing_error::ErrorLayer::default();

    // Суммарный обработчик c консолью
    #[cfg(feature = "tokio-console")]
    let full_subscriber = {
        // Специальный слой для отладочной консоли tokio
        // Используем стандартные настройки для фильтрации из переменной RUST_LOG
        let console_layer = console_subscriber::ConsoleLayer::builder().with_default_env().spawn();
        tracing_subscriber::registry()
            .with(console_layer)
            .with(filter)
            .with(error_layer)
            .with(stdoud_sub)
    };

    // Суммарный обработчик без консоли
    #[cfg(not(feature = "tokio-console"))]
    let full_subscriber = tracing_subscriber::registry().with(filter).with(error_layer).with(stdoud_sub);

    // Враппер для библиотеки log
    tracing_log::LogTracer::init().wrap_err("Log wrapper create failed")?;

    // Установка по-умолчанию
    tracing::subscriber::set_global_default(full_subscriber).wrap_err("Global subscriber set failed")?;

    Ok(())
}

/// Конвертируем Result в нормальный ответ + trace_id
fn unwrap_result_to_response_with_trace_id(
    res: Result<Response<BodyStruct>, ErrorWithStatusAndDesc>,
    trace_id: &str,
) -> Response<BodyStruct> {
    match res {
        Ok(response) => response,
        Err(err) => {
            // Выводим ошибку в консоль
            error!("{}", err);

            // Ответ в виде ошибки
            response_with_status_desc_and_trace_id(err.status, &err.desc, trace_id)
        }
    }
}

/// Конвертируем Result в нормальный ответ + trace_id
fn unwrap_result_to_response(res: Result<Response<BodyStruct>, ErrorWithStatusAndDesc>) -> Response<BodyStruct> {
    match res {
        Ok(response) => response,
        Err(err) => {
            // Выводим ошибку в консоль
            error!("{}", err);

            // Ответ в виде ошибки
            response_with_status_and_error(err.status, &err.desc)
        }
    }
}

/// Непосредственно обработчик запроса без внешней мишуры
async fn process_req(app: Arc<App>, req: Request<BodyStruct>) -> Response<BodyStruct> {
    let method = req.method();
    let path = req.uri().path().trim_end_matches('/');

    // Делаем предварительный анализ с обработкой сервисных разных запросов
    // Данные запросы не требуют никаких дополнительных трассировок и тд
    match (method, path) {
        // Заранее делаем обработку метрик, чтобы не учитывать их в общей статистике
        (&Method::GET, "/prometheus_metrics") => unwrap_result_to_response(prometheus_metrics().await),

        // Работоспособность сервиса, тоже не учитываем в статистике
        (&Method::GET, "/health") => {
            // Пустой ответ со статусом 200
            let resp = hyper::Response::builder()
                .status(StatusCode::OK)
                .body(BodyStruct::empty())
                .wrap_err_with_500_desc("Empty body struct build".into());
            unwrap_result_to_response(resp)
        }

        // Все остальные пути, относящиеся к логике
        (method, path) => {
            // Создаем идентификатор трассировки для отслеживания ошибок в общих логах
            let request_id = format!("{:x}", uuid::Uuid::new_v4());

            // Создаем span с идентификатором трассировки
            let span = tracing::error_span!("request", 
                %request_id);
            let entered_span = span.enter();

            // Увеличиваем общий счетчик запросов
            count_request();

            // Начинаем подсчет времени
            let timer_guard = count_request_time(path, method);

            // Так как владение запросом передается дальше, тогда просто создадим тут копии
            // TODO: Ножно было бы развернуть запрос на содержимое и вернуть назад мета-информацию
            // Обернуть Body + заголовки в отдельную структуру, а путь и метод - по ссылке передавать в обработчик
            // Но пока обойдемся копией данных
            let path = &path.to_owned();
            let method = &method.clone();

            // Обработка сервиса
            let response = {
                // Для асинхронщины обязательно проставляем текущий span для трассиовки
                let response_res = handle_request(&app, path, method, req, &request_id).await;
                unwrap_result_to_response_with_trace_id(response_res, &request_id)
            };

            // Делаем подсчет значений статусов и запросов, но кроме получаемых метрик
            count_response_status(path, method, &response.status());

            // Фиксируем затраченное время, но можно было бы просто использовать drop
            timer_guard.observe_duration();

            drop(entered_span);
            drop(span);

            response
        }
    }
}

// Стартуем сервер
async fn run_server(app: App) -> Result<(), eyre::Error> {
    // Перемещаем в кучу для свободного доступа из разных обработчиков
    let app = Arc::new(app);

    // Адрес
    let addr = SocketAddr::from(([0, 0, 0, 0], app.app_arguments.port));

    // Обязательно создаем корневой span, чтобы не было проблем с наложением дочерних
    let root_span = tracing::trace_span!("root");

    // Сервис необходим для каждого соединения, поэтому создаем враппер, который будет генерировать наш сервис
    let make_svc = make_service_fn(move |_: &AddrStream| {
        let app = app.clone();
        let root_span = root_span.clone();
        async move {
            // Создаем сервис из функции с помощью service_fn
            Ok::<_, Infallible>(service_fn(move |req| {
                let app = app.clone();
                let root_span = root_span.clone();

                // Обработка запроса, мапим результат в infallible тип
                process_req(app, req).map(Ok::<_, Infallible>).instrument(root_span)
            }))
        }
    });

    // Создаем сервер c ожиданием завершения работы
    Server::bind(&addr)
        .serve(make_svc)
        /*.with_graceful_shutdown(async {
            // Docker уже сам умеет делать завершение работы плавное
            // https://github.com/hyperium/hyper/issues/1681
            // https://github.com/hyperium/hyper/issues/1668
            // Есть проблема с одновременным использованием клиента и сервера
            // Gracefull Shutdown сервера работает долго очень
            // Вроде как нужно просто уничтожать все объекты HTTP клиента заранее
            // Wait for the CTRL+C signal
            if let Err(err) = tokio::signal::ctrl_c().await {
                warn!("Shutdown signal awaiter setup failed, continue without: {}", err);
                // Создаем поэтому вечную future
                futures::future::pending::<()>().await;
            }
            println!("Shutdown signal received, please wait all timeouts");
        })*/
        .await
        .wrap_err("Server awaiting fail")?;

    Ok(())
}

/// Выполняем валидацию переданных аргументов приложения
fn validate_arguments(arguments: &AppArguments) -> Result<(), &str> {
    macro_rules! validate_argument {
        ($argument: expr, $desc: literal) => {
            if $argument == false {
                return Err($desc);
            }
        };
    }

    validate_argument!(arguments.google_credentials_file.exists(), "Google credential file does not exist");
    validate_argument!(arguments.google_credentials_file.is_file(), "Google credential file is not a file");
    validate_argument!(!arguments.google_bucket_name.is_empty(), "Target Google bucket can't be empty");
    Ok(())
}

fn build_http_client() -> HttpClient {
    // Коннектор для работы уже с HTTPS
    let https_connector = HttpsConnector::with_native_roots();

    // Клиент с коннектором
    let http_client = Client::builder().set_host(false).build::<_, BodyStruct>(https_connector);

    http_client
}

fn main() {
    // Бектрейсы в ошибках
    color_eyre::install().expect("Color eyre initialize failed");

    // Аргументы приложения
    let app_arguments = AppArguments::from_args();
    debug!("App arguments: {:?}", app_arguments);

    // Проверка аргументов приложения
    if let Err(err_desc) = validate_arguments(&app_arguments) {
        eprintln!("Invalid argument: {}", err_desc);
        exit(1);
    }

    // Логи
    initialize_logs().expect("Logs init");

    // Клиент для https
    let http_client = build_http_client();

    // Создаем провайдер для токенов
    let token_provider = AuthTokenProvider::new(
        http_client.clone(),
        &app_arguments.google_credentials_file,
        "https://www.googleapis.com/auth/devstorage.read_write",
    )
    .expect("Token provider create failed");

    // Создаем рантайм для работы сервера
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Tokio runtime build");

    // Контейнер со всеми менеджерами
    let app = App {
        app_arguments,
        http_client,
        token_provider,
    };

    // Стартуем сервер
    runtime.block_on(run_server(app)).expect("Server running fail");
}
