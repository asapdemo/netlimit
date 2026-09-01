# NetLimit

Interactive **btop-style** TUI for system-wide network traffic control on **Linux**.

Shape **download**, **upload**, **packet loss**, **delay**, and **jitter** using native `tc` / `netem` / IFB. Measure the path with ICMP path-quality graphs and a full-screen **Cloudflare** speed test.

| | |
|--|--|
| Language | Rust + [ratatui](https://ratatui.rs) |
| Binary | `netlimit` |
| OS | Linux (requires `iproute2`) |
| Privileges | Root for Apply / Reset (`sudo`) |

**Documentation**

- English: [docs/en/README.md](docs/en/README.md)
- Українська: [docs/uk/README.md](docs/uk/README.md)

---

## Features

- Dark TUI inspired by btop
- Rate limits (Mbps), packet loss (%), delay & jitter (ms)
- Interface picker (list + default route marker)
- Presets: **No limits**, **4G**, **3G**, **Starlink** (+ custom save/delete)
- Path quality: live packet-loss and RTT graphs (ICMP to `1.1.1.1`)
- Cloudflare speed test (per-phase duration, re-run each graph)
- Speed-test history on disk
- Mouse + keyboard controls

---

## Requirements

| Requirement | Notes |
|-------------|--------|
| **Linux** | Traffic control uses `tc` / `ip` |
| **iproute2** | Provides `tc` and `ip` |
| **Root** | Needed to apply or reset limits |
| **Rust 1.74+** | Only if building from source |
| **ping** | Used for path-quality sampling (usually preinstalled) |

Install `iproute2`:

```bash
# Debian / Ubuntu / Raspberry Pi OS
sudo apt update && sudo apt install -y iproute2

# Arch Linux
sudo pacman -S --needed iproute2

# Fedora
sudo dnf install iproute-tc
```

---

## Download

### One-command install (Raspberry Pi / Linux)

After a GitHub Release is published, install the matching binary on **64-bit Raspberry Pi OS** (`aarch64`) or **x86_64 Linux**:

```bash
curl -sSfL https://raw.githubusercontent.com/virtuoz-afk/netlimit/main/install.sh | sh
sudo netlimit
```

The script detects the CPU, downloads the latest release, checks the SHA-256 checksum, and installs `netlimit` to `/usr/local/bin`.

Pin a version:

```bash
curl -sSfL https://raw.githubusercontent.com/virtuoz-afk/netlimit/main/install.sh | \
  NETLIMIT_VERSION=v0.2.0 sh
```

Raspberry Pi must be running a **64-bit** OS (`uname -m` → `aarch64`). 32-bit Raspberry Pi OS is not built.

Two other options:

### 1. Clone, build, and run locally

Needs **Rust 1.74+**.

```bash
git clone https://github.com/virtuoz-afk/netlimit.git
cd netlimit
cargo build --release
sudo ./target/release/netlimit
```

Optional — install so `sudo netlimit` works from any directory:

```bash
sudo install -m 755 target/release/netlimit /usr/local/bin/netlimit
sudo netlimit
```

`sudo` uses a restricted `PATH`, so either run the binary by full path (`sudo ./target/release/netlimit`) or install it into `/usr/local/bin`.

### 2. Prebuilt binaries (manual)

Download a Linux archive from [Releases](https://github.com/virtuoz-afk/netlimit/releases) and run it.

**x86_64** (typical PC / Arch / Ubuntu desktop):

```bash
curl -LO https://github.com/virtuoz-afk/netlimit/releases/latest/download/netlimit-linux-x86_64.tar.gz
tar -xzf netlimit-linux-x86_64.tar.gz
sudo ./netlimit-linux-x86_64/netlimit
```

**aarch64** (Raspberry Pi 64-bit, ARM servers):

```bash
curl -LO https://github.com/virtuoz-afk/netlimit/releases/latest/download/netlimit-linux-aarch64.tar.gz
tar -xzf netlimit-linux-aarch64.tar.gz
sudo ./netlimit-linux-aarch64/netlimit
```

Optional — install system-wide (x86_64 example):

```bash
sudo install -m 755 netlimit-linux-x86_64/netlimit /usr/local/bin/netlimit
sudo netlimit
```

### Verify

```bash
netlimit --version
which netlimit
sudo netlimit --no-sudo   # opens UI without re-exec (Apply needs root)
```

---

## Quick start

### Interactive TUI (default)

```bash
sudo netlimit
# same as:
sudo netlimit tui
```

1. Select a **network interface** (left list).  
2. Adjust **Download / Upload / Loss / Delay / Jitter**, or pick a **preset**.  
3. Press **Apply** (`a`) to enforce limits.  
4. Optional: open **Speed Test** (`t`) to measure under those limits.  
5. Press **Reset** (`r`) when finished.

Without root the app tries to re-run itself with `sudo` (absolute path).

### CLI commands

```bash
netlimit --help

# Shape traffic (needs root; re-execs with sudo unless --no-sudo)
sudo netlimit apply --download 10 --upload 2 --loss 1 --delay 50 --jitter 10
sudo netlimit apply --preset 4G -i wlan0
sudo netlimit reset

# Inspect (no root required)
netlimit status
netlimit status --json
netlimit interfaces
netlimit presets
netlimit history -n 10

# Cloudflare speed test from the shell
netlimit speedtest --duration 5
netlimit speedtest --scope download -t 8 --json
```

| Command | Description |
|---------|-------------|
| `tui` | Interactive UI (default if no command) |
| `apply` | Apply limits (`--download`, `--upload`, `--loss`, `--delay`, `--jitter`, `--preset`) |
| `reset` | Clear shaping on the interface |
| `status` | Show current limits |
| `interfaces` | List NICs (`ifaces` alias) |
| `presets` | List built-in + custom presets |
| `speedtest` | Non-interactive Cloudflare test |
| `history` | Print saved speed-test results |

Global flags: `-i` / `--interface`, `--no-sudo`, `--json`.

---

## Controls (summary)

| Key | Action |
|-----|--------|
| `↑` `↓` / `Tab` | Select metric |
| `←` `→` / `+` `−` | Adjust value |
| `Shift` + adjust | Coarse step |
| `1`–`9` | Load preset |
| `s` | Save custom preset |
| `x` / `Del` | Delete selected custom preset |
| `a` | Apply |
| `r` | Reset |
| `t` | Speed test screen |
| `h` | History |
| `y` / `j` | Focus delay / jitter |
| `i` / `[` `]` | Cycle interface |
| `q` / `Esc` | Quit |

Full guide: [docs/en/README.md](docs/en/README.md) · [docs/uk/README.md](docs/uk/README.md)

---

## How it works

| Control | Mechanism |
|---------|-----------|
| **Upload** | HTB + netem on interface egress |
| **Download** | Ingress redirected to `ifb0`, then HTB + netem |
| **Loss / delay / jitter** | `netem` on shaped paths |
| **Reset** | Delete root/ingress qdiscs; bring `ifb0` down |

---

## Data on disk

| Path | Purpose |
|------|---------|
| `~/.config/netlimit/presets.json` | Custom presets |
| `~/.config/netlimit/speedtest_history.json` | Speed-test history |

---

## Safety

- Rules affect **all** traffic on the selected interface.
- Always **Reset** when you are done testing.
- Active limits change Cloudflare speed-test results (by design).

---

## Project layout

```
netlimit/
├── Cargo.toml
├── README.md                 # this file
├── install.sh                # one-command Linux / Raspberry Pi installer
├── docs/
│   ├── en/README.md          # English user guide
│   └── uk/README.md          # Ukrainian user guide
└── src/
    ├── main.rs
    ├── cli.rs                # clap subcommands
    ├── app.rs
    ├── ui.rs
    ├── tc.rs
    ├── speedtest.rs
    ├── monitor.rs
    ├── presets.rs
    ├── history.rs
    └── …
```

---

## License

MIT
