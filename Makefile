APP_NAME := NotTerminal
EXECUTABLE := not-terminal
BUNDLE_ID := com.seg4lt.notterminal
ICON_FILE := app-icon.icns
CONFIG ?= release

TARGET_DIR := target/$(CONFIG)
BIN_PATH := $(TARGET_DIR)/$(EXECUTABLE)
BUILD_DIR := build
APP_DIR := $(BUILD_DIR)/$(APP_NAME).app
CONTENTS_DIR := $(APP_DIR)/Contents
MACOS_DIR := $(CONTENTS_DIR)/MacOS
RESOURCES_DIR := $(CONTENTS_DIR)/Resources
PLIST_PATH := $(CONTENTS_DIR)/Info.plist

.PHONY: all build app clean

all: app

build:
	cargo build --$(CONFIG)

app: build
	mkdir -p "$(MACOS_DIR)" "$(RESOURCES_DIR)"
	cp "$(BIN_PATH)" "$(MACOS_DIR)/$(EXECUTABLE)"
	cp "assets/$(ICON_FILE)" "$(RESOURCES_DIR)/$(ICON_FILE)"
	chmod +x "$(MACOS_DIR)/$(EXECUTABLE)"
	printf '%s\n' \
		'<?xml version="1.0" encoding="UTF-8"?>' \
		'<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">' \
		'<plist version="1.0">' \
		'<dict>' \
		'	<key>CFBundleName</key>' \
		'	<string>$(APP_NAME)</string>' \
		'	<key>CFBundleDisplayName</key>' \
		'	<string>$(APP_NAME)</string>' \
		'	<key>CFBundleExecutable</key>' \
		'	<string>$(EXECUTABLE)</string>' \
		'	<key>CFBundleIdentifier</key>' \
		'	<string>$(BUNDLE_ID)</string>' \
		'	<key>CFBundleIconFile</key>' \
		'	<string>$(ICON_FILE)</string>' \
		'	<key>CFBundlePackageType</key>' \
		'	<string>APPL</string>' \
		'	<key>CFBundleVersion</key>' \
		'	<string>1</string>' \
		'	<key>CFBundleShortVersionString</key>' \
		'	<string>0.1.0</string>' \
		'	<key>LSMinimumSystemVersion</key>' \
		'	<string>13.0</string>' \
		'</dict>' \
		'</plist>' > "$(PLIST_PATH)"
	@echo "Built $(APP_DIR)"

clean:
	rm -rf "$(BUILD_DIR)"
