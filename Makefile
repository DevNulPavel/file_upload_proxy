.PHONY:
.SILENT:

DECRYPT_CONFIGS:
	git-crypt unlock

RUN_APP:
	export RUST_LOG=file_upload_proxy=trace,warn && \
	cargo clippy && \
	cargo build --release && \
	target/release/file_upload_proxy \
		--config "./configs/app_config/test.yaml"

RUN_TOKIO_CONSOLE:
	# cargo install tokio-console
	tokio-console

# Лучше использовать Docker
RUN_PROMETHEUS_LOCAL:
	prometheus \
		--storage.tsdb.path "./prometheus_data/" \
		--config.file "./monitoring_configs/prometheus/prometheus.yml" \
		--web.external-url "http://localhost:9090"

RUN_PROMETHEUS_AND_GRAFANA_DOCKER:
	cd docker_compose_testing && \
	docker-compose up

###########################################################################################

# Подключаем необходимое нам окружение для теста сервера
# include ./env/prod_test_configs.env

# nginx сейчас настроен для редиректов, поэтому требуется флаг -L
# При использовании нативной библиотеки нужно проставлять флаг
# https://curl.se/libcurl/c/CURLOPT_FOLLOWLOCATION.html
# !!!!! Обязательно указываем в конце слеш, иначе прилетает 301 редирект !!!!!

TEST_REQUEST_REMOTE:
	cd test_app && \
	cargo run -- --config=../configs/deploy_test/prod.yaml

TEST_REQUEST_LOCALHOST:
	cd test_app && \
	cargo run -- --config=../configs/deploy_test/localhost.yaml

###########################################################################################

# Руками лучше не собрать билды локально, а вместо этого
# запускать сборку на github через actions
BUILD_DOCKER_IMAGE:
	docker buildx build --platform linux/amd64,linux/arm64 .

TEST:
	source Dockerfile