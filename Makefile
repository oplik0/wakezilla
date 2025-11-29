
dependencies:
	echo "Installing nightly toolchain if not already installed..." && \
	rustup toolchain install nightly --allow-downgrade && \
	echo "Installing wasm32 target..." && \
	rustup target add wasm32-unknown-unknown && \
	echo "Installing trunk..." && \
	cargo install trunk --locked

build:
	echo "Building frontend and backend..." && \
	cd frontend && trunk build --release && cd .. && \
	echo "Frontend build complete." && \
	cargo build --release && \
	echo "Backend build complete."

install:
	$(MAKE) dependencies
	$(MAKE) build
	sudo cp target/release/wakezilla /usr/local/bin/wakezilla && \
	echo "Installation complete."

release:
	$(MAKE) build
	cargo publish
