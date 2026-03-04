PREFIX ?= $(HOME)/.local

.PHONY: build install clean uninstall

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
