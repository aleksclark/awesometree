PREFIX ?= $(HOME)/.local

.PHONY: build install clean uninstall openapi android-lib test

build:
	cargo build --release

install: build
	install -d $(PREFIX)/bin
	install -m 755 target/release/awesometree $(PREFIX)/bin/awesometree
	install -m 755 target/release/awesometree-daemon $(PREFIX)/bin/awesometree-daemon

uninstall:
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
