.PHONY:
.SILENT:

ENCRYPT_TEST_ENV:
	gpg -a -r 0x0BD10E4E6E578FB6 -o env/test_google_service_account.json.asc -e env/test_google_service_account.json
	gpg -a -r 0x0BD10E4E6E578FB6 -o env/prod_google_service_account.json.asc -e env/prod_google_service_account.json
	gpg -a -r 0x0BD10E4E6E578FB6 -o env/prod_test_settings.env.asc -e env/prod_test_settings.env

DECRYPT_TEST_ENV:
	rm -rf env/test_google_service_account.json
	rm -rf env/prod_google_service_account.json
	rm -rf env/prod_test_settings.env
	gpg -a -r 0x0BD10E4E6E578FB6 -o env/test_google_service_account.json -d env/test_google_service_account.json.asc
	gpg -a -r 0x0BD10E4E6E578FB6 -o env/prod_google_service_account.json -d env/prod_google_service_account.json.asc
	gpg -a -r 0x0BD10E4E6E578FB6 -o env/prod_test_settings.env -d env/prod_test_settings.env.asc

RUN_APP:
	export RUST_LOG=file_upload_proxy=trace,warn && \
	cargo clippy && \
	cargo build --release && \
	target/release/file_upload_proxy \
		--uploader-api-token "test-api-token-aaa-bbb" \
		--google-credentials-file "env/prod_google_service_account.json" \
		--google-bucket-name "pi2-prod" \
		--port 8888

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

TEST_REQUEST_LOCAL_1:
	curl \
		-v \
		-X GET \
		-H "Content-Type: text/plain" \
		-H "X-Api-Token: test-api-token-aaa-bbb" \
		"http://localhost:8888/upload_file/"

TEST_REQUEST_LOCAL_2:
	curl \
		-v \
		-X POST \
		-H "Content-Type: text/plain" \
		-H "X-Api-Token: test-api-token-aaa-bbb" \
		--data-binary "@./Cargo.lock" \
		"http://localhost:8888/upload_file/"

TEST_REQUEST_LOCAL_3:
	curl \
		-v \
		-X POST \
		-H "Content-Type: text/plain" \
		-H "X-Filename: file_$(shell date +%Y-%m-%d_%H-%M-%S).txt" \
		-H "X-Api-Token: test-api-token-aaa-bbb" \
		--data-binary "@./Cargo.lock" \
		"http://localhost:8888/upload_file/"

TEST_REQUEST_LOCAL_4:
	curl \
		-v \
		-X POST \
		-H "Content-Type: text/plain" \
		-H "X-Api-Token: test-api-token-aaa-bbb" \
		--data-binary "@./Cargo.lock" \
		"http://localhost:8888/upload_file/?filename=file_$(shell date +%Y-%m-%d_%H-%M-%S).txt"

TEST_REQUEST_LOCAL_5:
	curl \
		-v \
		-X GET \
		"http://localhost:8888/prometheus_metrics/"

TEST_REQUEST_LOCAL_6:
	curl \
		-v \
		-X GET \
		"http://localhost:8888/health/"


###########################################################################################

# Подключаем необходимое нам окружение для теста сервера
# include ./env/prod_test_settings.env

# nginx сейчас настроен для редиректов, поэтому требуется флаг -L
# При использовании нативной библиотеки нужно проставлять флаг
# https://curl.se/libcurl/c/CURLOPT_FOLLOWLOCATION.html
# !!!!! Обязательно указываем в конце слеш, иначе прилетает 301 редирект !!!!!
TEST_REQUEST_REMOTE:
	source ./env/prod_test_settings.env && \
	cd prod_deploy_test && \
	cargo run

# Руками лучше не собрать билды локально, а вместо этого
# запускать сборку на github через actions
BUILD_DOCKER_IMAGE:
	docker buildx build --platform linux/amd64,linux/arm64 .

TEST:
	source Dockerfile