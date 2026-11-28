# Release build of WASM Geo Host.
#
#   make release   – tests, sdd gates, all release binaries and the packages in dist/packages
#   make binaries  – only the binaries (host, plugin components, plugin CLIs)
#   make models    – download the CLIP model files into models/ (network, SHA-256 checked, ADR 0010)
#   make geonames  – refresh the reversegeo offline gazetteer from GeoNames (network, ADR 0006)
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

PLUGINS := metadata image-convert reversegeo countryflag thumbnail ai-tagging duplicate timeline \
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

# GeoNames dumps for `make geonames` (CC BY 4.0, https://www.geonames.org/); kept outside the repo
GEONAMES_URL   := https://download.geonames.org/export/dump
GEONAMES_DIR   ?= $(or $(TMPDIR),/tmp)/flagme-geonames
GEONAMES_FILES := cities1000.zip alternateNamesV2.zip admin1CodesASCII.txt admin2Codes.txt countryInfo.txt
GEONAMES_OUT   := plugins/reversegeo/data/places.bin

# CLIP model files for ai-tagging (ADR 0010); fixed revision, checked against models/SHA256SUMS
CLIP_URL     := https://huggingface.co/Xenova/clip-vit-base-patch32/resolve/6b7c523703ae26e39be4b03774966f8571c7f523
CLIP_FILES   := clip-vit-b32-vision-int8.onnx=onnx/vision_model_int8.onnx \
                clip-vit-b32-text-int8.onnx=onnx/text_model_int8.onnx \
                clip-vit-b32-tokenizer.json=tokenizer.json
CLIP_LICENSE := https://raw.githubusercontent.com/openai/CLIP/main/LICENSE
CLIP_MODEL   := models/clip-vit-b32-vision-int8.onnx

.PHONY: help release binaries host components wasm-cli native-cli verify test check dist clean-dist version geonames models labels

help: ## Show the targets
	@grep -E '^[a-z-]+:.*## ' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*## "} {printf "  %-12s %s\n", $$1, $$2}'

version: ## Print the release version (from crates/host/Cargo.toml)
	@echo $(VERSION)

release: check models test binaries verify dist ## Tests, sdd gates, binaries and packages
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

geonames: ## Download GeoNames dumps and rebuild plugins/reversegeo/data/places.bin (commit it afterwards)
	mkdir -p "$(GEONAMES_DIR)"
	for f in $(GEONAMES_FILES); do curl -fsSL --retry 3 -o "$(GEONAMES_DIR)/$$f" "$(GEONAMES_URL)/$$f"; done
	cd "$(GEONAMES_DIR)" && unzip -o -q cities1000.zip cities1000.txt && unzip -o -q alternateNamesV2.zip alternateNamesV2.txt
	$(CARGO) run --release --locked -p geonames-import -- --input "$(GEONAMES_DIR)" --output $(GEONAMES_OUT) --date "$$(date -u +%F)"
	@echo "Review and commit $(GEONAMES_OUT); update the data date in NOTICE, plugins/reversegeo/README.md and CHANGELOG.md"

models: ## Download the CLIP model files (ai-tagging) into models/ and verify their SHA-256
	@for pair in $(CLIP_FILES); do \
	  name=$${pair%%=*}; src=$${pair#*=}; \
	  if [ -f "models/$$name" ] && (cd models && grep " $$name\$$" SHA256SUMS | shasum -a 256 -c --status); then \
	    echo "models/$$name: OK (present)"; continue; fi; \
	  curl -fsSL --retry 3 -o "models/$$name.part" "$(CLIP_URL)/$$src"; \
	  mv "models/$$name.part" "models/$$name"; \
	  if ! (cd models && grep " $$name\$$" SHA256SUMS | shasum -a 256 -c --status); then \
	    rm -f "models/$$name"; echo "models/$$name: SHA-256 mismatch, removed"; exit 1; fi; \
	  echo "models/$$name: OK (downloaded)"; \
	done
	@[ -f models/CLIP-LICENSE ] || curl -fsSL --retry 3 -o models/CLIP-LICENSE "$(CLIP_LICENSE)"

labels: models ## Recompute plugins/ai-tagging/labels/labels.bin from labels.txt (commit it afterwards)
	$(CARGO) run --release --locked -p clip-labels -- --labels plugins/ai-tagging/labels/labels.txt \
	  --model models/clip-vit-b32-text-int8.onnx --tokenizer models/clip-vit-b32-tokenizer.json \
	  --output plugins/ai-tagging/labels/labels.bin

test: models ## Workspace tests (the host tests build the components themselves; ai-tagging needs make models)
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
	for c in config.yaml config_2cores_4GB.yaml config_cpu_8GB.yaml config_mac-studio_32GB.yaml config_mac-studio_64GB.yaml; do \
	  sed -E 's|^plugin_dir:.*|plugin_dir: "plugins" # relative to the working directory|' $$c > $(STAGE)/$(COMPLETE)/$$c; \
	done
	cp $(DOCS) $(STAGE)/$(COMPLETE)/
	mkdir -p $(STAGE)/$(COMPLETE)/models && cp $(CLIP_MODEL) models/CLIP-LICENSE $(STAGE)/$(COMPLETE)/models/
	@# Platform package: what is needed to run the host (host, components, configs, docs) plus the native plugin CLIs
	mkdir -p $(STAGE)/$(PLATFORM)
	cp -R $(STAGE)/$(COMPLETE)/bin $(STAGE)/$(COMPLETE)/plugins $(STAGE)/$(COMPLETE)/models $(STAGE)/$(PLATFORM)/
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
