# Rust File Backup Service

A lightweight and automated file backup service written in **Rust**, designed to upload local files to **Dropbox** while maintaining logs, tracking uploaded files, and automatically refreshing API tokens.

This is a **Rust port** of an original Python backup script, rewritten for performance, reliability, and full async operation using the `tokio` runtime.

---

## ⚙️ Prerequisites (Rust on Windows with MinGW + MSYS2)

Before running this project, you need a working Rust environment configured with the GNU toolchain.

### 🧩 1. Install MSYS2 + MinGW
1. Download and install **MSYS2** from [https://www.msys2.org](https://www.msys2.org)
2. Open the **MSYS2 UCRT64 terminal** and install the required toolchain:
   ```
   pacman -S --needed mingw-w64-ucrt-x86_64-gcc
   ```
3. Add MinGW’s `bin` directory to your Windows PATH:
   ```
   C:\msys64\ucrt64\bin
   ```

### 2. Install Rust (GNU toolchain)
```
rustup default stable-x86_64-pc-windows-gnu
```

Verify everything works:
```
rustc --version
cargo --version
gcc --version
```

You’re now ready to build Rust projects using MinGW.

---

## 📦 Project Overview

This service handles **automated backups of a local file-based library** to Dropbox.

It performs:
- Automated file discovery (supports recursion)
- File renaming (spaces → underscores)
- Upload tracking via log file
- Token auto-refresh (using Dropbox refresh tokens)
- File movement after successful upload

---

## 🧰 Environment Configuration

The service requires an `.env` file in the project root.

### Example `.env`

```env
API_ADDRESS=https://content.dropboxapi.com/2/files/upload
API_REFRESH_ADDRESS=https://api.dropboxapi.com/oauth2/token
DROPBOX_DIR=/Apps/YourAppName
APP_KEY=your_app_key
APP_SECRET=your_app_secret
REFRESH_TOKEN=your_refresh_token

UPLOADED_FILES_LOG=./uploaded_files.log
UPLOADED_DIRECTORY=./uploaded
CURRENT_DIRECTORY=./to_send
FILE_EXTENSIONS=.epub,.mobi,.txt
RECURSE=True
SKIP_DIRS=processed,uploaded
SHORT_TOKEN_FILE=./short_token.txt
```

> ⚠️ The program will automatically request a new short-lived Dropbox access token on first run and create `short_token.txt` for you.

---

## 🚀 Running the Program

### Development mode
```
cargo run
```

### Optimized release build
```
cargo run --release
```

### Logs
By default, logs print to the console.
To save logs to a file:
```bash
cargo run > app.log 2>&1
```

---

## 🗂️ Directory Structure

```
fs_library/
├── Cargo.toml
├── src/
│   └── main.rs
├── .env
├── uploaded_files.log
└── short_token.txt
```

---

## 🔄 How It Works

1. The service scans the directory defined in `CURRENT_DIRECTORY` for files matching `FILE_EXTENSIONS`.
2. Each file is uploaded to your Dropbox directory (`DROPBOX_DIR`).
3. After successful upload:
   - The file’s full path is appended to `UPLOADED_FILES_LOG`.
   - The file is moved to the directory defined by `UPLOADED_DIRECTORY`.
4. If a file upload returns a 401 error (token expired), the service automatically requests a new token and retries once.

---

## 🧱 Building Without Running

```
cargo build --release
```

Binary will be available at:
```
target/release/dropbox_backup_service.exe
```

You can then run it directly:
```
target/release/dropbox_backup_service.exe
```

---

## 🧹 Recommended `.gitignore`

```gitignore
# Rust build
/target/

# Logs
*.log

# Secrets
.env
short_token.txt
uploaded_files.log

# Editor files
.vscode/
.idea/
```

---

## 🧠 Notes

- Works with Dropbox API v2.
- Automatically handles refresh tokens and short-lived access tokens.
- All operations are logged via `env_logger`.
- Fully async (based on `tokio` and `reqwest`).

---
