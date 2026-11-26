# Release build of WASM Geo Host.
#
#   make release   – tests, sdd gates, all release binaries and the packages in dist/packages
#   make binaries  – only the binaries (host, plugin components, plugin CLIs)
#   make help      – all targets
#
# The host loads plugins only as Wasm components (ADR 0003). Components (feature `component`)
# and the standalone WASI CLI binaries (without it) are built into separate target directories,
# because both link the same plugin libraries and would overwrite each other's artifacts.

SHELL := /bin/bash
.SHELLFLAGS := -eu -o pipefail -c
.DEFAULT_GOAL := help

CARGO   ?= cargo
WASM    := wasm32-wasip2
VERSION := $(shell sed -n 's/^version = "\(.*\)"/\1/p' crates/host/Cargo.toml | head -n 1)
OS      := $(shell uname -s | tr '[:upper:]' '[:lower:]')
ARCH    := $(shell uname -m | sed 's/x86_64/amd64/;s/aarch64/arm64/')

PLUGINS := metadata reversegeo countryflag thumbnail ai-tagging duplicate timeline \
           weather-enrich ocr-text face-detection privacy-redact
PKG_FLAGS := $(foreach p,$(PLUGINS),-p flagme-plugin-$(p))

# Build outputs
COMPONENT_DIR := target/$(WASM)/release
WASM_CLI_DIR  := target/wasm-cli/$(WASM)/release
NATIVE_DIR    := target/release
COMPONENTS    := $(foreach p,$(PLUGINS),$(COMPONENT_DIR)/flagme_plugin_$(subst -,_,$(p)).wasm)
WASM_CLIS     := $(foreach p,$(PLUGINS),$(WASM_CLI_DIR)/flagme-plugin-$(p).wasm)
NATIVE_CLIS   := $(foreach p,$(PLUGINS),$(NATIVE_DIR)/flagme-plugin-$(p))

# Release layout
DIST      := dist
STAGE     := $(DIST)/v$(VERSION)
PACKAGES  := $(DIST)/packages
COMPLETE  := flagme-v$(VERSION)-complete-bundle
PLATFORM  := flagme-v$(VERSION)-$(OS)-$(ARCH)
PKG_COMP  := flagme-plugins-components-v$(VERSION)-$(WASM)
PKG_CLI   := flagme-plugins-cli-v$(VERSION)-$(WASM)
DOCS      := README.md CHANGELOG.md LICENSE NOTICE

.PHONY: help release binaries host components wasm-cli native-cli verify test check dist clean-dist version

help: ## Show the targets
	@grep -E '^[a-z-]+:.*## ' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*## "} {printf "  %-12s %s\n", $$1, $$2}'

version: ## Print the release version (from crates/host/Cargo.toml)
	@echo $(VERSION)

release: check test binaries verify dist ## Tests, sdd gates, binaries and packages
	@echo "Release v$(VERSION) ready in $(PACKAGES)"

binaries: host components wasm-cli native-cli ## All release binaries

host: ## Native host binary (flagme-host)
	$(CARGO) build --release --locked -p flagme-host

components: ## Plugin Wasm components (media-plugin world) for the host
	$(CARGO) build --release --locked --target $(WASM) --lib --features component $(PKG_FLAGS)

wasm-cli: ## Standalone WASI CLI binaries of the plugins (wasmtime run)
	$(CARGO) build --release --locked --target $(WASM) --bins --target-dir target/wasm-cli $(PKG_FLAGS)

native-cli: ## Native standalone CLI binaries of the plugins
	$(CARGO) build --release --locked --bins $(PKG_FLAGS)

verify: ## Check that every component exports the media-plugin world (needs wasm-tools)
	@command -v wasm-tools >/dev/null || { echo "wasm-tools not found (cargo install wasm-tools)"; exit 1; }
	@for f in $(COMPONENTS); do \
	  wasm-tools component wit $$f | grep -q 'export process: func' || { echo "$$f: no media-plugin export"; exit 1; }; \
	done
	@for f in $(WASM_CLIS); do \
	  if wasm-tools component wit $$f | grep -q 'host-capabilities'; then echo "$$f: CLI imports host-capabilities"; exit 1; fi; \
	done
	@echo "verified $(words $(COMPONENTS)) components and $(words $(WASM_CLIS)) CLI binaries"

test: ## Workspace tests (the host tests build the components themselves)
	$(CARGO) test --workspace --locked

check: ## sdd lint and pre-release gate
	sdd lint
	sdd gate pre-release

dist: ## Assemble dist/v<version> and the packages with SHA256SUMS
	rm -rf $(STAGE) && mkdir -p $(PACKAGES)
	rm -f $(PACKAGES)/*v$(VERSION)*
	@# Complete bundle: native host and plugin CLIs, components, WASI CLIs, configs, docs
	mkdir -p $(STAGE)/$(COMPLETE)/bin $(STAGE)/$(COMPLETE)/plugins $(STAGE)/$(COMPLETE)/wasm-cli
	cp $(NATIVE_DIR)/flagme-host $(NATIVE_CLIS) $(STAGE)/$(COMPLETE)/bin/
	cp $(COMPONENTS) $(STAGE)/$(COMPLETE)/plugins/
	cp $(WASM_CLIS) $(STAGE)/$(COMPLETE)/wasm-cli/
	for c in config.yaml config.mac-studio.yaml; do \
	  sed -E 's|^plugin_dir:.*|plugin_dir: "plugins" # relative to the working directory|' $$c > $(STAGE)/$(COMPLETE)/$$c; \
	done
	cp $(DOCS) $(STAGE)/$(COMPLETE)/
	@# Platform package: what is needed to run the host (host, components, configs, docs) plus the native plugin CLIs
	mkdir -p $(STAGE)/$(PLATFORM)
	cp -R $(STAGE)/$(COMPLETE)/bin $(STAGE)/$(COMPLETE)/plugins $(STAGE)/$(PLATFORM)/
	cp $(STAGE)/$(COMPLETE)/config*.yaml $(STAGE)/$(PLATFORM)/
	cp $(DOCS) $(STAGE)/$(PLATFORM)/
	@# Plugin packages
	mkdir -p $(STAGE)/$(PKG_COMP) $(STAGE)/$(PKG_CLI)
	cp $(COMPONENTS) $(STAGE)/$(PKG_COMP)/
	cp $(WASM_CLIS) $(STAGE)/$(PKG_CLI)/
	for d in $(COMPLETE) $(PLATFORM) $(PKG_COMP) $(PKG_CLI); do \
	  tar -C $(STAGE) -czf $(PACKAGES)/$$d.tar.gz $$d; \
	done
	cd $(PACKAGES) && shasum -a 256 *v$(VERSION)*.tar.gz > SHA256SUMS-v$(VERSION)
	@cat $(PACKAGES)/SHA256SUMS-v$(VERSION)

clean-dist: ## Remove the staged files and packages of this version
	rm -rf $(STAGE) $(PACKAGES)/*v$(VERSION)*
