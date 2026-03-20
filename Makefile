.PHONY: run build test test-vst3 release dist release-intel icon clean web

APP_NAME    := Layers
VERSION     := $(shell grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/' | cut -d. -f1,2)
BUILD       := $(shell cat build_version)
FULL_VER    := $(VERSION).$(BUILD)
BUNDLE      := build/$(APP_NAME).app
CONTENTS    := $(BUNDLE)/Contents
MACOS_DIR   := $(CONTENTS)/MacOS
RES_DIR     := $(CONTENTS)/Resources
BINARY_ARM  := target/aarch64-apple-darwin/release/layers
BINARY_X86  := target/x86_64-apple-darwin/release/layers
BINARY      := target/release/layers-universal

run:
	@v=$$(cat build_version); v=$$((v + 1)); echo $$v > build_version; echo "build #$$v"
	cargo run --bin layers -- --empty

build:
	@v=$$(cat build_version); v=$$((v + 1)); echo $$v > build_version; echo "build #$$v"
	cargo build

test:
	cargo test

test-vst3:
	cargo run --bin test_vst3

release:
	@echo "Building universal release binary (arm64 + x86_64)..."
	cargo build --release --target aarch64-apple-darwin
	cargo build --release --target x86_64-apple-darwin
	@mkdir -p target/release
	lipo -create "$(BINARY_ARM)" "$(BINARY_X86)" -output "$(BINARY)"
	@echo "Creating app bundle at $(BUNDLE)..."
	@rm -rf "$(BUNDLE)"
	@mkdir -p "$(MACOS_DIR)" "$(RES_DIR)"
	@cp "$(BINARY)" "$(MACOS_DIR)/layers"
	@cp macos/Info.plist "$(CONTENTS)/Info.plist"
	@if [ -f macos/AppIcon.icns ]; then \
		cp macos/AppIcon.icns "$(RES_DIR)/AppIcon.icns"; \
		echo "Bundled existing AppIcon.icns"; \
	elif [ -f macos/AppIcon.png ]; then \
		bash macos/create_icns.sh macos/AppIcon.png "$(RES_DIR)/AppIcon.icns"; \
	else \
		echo "No icon found (place AppIcon.png or AppIcon.icns in macos/)"; \
	fi
	@echo "Signing app bundle..."
	@codesign --force --deep --sign - --entitlements macos/Entitlements.plist "$(BUNDLE)"
	@echo "Done: $(BUNDLE)"

dist: release
	@echo "Creating distributable zip ($(FULL_VER))..."
	@cd build && ditto -c -k --keepParent "$(APP_NAME).app" "$(APP_NAME)-$(FULL_VER).zip"
	@echo "Done: build/$(APP_NAME)-$(FULL_VER).zip"

release-intel:
	@echo "Building Intel (x86_64) release binary..."
	@v=$$(cat build_version); v=$$((v + 1)); echo $$v > build_version; echo "build #$$v"
	cargo build --release --target x86_64-apple-darwin
	@echo "Creating app bundle at $(BUNDLE)..."
	@rm -rf "$(BUNDLE)"
	@mkdir -p "$(MACOS_DIR)" "$(RES_DIR)"
	@cp "$(BINARY_X86)" "$(MACOS_DIR)/layers"
	@cp macos/Info.plist "$(CONTENTS)/Info.plist"
	@if [ -f macos/AppIcon.icns ]; then \
		cp macos/AppIcon.icns "$(RES_DIR)/AppIcon.icns"; \
		echo "Bundled existing AppIcon.icns"; \
	elif [ -f macos/AppIcon.png ]; then \
		bash macos/create_icns.sh macos/AppIcon.png "$(RES_DIR)/AppIcon.icns"; \
	else \
		echo "No icon found (place AppIcon.png or AppIcon.icns in macos/)"; \
	fi
	@echo "Signing app bundle..."
	@codesign --force --deep --sign - --entitlements macos/Entitlements.plist "$(BUNDLE)"
	@echo "Creating distributable zip ($(FULL_VER)-intel)..."
	@cd build && ditto -c -k --keepParent "$(APP_NAME).app" "$(APP_NAME)-$(FULL_VER)-intel.zip"
	@echo "Done: build/$(APP_NAME)-$(FULL_VER)-intel.zip"

icon:
	@if [ ! -f macos/AppIcon.png ]; then \
		echo "Error: macos/AppIcon.png not found"; exit 1; \
	fi
	bash macos/create_icns.sh macos/AppIcon.png macos/AppIcon.icns

web:
	trunk serve --open --port 9090

clean:
	rm -rf build dist
