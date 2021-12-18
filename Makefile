.PHONY:
.SILENT:

ENCRYPT_TEST_ENV:
	gpg -a -r 0x0BD10E4E6E578FB6 -o env/test_google_service_account.json.asc -e env/test_google_service_account.json
	gpg -a -r 0x0BD10E4E6E578FB6 -o env/prod_google_service_account.json.asc -e env/prod_google_service_account.json

DECRYPT_TEST_ENV:
	rm -rf env/test_google_service_account.json
	rm -rf env/prod_google_service_account.json
	gpg -a -r 0x0BD10E4E6E578FB6 -o env/test_google_service_account.json -d env/test_google_service_account.json.asc
	gpg -a -r 0x0BD10E4E6E578FB6 -o env/prod_google_service_account.json -d env/prod_google_service_account.json.asc

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

TEST_REQUEST_1:
	curl \
		-v \
		-X GET \
		-H "Content-Type: text/plain" \
		-H "X-Api-Token: test-api-token-aaa-bbb" \
		"http://localhost:8888/upload_file/"

TEST_REQUEST_2:
	curl \
		-v \
		-X POST \
		-H "Content-Type: text/plain" \
		-H "X-Api-Token: test-api-token-aaa-bbb" \
		--data-binary "@./Cargo.lock" \
		"http://localhost:8888/upload_file/"

TEST_REQUEST_3:
	curl \
		-v \
		-X POST \
		-H "Content-Type: text/plain" \
		-H "X-Filename: file_$(shell date +%Y-%m-%d_%H-%M-%S).txt" \
		-H "X-Api-Token: test-api-token-aaa-bbb" \
		--data-binary "@./Cargo.lock" \
		"http://localhost:8888/upload_file/"

TEST_REQUEST_4:
	curl \
		-v \
		-X POST \
		-H "Content-Type: text/plain" \
		-H "X-Api-Token: test-api-token-aaa-bbb" \
		--data-binary "@./Cargo.lock" \
		"http://localhost:8888/upload_file/?filename=file_$(shell date +%Y-%m-%d_%H-%M-%S).txt"

# nginx сейчас настроен для редиректов, поэтому требуется флаг -L
# При использовании нативной библиотеки нужно проставлять флаг
# https://curl.se/libcurl/c/CURLOPT_FOLLOWLOCATION.html
# !!!!! Обязательно указываем в конце слеш, иначе прилетает 301 редирект !!!!!
TEST_REQUEST_5:
	curl \
		-L \
		-v \
		-X GET \
		-H "Content-Type: text/plain" \
		-H "X-Api-Token: f7011af4-231b-473c-b983-f200f9fcb585" \
		"https://island2-web.17btest.com/upload_file/"

TEST_REQUEST_6:
	curl \
		-L \
		-v \
		-X POST \
		-H "Content-Type: text/plain" \
		-H "X-Api-Token: f7011af4-231b-473c-b983-f200f9fcb585" \
		--data-binary "@./Cargo.lock" \
		"https://island2-web.17btest.com/upload_file/"

TEST_REQUEST_7:
	curl \
		-L \
		-v \
		-X POST \
		-H "Content-Type: text/plain" \
		-H "X-Api-Token: f7011af4-231b-473c-b983-f200f9fcb585" \
		-H "X-Filename: file_$(shell date +%Y-%m-%d_%H-%M-%S).txt" \
		--data-binary "@./Cargo.lock" \
		"https://island2-web.17btest.com/upload_file/"

TEST_REQUEST_8:
	curl \
		-L \
		-v \
		-X POST \
		-H "Content-Type: text/plain" \
		-H "X-Api-Token: f7011af4-231b-473c-b983-f200f9fcb585" \
		--data-binary "@./Cargo.lock" \
		"https://island2-web.17btest.com/upload_file/?filename=file_$(shell date +%Y-%m-%d_%H-%M-%S).txt"

# Руками лучше не собрать билды локально, а вместо этого
# запускать сборку на github через actions
BUILD_DOCKER_IMAGE:
	docker buildx build --platform linux/amd64,linux/arm64 .

TEST:
	source Dockerfile