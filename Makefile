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
	check-agent-navigation \
	check-integration-targets \
	test-tooling \
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
	docs-check-contracts \
	docs-check-routes \
	docs-check-browser \
	docs-lint \
	docs-format \
	db-pull \
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
	dev-demo \
	server-dev-demo \
	docker-build \
	docker-up \
	docker-down \
	docker-logs \
	ios-install

.PHONY: menu recent help publish $(COMMAND_TARGETS)

menu:
	@$(MENU_RUNNER) menu

recent:
	@$(MENU_RUNNER) recent

help: menu

publish:
	@VERSION='$(VERSION)' DRY_RUN='$(DRY_RUN)' YES='$(YES)' PREPARE_ONLY='$(PREPARE_ONLY)' ./scripts/publish.sh

$(COMMAND_TARGETS):
	@$(MENU_RUNNER) run $@
