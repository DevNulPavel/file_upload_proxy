.PHONY:
.SILENT:

ENCRYPT_TEST_ENV:
	gpg -a -r 0x0BD10E4E6E578FB6 -o env/test_google_service_account.json.asc -e env/test_google_service_account.json
	gpg -a -r 0x0BD10E4E6E578FB6 -o env/test.env.asc -e env/test.env

DECRYPT_TEST_ENV:
	rm -rf env/test.env
	rm -rf env/test_google_service_account.json
	gpg -a -r 0x0BD10E4E6E578FB6 -o env/test_google_service_account.json -d env/test_google_service_account.json.asc

RUN_APP:
	export RUST_LOG=file_upload_proxy=debug,warn && \
	cargo clippy && \
	cargo build --release && \
	target/release/file_upload_proxy \
		--uploader-api-token "test-api-token-aaa-bbb" \
		--google-credentials-file "env/test_google_service_account.json" \
		--google-bucket-name "dev_test_public_bucket" \
		--port 8888

TEST_REQUEST_1:
	curl \
		-v \
		-X POST \
		-H "Content-Type: application/octet-stream" \
		-H "X-Api-Token: test-api-token-aaa-bbb" \
		--data-binary "@./Cargo.lock" \
		"http://localhost:8888/upload_file"

# nginx сейчас настроен для редиректов, поэтому требуется флаг -L
# При использовании нативной библиотеки нужно проставлять флаг
# https://curl.se/libcurl/c/CURLOPT_FOLLOWLOCATION.html
TEST_REQUEST_2:
	curl \
		-L \
		-v \
		-X POST \
		-H "Content-Type: application/octet-stream" \
		-H "X-Api-Token: f7011af4-231b-473c-b983-f200f9fcb585" \
		--data-binary "TEST_TEST_TEST" \
		"https://island2-web.17btest.com/upload_file"

# Руками лучше не собрать билды локально, а вместо этого
# запускать сборку на github через actions
BUILD_DOCKER_IMAGE:
	docker buildx build --platform linux/amd64,linux/arm64 .
