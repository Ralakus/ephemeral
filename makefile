
OUT_DIR = dist
WWW     = $(OUT_DIR)/www

BUILD_MODE  = debug
CARGO_FLAGS = 
TRUNK_FLAGS = 

TRUNK_BUILD_FILE = index/index.html
TAILWIND_CONFIG  = index/tailwind.config.js

dist: target/server target/index

dist_directories:
	if [ -d "$(OUT_DIR)" ]; then rm -r $(OUT_DIR); fi
	mkdir $(OUT_DIR)
	mkdir $(WWW)

release: BUILD_MODE = release
release: CARGO_FLAGS += --release
release: TRUNK_FLAGS += --release
release: dist

target/common:
	cargo build --bin common $(CARGO_FLAGS)

target/server: dist_directories
	cargo build --bin server $(CARGO_FLAGS)
	cp target/$(BUILD_MODE)/server $(OUT_DIR)

target/css:
	NODE_ENV=production tailwindcss -c $(TAILWIND_CONFIG) -o target/tailwind.css --minify

target/index: dist_directories target/css
	trunk build $(TRUNK_BUILD_FILE) -d $(WWW) $(TRUNK_FLAGS)

run: dist
	cd $(OUT_DIR); ./server 8000

watch:
	cargo watch --no-gitignore --ignore $(OUT_DIR) --ignore makefile --ignore readme.md --ignore license -s "make run"
