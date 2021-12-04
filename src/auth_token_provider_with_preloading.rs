use crate::{
    oauth2::{get_token_data, ServiceAccountData, TokenData},
    types::HttpClient,
};
use chrono::Duration as ChronoDuration;
use eyre::{Context, ContextCompat};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration as StdDuration, Instant},
};
use tokio::{sync::Mutex, task::JoinHandle};
use tracing::debug;
use tracing_log::log::warn;

////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////

#[derive(Debug)]
struct ReceivedTokenInfo {
    data: TokenData,
    expire_time: Instant,
}

impl ReceivedTokenInfo {
    async fn request(http_client: &HttpClient, account_data: &ServiceAccountData, scopes: &str) -> Result<ReceivedTokenInfo, eyre::Error> {
        // Получаем токен на основе данных
        let data = get_token_data(http_client, account_data, scopes, ChronoDuration::minutes(60))
            .await
            .wrap_err("Token receive")?;

        // Вычисляем время завершения
        let expire_time = Instant::now()
            .checked_add(StdDuration::from_secs(data.expires_in))
            .wrap_err("Invalid token expire time")?;

        Ok(ReceivedTokenInfo { data, expire_time })
    }

    fn life_duration_left(&self) -> StdDuration {
        let now = Instant::now();
        self.expire_time.saturating_duration_since(now)
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////

#[derive(Debug)]
pub struct AuthTokenProvider {
    http_client: HttpClient,
    account_data: Arc<ServiceAccountData>,
    scopes: &'static str,
    token_info: Mutex<Option<ReceivedTokenInfo>>,
    background_loading: Mutex<Option<JoinHandle<Result<ReceivedTokenInfo, eyre::Error>>>>,
}

impl AuthTokenProvider {
    pub fn new(http_client: HttpClient, service_account_json_path: &Path, scopes: &'static str) -> Result<AuthTokenProvider, eyre::Error> {
        // Прочитаем креденшиалы для гугла
        let service_acc_data = ServiceAccountData::new_from_file(service_account_json_path).wrap_err("Service account file read err")?;
        debug!("Service account data: {:?}", service_acc_data);

        Ok(AuthTokenProvider {
            http_client,
            account_data: Arc::new(service_acc_data),
            scopes,
            token_info: Mutex::new(None),
            background_loading: Mutex::new(None),
        })
    }

    fn spawn_receive_token(&self) -> JoinHandle<Result<ReceivedTokenInfo, eyre::Error>> {
        let http_client = self.http_client.clone();
        let account_data = self.account_data.clone();
        let scopes = self.scopes;
        tokio::spawn(async move { ReceivedTokenInfo::request(&http_client, account_data.as_ref(), scopes).await })
    }

    pub async fn get_token(&self) -> Result<String, eyre::Error> {
        macro_rules! update_token_or_warning {
            ($load_result: expr, $token_lock: expr, $iteration_num: expr) => {
                match $load_result {
                    Ok(new_info) => {
                        $token_lock.replace(new_info);
                    }
                    Err(err) => {
                        // Пока ограничиваемся выкидываем предупреждения, может быть повезет на новой итерации
                        warn!("Token receive iteration number {} failed with err: {}", $iteration_num, err);
                    }
                }
            };
        }

        // Блокируемся, тем самым не даем другим клиентам тоже получать токены
        let mut token_lock = self.token_info.lock().await;

        // Ограничиваемся количеством итераций, вдруг время жизни токена будет кривое приходить
        for request_iteration in 0..10 {
            // Если токен есть и не протух
            if let Some(info) = token_lock.as_ref() {
                debug!("Token info: {:?}, life left: {:?}", info, info.life_duration_left());

                // Если осталось уже меньше 10 секунд, то принудительно ждем завершения фоновой подгрузки если она есть
                // Либо стартуем блокирующую подгрузку
                if info.life_duration_left() < StdDuration::from_secs(10) {
                    debug!("Token will expire after 10 seconds");

                    let mut loading_join_lock = self.background_loading.lock().await;
                    match loading_join_lock.take() {
                        Some(join_handle) => {
                            // Ждем результат фоновой подгрузки
                            let loading_result = join_handle.await.wrap_err("Spawn join failed")?;
                            debug!("Background loading result received: {:?}", loading_result);

                            // Обновляем значение
                            update_token_or_warning!(loading_result, token_lock, request_iteration);
                        }
                        None => {
                            // Сбрасываем блокировку
                            drop(loading_join_lock);

                            // Иначе запрашиваем токен и обновляем значение локально
                            let load_res = ReceivedTokenInfo::request(&self.http_client, self.account_data.as_ref(), self.scopes).await;

                            // Обновляем значение или идем на новую итерацию при ошибке
                            update_token_or_warning!(load_res, token_lock, request_iteration);
                        }
                    }
                }
                // Если протухает через 60 секунд, начинаем фоновое получение нового токена
                else if info.life_duration_left() < StdDuration::from_secs(60) {
                    debug!("Token will expire after 60 seconds");

                    // Старт фоновой загрузки если нету
                    {
                        let mut loading_join_lock = self.background_loading.lock().await;
                        if loading_join_lock.is_none() {
                            debug!("Start background loading");
                            loading_join_lock.replace(self.spawn_receive_token());
                        }
                    }

                    // Возвращаем старое значение пока что, оно вполне валидное
                    return Ok(info.data.access_token.clone());
                }
                // Пока возвращаем старый статус
                else {
                    return Ok(info.data.access_token.clone());
                }
            } else {
                // Иначе запрашиваем токен и обновляем значение локально
                let load_res = ReceivedTokenInfo::request(&self.http_client, self.account_data.as_ref(), self.scopes).await;
                // Обновляем значение или идем на новую итерацию при ошибке
                update_token_or_warning!(load_res, token_lock, request_iteration);
            }
        }

        // Делаем токен None
        token_lock.take();

        // За 10 итерациий не смогли получить токен
        Err(eyre::eyre!("Invalid tokens received more than 10 times"))
    }
}
