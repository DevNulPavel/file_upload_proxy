use serde::Deserialize;
use std::{fs::File, io::BufReader, path::PathBuf};

/// Настройки для всего приложения
#[derive(Deserialize, Debug)]
pub struct SettingsConfig {
    pub port: u16,
}

/// Настройки для проекта и выгрузки в CloudStorage
#[derive(Deserialize, Debug)]
pub struct GoogleStorageConfig {
    pub credentials_file: PathBuf,
    pub bucket_name: String,
}

/// Настройки для проекта и выгрузки в Slack
#[derive(Deserialize, Debug)]
pub struct SlackConfig {
    pub token: String,
    pub targets: Vec<String>,
    pub qr_code: bool,
    pub default_text_before: Option<String>,
}

/// Описание для отдельного проекта
#[derive(Deserialize, Debug)]
pub struct ProjectConfig {
    pub api_token: String,
    pub google_storage_target: GoogleStorageConfig,
    pub slack_link_dub: Option<SlackConfig>,
}

/// Конфиг нашего приложения
#[derive(Deserialize, Debug)]
pub struct Config {
    pub settings: SettingsConfig,
    pub projects: Vec<ProjectConfig>,
}

impl Config {
    /// Пытаемся распасить конфиг из файлика
    pub fn parse_from_file(path: PathBuf) -> Result<Config, eyre::Error> {
        // Пробуем загрузить конфиг из файлика в зависимости от расширения
        let config: Config = match path.extension().and_then(|v| v.to_str()).map(str::to_lowercase).as_deref() {
            Some("yml") | Some("yaml") => {
                let r = BufReader::new(File::open(path)?);
                serde_yaml::from_reader(r)?
            }
            Some("json") => {
                let r = BufReader::new(File::open(path)?);
                serde_json::from_reader(r)?
            }
            _ => {
                return Err(eyre::eyre!(
                    "Unsupported config file extention {}. Only yml/yaml/json/toml are supported",
                    path.display()
                ));
            }
        };

        // Отвалидируем данные конфига после загрузки
        config.validate_config()?;

        Ok(config)
    }

    fn validate_config(&self) -> Result<(), eyre::Error> {
        use eyre::ensure;

        // Есть вообще проекты?
        ensure!(!self.projects.is_empty(), "Empty projects list");

        // Проверим каждый проект
        for (key, proj) in self.projects.iter().enumerate() {
            // Токен
            ensure!(!proj.api_token.is_empty(), "Project {}: empty token", key);

            // Корзина выгрузки
            ensure!(
                !proj.google_storage_target.bucket_name.is_empty(),
                "Project {}: empty google storage bucket",
                key
            );

            // Файлик креденшиалов выгрузки
            ensure!(
                proj.google_storage_target.credentials_file.exists(),
                "Project {}: google storage credential file does not exist",
                key
            );
            ensure!(
                proj.google_storage_target.credentials_file.is_file(),
                "Project {}: google storage credential file is NOT a file",
                key
            );

            // Данные слака
            if let Some(slack) = &proj.slack_link_dub {
                // Токен
                ensure!(!slack.token.is_empty(), "Project {}: empty slack token", key);
                // Токен
                ensure!(!slack.targets.is_empty(), "Project {}: slack targets", key);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn test_results(config: Config) {
        assert_eq!(config.settings.port, 8080);

        let project_config = config.projects.get(0).unwrap();
        assert_eq!(project_config.api_token, "TOKEN_VALUE");

        let google_storage_info = &project_config.google_storage_target;
        assert_eq!(google_storage_info.credentials_file, Path::new("/TEST/CREDENTIALS_FILE.json"));
        assert_eq!(google_storage_info.bucket_name, "PI2_BUCKET_NAME");

        // TODO: Add new tests
    }

    #[test]
    fn test_yaml_config_parsing() {
        #[rustfmt::skip]
        let config: Config = serde_yaml::from_str(r#"
            settings:
              port: 8080
            projects:
                  - api_token: "TOKEN_VALUE"
                    google_storage_target:
                        credentials_file: "/TEST/CREDENTIALS_FILE.json"
                        bucket_name: "PI2_BUCKET_NAME"
                    slack_link_dub:
                        token: "asdasd"
                        targets: ["asdasd", "asdads", "asdasd"]
                        qr_code: true
                        default_text_before: "qweqwe"
                  - api_token: "asddasd"
                    google_storage_target:
                        credentials_file: "/asd/asdasd.json"
                        bucket_name: "dfgdfg"
                    slack_link_dub:
                        token: "asdasd"
                        targets: ["sfdsf", "sfds", "sdfds"]
                        qr_code: true
                        default_text_before: "dfgdfg"
        "#)
        .expect("Yaml config parsing failed");

        test_results(config);
    }

    #[test]
    fn test_json_config_parsing() {
        #[rustfmt::skip]
        let config: Config = serde_json::from_str(r#"
            {
                "settings": {
                    "port": 8080
                },
                "projects": [
                    {
                        "api_token": "TOKEN_VALUE",
                        "google_storage_target": {
                            "credentials_file": "/TEST/CREDENTIALS_FILE.json",
                            "bucket_name": "PI2_BUCKET_NAME"
                        },
                        "slack_link_dub": {
                            "token": "asdasd",
                            "targets": ["qweasd", "asdasdas"],
                            "qr_code": true,
                            "default_text_before": "asdasda"
                        }                        
                    },
                    {
                        "api_token": "fgdfg",
                        "google_storage_target": {
                            "credentials_file": "test/test.json",
                            "bucket_name": "sfdsdff"
                        },
                        "slack_link_dub": {
                            "token": "sdfdsf",
                            "targets": ["sdf", "sdf"],
                            "qr_code": true,
                            "default_text_before": "sdfsdf"
                        }                        
                    }
                ]
            }
        "#).expect("Json parsing failed");

        test_results(config);
    }
}
