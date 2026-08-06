# 🚀 Deployment & Service Auto-Start Guide

OpenDocuments compiles into a high-performance **Single Binary (`opendoc`)** containing both the Rust backend and the embedded React WebUI. You do not need Node.js, Docker, or any external assets to run it in production.

This guide details how to install the single binary and configure it to run automatically on system startup across **macOS, Windows, and Linux**.

---

## 📦 1. Installation

### Quick Direct Install (Linux & macOS)
```bash
curl -fsSL https://raw.githubusercontent.com/cawa0505/OpenDocuments/main/install.sh | sh
```

### Install via Cargo (Rust Developers)
::: info
Requires `protoc` (protobuf compiler) installed on your system. We pass `RUSTC_BOOTSTRAP=1` to allow raising the internal compiler recursion limit for compiling heavy asynchronous dependency trees like `lance` and `arrow`.
:::
```bash
RUSTC_BOOTSTRAP=1 RUSTFLAGS="-Z min-recursion-limit=512" cargo install --git https://github.com/cawa0505/OpenDocuments opendoc --force
```

### Build from Source
```bash
# Build frontend web assets and compile the Rust binary in release mode
make install
```

Verify the installation:
```bash
opendoc --version
```

To run the unified server manually:
```bash
opendoc start --port 3000
```
Your WebUI will be served instantly at `http://localhost:3000` with zero external directory requirements!

---

## 🖥️ 2. Platform-Specific Auto-Start Configurations

### 🍎 A. macOS (using `launchd`)

For macOS, the cleanest way to run `opendoc` as a persistent background daemon that starts automatically on user login is using a `LaunchAgent`.

1. Create a plist configuration file under your user's LaunchAgents directory:
   ```bash
   nano ~/Library/LaunchAgents/org.opendocuments.opendoc.plist
   ```

2. Paste the following configuration (replace `YOUR_USERNAME` with your actual macOS username):
   ```xml
   <?xml version="1.0" encoding="UTF-8"?>
   <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
   <plist version="1.0">
   <dict>
       <key>Label</key>
       <string>org.opendocuments.opendoc</string>
       <key>ProgramArguments</key>
       <array>
           <string>/Users/YOUR_USERNAME/.cargo/bin/opendoc</string>
           <string>start</string>
           <string>--port</string>
           <string>3000</string>
       </array>
       <key>RunAtLoad</key>
       <true/>
       <key>KeepAlive</key>
       <true/>
       <key>StandardOutPath</key>
       <string>/Users/YOUR_USERNAME/.opendocuments/stdout.log</string>
       <key>StandardErrorPath</key>
       <string>/Users/YOUR_USERNAME/.opendocuments/stderr.log</string>
       <key>EnvironmentVariables</key>
       <dict>
           <key>PATH</key>
           <string>/usr/bin:/bin:/usr/sbin:/sbin:/usr/local/bin</string>
       </dict>
   </dict>
   </plist>
   ```

3. Load and start the background agent:
   ```bash
   launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/org.opendocuments.opendoc.plist
   ```

4. To stop or remove the agent:
   ```bash
   launchctl bootout gui/$(id -u) ~/Library/LaunchAgents/org.opendocuments.opendoc.plist
   ```

---

### 🪟 B. Windows (using Windows Task Scheduler or Startup)

For Windows, you can achieve persistent background execution using **Windows Task Scheduler** or **NSSM** (Non-Sucking Service Manager).

#### Option 1: Windows Task Scheduler (Recommended)
This method allows running silently in the background on startup without a visible Command Prompt window.

1. Press `Win + R`, type `taskschd.msc`, and press Enter.
2. Click **Create Basic Task...** in the right sidebar.
3. **Name**: `OpenDocuments Daemon`
4. **Trigger**: Select **When I log on**.
5. **Action**: Select **Start a program**.
6. **Program/script**: Browse to your compiled binary, e.g., `C:\Users\YOUR_USERNAME\.cargo\bin\opendoc.exe` (or your chosen path).
7. **Add arguments**: `start --port 3000`
8. Finish the wizard. Then right-click the newly created task in the list, choose **Properties**, and:
   - On the **General** tab, check **Run whether user is logged on or not** or check **Run with highest privileges** if needed.
   - On the **Conditions** tab, uncheck **Start the task only if the computer is on AC power**.

#### Option 2: NSSM (Run as a true Windows Service)
If you require OpenDocuments to run as a system-level Windows Service that restarts automatically:

1. Download [NSSM](https://nssm.cc/) and place it in your PATH.
2. Open Command Prompt as Administrator and execute:
   ```cmd
   nssm install OpenDocuments
   ```
3. In the GUI window that pops up:
   - **Path**: `C:\Users\YOUR_USERNAME\.cargo\bin\opendoc.exe`
   - **Arguments**: `start --port 3000`
4. Click **Install service**. OpenDocuments will now run as a background service managed by Windows Services (`services.msc`).

---

### 🐧 C. Linux (using `systemd`)

For Linux servers or workstations, creating a standard systemd service is the gold standard.

1. Create a new service file:
   ```bash
   sudo nano /etc/systemd/system/opendoc.service
   ```

2. Paste the following configuration (replace `YOUR_USERNAME` and `YOUR_GROUP` with your Linux username and group):
   ```ini
   [Unit]
   Description=OpenDocuments Unified Server
   After=network.target

   [Service]
   Type=simple
   User=YOUR_USERNAME
   Group=YOUR_GROUP
   WorkingDirectory=/home/YOUR_USERNAME
   ExecStart=/home/YOUR_USERNAME/.cargo/bin/opendoc start --port 3000
   Restart=always
   RestartSec=5
   StandardOutput=append:/home/YOUR_USERNAME/.opendocuments/stdout.log
   StandardError=append:/home/YOUR_USERNAME/.opendocuments/stderr.log

   [Install]
   WantedBy=multi-user.target
   ```

3. Reload systemd, enable the service to run on boot, and start it:
   ```bash
   sudo systemctl daemon-reload
   sudo systemctl enable opendoc.service
   sudo systemctl start opendoc.service
   ```

4. Check the live status and logs:
   ```bash
   sudo systemctl status opendoc.service
   journalctl -u opendoc.service -f
   ```

---

## 🔒 3. Production Hardening Checklist

When deploying OpenDocuments as a public or team service, always ensure the following safety measures are in place:

### Reverse Proxy (Nginx Example)
To secure your instance with SSL/TLS and run on a public port, configure an Nginx reverse proxy. Ensure SSE buffering is disabled for smooth response streaming:

```nginx
server {
    listen 443 ssl;
    server_name docs.yourdomain.com;

    ssl_certificate /etc/letsencrypt/live/docs.yourdomain.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/docs.yourdomain.com/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:3000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # Mandatory SSE streaming support
        proxy_buffering off;
        proxy_cache off;
        proxy_read_timeout 300s;
    }
}
```

### Security Best Practices
- **SQLite Database Backup**: Set up a cron job to backup the workspace databases residing under your `~/.opendocuments/` directory.
- **Firewall Isolation**: If `opendoc` is only accessed by local developers or an internal Tauri desktop shell, bind it strictly to loopback (`127.0.0.1`) or block external port 3000 via `ufw` / `iptables`.
