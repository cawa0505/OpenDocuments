#!/bin/sh
# OpenDocuments 單一 Binary 跨平台安裝腳本 (Linux / macOS)
# 支援平台：Linux (x86_64, aarch64) 與 macOS (x86_64, Apple Silicon)
# 使用方式：curl -fsSL https://raw.githubusercontent.com/cawa0505/OpenDocuments/main/install.sh | sh

set -eu

REPO="cawa0505/OpenDocuments"
REPO_URL="https://github.com/$REPO"
APP_NAME="opendoc"

# 決定安裝路徑，優先使用 XDG_BIN_HOME，次之使用 ~/.local/bin
if [ -n "${XDG_BIN_HOME:-}" ]; then
    INSTALL_DIR="$XDG_BIN_HOME"
else
    INSTALL_DIR="$HOME/.local/bin"
fi

# 建立安裝目錄
mkdir -p "$INSTALL_DIR"

# 偵測系統與硬體架構
OS="$(uname -s)"
ARCH="$(uname -m)"

# 轉換架構名稱以符合 GitHub Release 資產命名
case "$ARCH" in
    x86_64|amd64)
        ARCH_RELEASE="x86_64"
        ;;
    arm64|aarch64)
        ARCH_RELEASE="aarch64"
        ;;
    *)
        echo "❌ 錯誤：不支援的系統架構: $ARCH" >&2
        echo "支援的架構包括：x86_64, aarch64 (Apple Silicon / ARM64)" >&2
        exit 1
        ;;
esac

# 轉換 OS 名稱以符合 GitHub Release 資產命名
case "$OS" in
    Linux)
        OS_RELEASE="linux"
        ;;
    Darwin)
        OS_RELEASE="macos"
        ;;
    *)
        echo "❌ 錯誤：不支援的作業系統: $OS" >&2
        echo "支援的作業系統包括：Linux, macOS" >&2
        exit 1
        ;;
esac

ASSET_NAME="opendoc-${OS_RELEASE}-${ARCH_RELEASE}"
DOWNLOAD_URL="${REPO_URL}/releases/latest/download/${ASSET_NAME}"
CHECKSUMS_URL="${REPO_URL}/releases/latest/download/checksums.txt"

# 建立暫存資料夾
TMP_DIR="$(mktemp -d)"
clean_up() {
    rm -rf "$TMP_DIR"
}
trap clean_up EXIT

# 選擇下載工具
download_file() {
    url="$1"
    output="$2"
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL -o "$output" "$url"
    elif command -v wget >/dev/null 2>&1; then
        wget -q -O "$output" "$url"
    else
        echo "❌ 錯誤：此系統未安裝 curl 或 wget，無法進行下載。" >&2
        exit 1
    fi
}

echo "⬇️ 正在從 GitHub 下載最新版本 OpenDocuments..."
download_file "$DOWNLOAD_URL" "$TMP_DIR/$ASSET_NAME"

# 嘗試取得並驗證 Checksum
echo "🔍 正在驗證 Checksum 安全性..."
if download_file "$CHECKSUMS_URL" "$TMP_DIR/checksums.txt" >/dev/null 2>&1; then
    # 偵測雜湊計算工具
    if command -v sha256sum >/dev/null 2>&1; then
        HASH_TOOL="sha256sum"
    elif command -v shasum >/dev/null 2>&1; then
        HASH_TOOL="shasum -a 256"
    else
        HASH_TOOL=""
    fi

    if [ -n "$HASH_TOOL" ]; then
        WANTED_HASH="$(grep "${ASSET_NAME}$" "$TMP_DIR/checksums.txt" | cut -d' ' -f1)"
        CALCULATED_HASH="$($HASH_TOOL "$TMP_DIR/$ASSET_NAME" | cut -d' ' -f1)"
        
        if [ -n "$WANTED_HASH" ] && [ "$WANTED_HASH" = "$CALCULATED_HASH" ]; then
            echo "✅ Checksum 驗證成功！"
        else
            echo "❌ 錯誤：下載的二進位檔 Checksum 驗證不符！安全攔截中斷安裝。" >&2
            exit 1
        fi
    else
        echo "⚠️ 警報：本機未安裝 sha256sum 或 shasum，跳過 Checksum 安全校驗。"
    fi
else
    echo "⚠️ 警報：無法取得 checksums.txt 檔案，跳過安全校驗。"
fi

# 移動檔案至安裝目錄並設定執行權限
mv "$TMP_DIR/$ASSET_NAME" "$INSTALL_DIR/$APP_NAME"
chmod +x "$INSTALL_DIR/$APP_NAME"

# 檢查環境變數並提示加入 PATH
IN_PATH=0
case ":$PATH:" in
    *:"$INSTALL_DIR":*)
        IN_PATH=1
        ;;
esac

if [ "$IN_PATH" -eq 0 ]; then
    SHELL_NAME="$(basename "$SHELL")"
    PROFILE_FILE=""
    
    case "$SHELL_NAME" in
        zsh)
            PROFILE_FILE="$HOME/.zshrc"
            ;;
        bash)
            PROFILE_FILE="$HOME/.bashrc"
            ;;
    esac

    echo ""
    echo "⚠️ 提示：'$INSTALL_DIR' 尚未加入您的 PATH 環境變數。"
    if [ -n "$PROFILE_FILE" ] && [ -f "$PROFILE_FILE" ]; then
        echo "您可以執行以下指令將其永久加入您的環境變數中："
        echo "  echo 'export PATH=\"$INSTALL_DIR:\$PATH\"' >> $PROFILE_FILE"
        echo "  source $PROFILE_FILE"
    else
        echo "請手動將 '$INSTALL_DIR' 加入您的系統 PATH 環境變數。"
    fi
fi

# 預先建立 XDG 標準目錄
mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/opendocuments"
mkdir -p "${XDG_DATA_HOME:-$HOME/.local/share}/opendocuments"
mkdir -p "${XDG_CACHE_HOME:-$HOME/.cache}/opendocuments"

echo ""
echo "🎉 OpenDocuments 安裝完成！"
echo "您可以輸入以下指令開始使用："
echo "  opendoc --help"
