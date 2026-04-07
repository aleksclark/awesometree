PREFIX ?= $(HOME)/.local
SYSTEMD_USER_DIR ?= $(HOME)/.config/systemd/user
SERVICE_NAME = awesometree-daemon.service

.PHONY: build install clean uninstall openapi android-lib test enable disable restart

build:
	cargo build --release

install: build
	install -d $(PREFIX)/bin
	install -m 755 target/release/awesometree $(PREFIX)/bin/awesometree
	install -m 755 target/release/awesometree-daemon $(PREFIX)/bin/awesometree-daemon
	install -d $(SYSTEMD_USER_DIR)
	install -m 644 $(SERVICE_NAME) $(SYSTEMD_USER_DIR)/$(SERVICE_NAME)
	systemctl --user daemon-reload
	systemctl --user restart $(SERVICE_NAME)

enable:
	systemctl --user enable $(SERVICE_NAME)

disable:
	systemctl --user disable $(SERVICE_NAME)

restart:
	systemctl --user restart $(SERVICE_NAME)

uninstall:
	-systemctl --user stop $(SERVICE_NAME)
	-systemctl --user disable $(SERVICE_NAME)
	rm -f $(SYSTEMD_USER_DIR)/$(SERVICE_NAME)
	systemctl --user daemon-reload
	rm -f $(PREFIX)/bin/awesometree $(PREFIX)/bin/awesometree-daemon
	rm -f $(PREFIX)/bin/ws $(PREFIX)/bin/ws-picker $(PREFIX)/bin/ws-tray

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
