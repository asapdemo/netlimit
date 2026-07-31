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

## Install

### Option A — Build from source (recommended for development)

```bash
# 1. Clone
git clone https://github.com/virtuoz-afk/netlimit.git
cd netlimit

# 2. Build release binary
cargo build --release

# 3. Run with full path (always works with sudo)
sudo ./target/release/netlimit

# 4. Optional: install system-wide
sudo install -m 755 target/release/netlimit /usr/local/bin/netlimit
sudo netlimit
```

### Option B — Install the release binary into `/usr/local/bin`

After building (or downloading a release asset if available):

```bash
sudo install -m 755 ./target/release/netlimit /usr/local/bin/netlimit
sudo netlimit
```

Why a **full path or `/usr/local/bin` link**?  
`sudo` uses a restricted `PATH` and often cannot see `./target/release` or `~/.cargo/bin`.

### Option C — Cargo install (from local tree)

```bash
cd netlimit
cargo install --path .
# then:
sudo "$(which netlimit)"
# or:
sudo ln -sf ~/.cargo/bin/netlimit /usr/local/bin/netlimit
sudo netlimit
```

### Option D — Prebuilt GitHub Releases (if published)

If [Releases](https://github.com/virtuoz-afk/netlimit/releases) provide archives:

```bash
# x86_64 (typical PC / Arch / Ubuntu desktop)
curl -LO https://github.com/virtuoz-afk/netlimit/releases/latest/download/netlimit-linux-x86_64.tar.gz
tar -xzf netlimit-linux-x86_64.tar.gz
sudo install -m 755 netlimit-linux-x86_64/netlimit /usr/local/bin/netlimit
sudo netlimit
```

```bash
# aarch64 (Raspberry Pi 64-bit, ARM servers)
curl -LO https://github.com/virtuoz-afk/netlimit/releases/latest/download/netlimit-linux-aarch64.tar.gz
tar -xzf netlimit-linux-aarch64.tar.gz
sudo install -m 755 netlimit-linux-aarch64/netlimit /usr/local/bin/netlimit
sudo netlimit
```

> Adjust archive names to match the actual release assets.

### Verify install

```bash
netlimit --version
which netlimit
sudo netlimit --no-sudo   # opens UI without re-exec (Apply needs root)
```

---

## Quick start

```bash
sudo netlimit
```

1. Select a **network interface** (left list).  
2. Adjust **Download / Upload / Loss / Delay / Jitter**, or pick a **preset**.  
3. Press **Apply** (`a`) to enforce limits.  
4. Optional: open **Speed Test** (`t`) to measure under those limits.  
5. Press **Reset** (`r`) when finished.

Without root the app tries to re-run itself with `sudo` (absolute path).

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
├── docs/
│   ├── en/README.md          # English user guide
│   └── uk/README.md          # Ukrainian user guide
└── src/
    ├── main.rs
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
