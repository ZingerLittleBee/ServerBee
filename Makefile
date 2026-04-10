.DEFAULT_GOAL := menu

MENU_RUNNER := bun scripts/make-menu.ts

COMMAND_TARGETS := \
	install \
	dev \
	dev-web \
	build \
	build-web \
	typecheck \
	check-types \
	lint \
	check \
	fix \
	test \
	test-watch \
	web-dev \
	web-dev-prod \
	web-build \
	web-typecheck \
	web-preview \
	web-test \
	web-test-watch \
	web-generate-api-types \
	docs-dev \
	docs-build \
	docs-start \
	docs-preview \
	docs-typecheck \
	docs-lint \
	docs-format \
	db-pull \
	db-local \
	db-push \
	db-generate \
	db-migrate \
	db-studio \
	db-local-direct \
	db-push-direct \
	db-generate-direct \
	db-migrate-direct \
	db-studio-direct \
	ui-typecheck \
	cargo-build \
	cargo-build-release \
	cargo-check \
	cargo-test \
	cargo-clippy \
	cargo-fmt \
	server-build \
	server-run \
	agent-build \
	agent-run \
	server-dev \
	server-dev-prod \
	agent-dev \
	dev-full \
	docker-build \
	docker-up \
	docker-down \
	docker-logs

.PHONY: menu recent help $(COMMAND_TARGETS)

menu:
	@$(MENU_RUNNER) menu

recent:
	@$(MENU_RUNNER) recent

help: menu

$(COMMAND_TARGETS):
	@$(MENU_RUNNER) run $@
