
dependencies:
	echo "Installing nightly toolchain if not already installed..." && \
	rustup toolchain install nightly --allow-downgrade && \
	echo "Installing wasm32 target..." && \
	rustup target add wasm32-unknown-unknown
	echo "Installing trunk..." && \
	cargo install trunk

build:
	cargo build --release 

install:
	$(MAKE) dependencies
	$(MAKE) build
	sudo cp target/release/wakezilla /usr/local/bin/wakezilla && \
	echo "Installation complete."
