.PHONY: all build build-web build-rust install install-engine install-cli clean run

# 預設目標：建置前端並編譯安裝後端
all: install

# 1. 建立前端 WebUI 靜態資源
build-web:
	@echo "📦 正在建置 WebUI 前端靜態資源..."
	cd apps/webui && npm install && npm run build

# 2. 編譯 Rust 後端二進位檔
build-rust:
	@echo "🦀 正在編譯 Rust 後端大一統二進位檔..."
	cargo build --release

# 3. 完整建置流程（前端 + 後端）
build: build-web build-rust

# 4a. 安裝 sidecar engine binary（LanceDB 引擎，由 opendoc 以 child process 啟動）
install-engine:
	@echo "🚀 正在安裝 LanceDB engine binary 至 ~/.cargo/bin/opendoc-engine-lancedb ..."
	cargo install --path crates/opendoc-engine-lancedb --force

# 4b. 安裝主 binary（含 WebUI）
install-cli: build-web
	@echo "🚀 正在將包含 WebUI 的單一二進位檔安裝至本機 ~/.cargo/bin/opendoc ..."
	cargo install --path crates/opendoc-cli --force

# 4. 前端打包並強制編譯安裝到 ~/.cargo/bin（engine + 主 binary）
install: install-engine install-cli

# 5. 快速本地測試執行 (自動開啟 API 伺服器)
run:
	@echo "🏃 正在執行大一統伺服器 (WebUI 內建模式)..."
	cargo run -p opendoc-cli -- serve --port 8080

# 6. 清理建置暫存與前端產物
clean:
	@echo "🧹 正在清理 Rust 暫存與前端打包產物..."
	cargo clean
	rm -rf apps/webui/dist
