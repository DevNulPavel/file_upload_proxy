use serde::Deserialize;
use std::{
    fs::{self, File},
    io::BufReader,
    path::PathBuf,
};
use crate::helpers::deserialize_url;



/// Описание для отдельного проекта
#[derive(Deserialize)]
pub struct ProjectConfig {
    pub api_token: String,
}

/// Конфиг нашего приложения
#[derive(Deserialize)]
pub struct Config {
    // #[serde(deserialize_with = "deserialize_url")]
    // pub base_url: reqwest::Url,

    #[serde(deserialize_with = "deserialize_url")]
    pub file_upload_url: reqwest::Url,
    pub projects: Vec<ProjectConfig>,
}

impl Config {
    /// Пытаемся распасить конфиг из файлика
    pub fn parse_from_file(path: PathBuf) -> Config {
        // Пробуем загрузить конфиг из файлика в зависимости от расширения
        let config: Config = match path.extension().and_then(|v| v.to_str()).map(str::to_lowercase).as_deref() {
            Some("toml") => toml::from_slice(&fs::read(path).unwrap()).unwrap(),
            Some("yml") | Some("yaml") => {
                let r = BufReader::new(File::open(path).unwrap());
                serde_yaml::from_reader(r).unwrap()
            }
            Some("json") => {
                let r = BufReader::new(File::open(path).unwrap());
                serde_json::from_reader(r).unwrap()
            }
            _ => {
                panic!(
                    "Unsupported config file extention {}. Only yml/yaml/json/toml are supported",
                    path.display()
                );
            }
        };

        // Отвалидируем данные конфига после загрузки
        config.validate_config().unwrap();

        config
    }

    fn validate_config(&self) -> Result<(), eyre::Error> {
        use eyre::ensure;

        // Есть вообще проекты?
        ensure!(!self.projects.is_empty(), "Empty projects list");

        // Проверим каждый проект
        for proj in self.projects.iter() {
            // Токен
            ensure!(!proj.api_token.is_empty(), "Empty token");
        }

        Ok(())
    }
}
