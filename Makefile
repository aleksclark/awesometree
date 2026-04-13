PREFIX ?= $(HOME)/.local
UNAME_S := $(shell uname -s)

# Linux-specific
SYSTEMD_USER_DIR ?= $(HOME)/.config/systemd/user
SERVICE_NAME = awesometree-daemon.service

# macOS-specific
LAUNCHD_DIR = $(HOME)/Library/LaunchAgents
PLIST_NAME = com.awesometree.daemon.plist
APP_NAME = Awesometree.app
APP_BUNDLE = target/release/$(APP_NAME)

.PHONY: build install clean uninstall openapi android-lib test enable disable restart bundle

build:
	cargo build --release

install: build
ifeq ($(UNAME_S),Darwin)
	install -d $(PREFIX)/bin
	install -m 755 target/release/awesometree $(PREFIX)/bin/awesometree
	install -m 755 target/release/awesometree-daemon $(PREFIX)/bin/awesometree-daemon
	install -d $(LAUNCHD_DIR)
	sed 's|__PREFIX__|$(PREFIX)|g' $(PLIST_NAME) > $(LAUNCHD_DIR)/$(PLIST_NAME)
	launchctl bootout gui/$$(id -u) $(LAUNCHD_DIR)/$(PLIST_NAME) 2>/dev/null || true
	launchctl bootstrap gui/$$(id -u) $(LAUNCHD_DIR)/$(PLIST_NAME)
else
	install -d $(PREFIX)/bin
	install -m 755 target/release/awesometree $(PREFIX)/bin/awesometree
	install -m 755 target/release/awesometree-daemon $(PREFIX)/bin/awesometree-daemon
	install -d $(SYSTEMD_USER_DIR)
	install -m 644 $(SERVICE_NAME) $(SYSTEMD_USER_DIR)/$(SERVICE_NAME)
	systemctl --user daemon-reload
	systemctl --user restart $(SERVICE_NAME)
endif

enable:
ifeq ($(UNAME_S),Darwin)
	launchctl bootstrap gui/$$(id -u) $(LAUNCHD_DIR)/$(PLIST_NAME)
else
	systemctl --user enable $(SERVICE_NAME)
endif

disable:
ifeq ($(UNAME_S),Darwin)
	launchctl bootout gui/$$(id -u) $(LAUNCHD_DIR)/$(PLIST_NAME) 2>/dev/null || true
else
	systemctl --user disable $(SERVICE_NAME)
endif

restart:
ifeq ($(UNAME_S),Darwin)
	launchctl kickstart -k gui/$$(id -u)/com.awesometree.daemon
else
	systemctl --user restart $(SERVICE_NAME)
endif

uninstall:
ifeq ($(UNAME_S),Darwin)
	-launchctl bootout gui/$$(id -u) $(LAUNCHD_DIR)/$(PLIST_NAME) 2>/dev/null
	rm -f $(LAUNCHD_DIR)/$(PLIST_NAME)
	rm -f $(PREFIX)/bin/awesometree $(PREFIX)/bin/awesometree-daemon
	rm -rf /Applications/$(APP_NAME)
else
	-systemctl --user stop $(SERVICE_NAME)
	-systemctl --user disable $(SERVICE_NAME)
	rm -f $(SYSTEMD_USER_DIR)/$(SERVICE_NAME)
	systemctl --user daemon-reload
	rm -f $(PREFIX)/bin/awesometree $(PREFIX)/bin/awesometree-daemon
	rm -f $(PREFIX)/bin/ws $(PREFIX)/bin/ws-picker $(PREFIX)/bin/ws-tray
endif

clean:
	cargo clean

openapi:
	cargo run --release --bin awesometree -- openapi

android-lib:
	cargo build -p awesometree-core --release --target aarch64-linux-android
	cargo build -p awesometree-core --release --target armv7-linux-androideabi
	cargo build -p awesometree-core --release --target x86_64-linux-android

test:
	cargo test --workspace

screenshots: build
	SCREENSHOTS=1 cargo test --test screenshots -- --nocapture

# macOS .app bundle
bundle: build
ifeq ($(UNAME_S),Darwin)
	@echo "Building Awesometree.app bundle..."
	rm -rf $(APP_BUNDLE)
	mkdir -p $(APP_BUNDLE)/Contents/MacOS
	mkdir -p $(APP_BUNDLE)/Contents/Resources
	cp target/release/awesometree $(APP_BUNDLE)/Contents/MacOS/
	cp target/release/awesometree-daemon $(APP_BUNDLE)/Contents/MacOS/
	cp macos/Info.plist $(APP_BUNDLE)/Contents/
	@if [ -f macos/AppIcon.icns ]; then \
		cp macos/AppIcon.icns $(APP_BUNDLE)/Contents/Resources/; \
	fi
	@echo "Bundle created: $(APP_BUNDLE)"
	@echo "To install: cp -r $(APP_BUNDLE) /Applications/"
else
	@echo "App bundle is only supported on macOS"
endif

install-bundle: bundle
ifeq ($(UNAME_S),Darwin)
	cp -r $(APP_BUNDLE) /Applications/
	@echo "Installed to /Applications/$(APP_NAME)"
endif
