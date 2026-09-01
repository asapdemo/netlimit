# NetLimit — User Guide (English)

NetLimit is an interactive terminal application (TUI) for **Linux** that lets you **simulate poor or mobile networks** and **measure** the result.

You can limit bandwidth, inject packet loss, add latency and jitter, watch live path quality, and run a Cloudflare-based speed test — all from one dark, btop-style interface.

---

## Table of contents

1. [What NetLimit does](#1-what-netlimit-does)
2. [Installation](#2-installation)
3. [First run](#3-first-run)
4. [Main screen](#4-main-screen)
5. [Presets](#5-presets)
6. [Path quality](#6-path-quality)
7. [Cloudflare speed test](#7-cloudflare-speed-test)
8. [History](#8-history)
9. [Keyboard & mouse](#9-keyboard--mouse)
10. [How traffic control works](#10-how-traffic-control-works)
11. [Troubleshooting](#11-troubleshooting)
12. [Safety](#12-safety)

---

## 1. What NetLimit does

| Capability | Description |
|------------|-------------|
| **Download limit** | Cap ingress rate (Mbps) via IFB + HTB |
| **Upload limit** | Cap egress rate (Mbps) via HTB |
| **Packet loss** | Random drop percentage (`netem loss`) |
| **Delay** | Base latency in milliseconds |
| **Jitter** | Delay variation in milliseconds |
| **Path quality** | Live ICMP loss % and RTT graphs to `1.1.1.1` |
| **Speed test** | Cloudflare HTTP throughput + latency |
| **History** | Saved speed-test results on disk |

Typical uses: app QA under bad networks, demos, ISP checks under artificial caps, learning Linux traffic control.

---

## 2. Installation

### Dependencies

```bash
# Debian / Ubuntu / Raspberry Pi OS
sudo apt update && sudo apt install -y iproute2

# Arch Linux
sudo pacman -S --needed iproute2

# Fedora
sudo dnf install iproute-tc
```

You also need a working `ping` (usually already installed).

### One-command install (Raspberry Pi / Linux)

After a GitHub Release is published:

```bash
curl -sSfL https://raw.githubusercontent.com/virtuoz-afk/netlimit/main/install.sh | sh
sudo netlimit
```

Works on 64-bit Raspberry Pi OS (`aarch64`) and x86_64 Linux. The script verifies the release checksum and installs to `/usr/local/bin`.

32-bit Raspberry Pi OS is not supported.

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

### 2. Prebuilt binaries

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

### Useful flags and CLI commands

```bash
netlimit --version
netlimit --help

# TUI (default)
sudo netlimit
sudo netlimit tui -i wlan0
sudo netlimit --no-sudo         # do not re-exec (Apply needs root)

# Shape / clear (root)
sudo netlimit apply --download 10 --upload 2 --loss 1
sudo netlimit apply --preset 4G
sudo netlimit reset

# Inspect (no root)
netlimit status
netlimit interfaces
netlimit presets
netlimit history
netlimit speedtest --duration 5 --scope full
```

| Command | Purpose |
|---------|---------|
| `tui` | Interactive UI (default) |
| `apply` | Set download/upload/loss/delay/jitter or `--preset` |
| `reset` | Remove shaping |
| `status` | Current limits |
| `interfaces` | List NICs |
| `presets` | List presets |
| `speedtest` | Cloudflare test in the terminal |
| `history` | Saved results |

Global: `-i` / `--interface`, `--no-sudo`, `--json`.

### Sudo and PATH

`sudo` may not find binaries in `./target/release` or `~/.cargo/bin`. Prefer:

```bash
sudo /full/path/to/netlimit
# or install into /usr/local/bin
```

---

## 3. First run

1. Launch with `sudo netlimit`.
2. Choose a **network interface** (left list; default route is marked ★).
3. Set metrics or load a **preset**.
4. Press **Apply** (`a`).
5. Optionally open **Speed Test** (`t`).
6. When finished, press **Reset** (`r`).

---

## 4. Main screen

Layout (top to bottom):

| Area | Content |
|------|---------|
| **Top left** | Interface list |
| **Top right** | **Current applied limits** (what is enforced now) |
| **Presets** | Quick profiles |
| **Metrics** | Download, Upload, Loss, Delay, Jitter |
| **Path quality** | Packet loss + latency graphs |
| **Actions** | Apply, Reset, History, Quit |
| **Banner** | Status messages |
| **Footer** | Key hints |

### Metrics

| Control | Unit | Meaning |
|---------|------|---------|
| **↓ Download** | Mbps | Max download (0 = unlimited) |
| **↑ Upload** | Mbps | Max upload (0 = unlimited) |
| **⚠ Loss** | % | Packet loss |
| **⏱ Delay** | ms | Base latency |
| **∿ Jitter** | ms | Latency variation |

Values in the UI are a **draft** until you **Apply**.  
The **Current applied limits** panel shows what is actually on the wire.

---

## 5. Presets

Built-in defaults:

| Preset | Typical intent |
|--------|----------------|
| **No limits** | Clear shaping |
| **4G** | Mobile LTE-class rates + moderate delay |
| **3G** | Slower mobile + more delay/loss |
| **Starlink** | High throughput, satellite-like RTT/jitter |

| Action | How |
|--------|-----|
| Load | Click chip or keys `1`–`9` |
| Save current as custom | `s` or **+ Save** |
| Delete custom | **×** on chip, or select + `x` / `Del` |

Custom presets: `~/.config/netlimit/presets.json`.

---

## 6. Path quality

Uses system **`ping`** to **1.1.1.1** (Cloudflare DNS).

| Graph | Meaning |
|-------|---------|
| **Packet loss** | % of failed pings in a rolling window; quality label (Excellent → Bad) |
| **Latency (RTT)** | Round-trip time in ms; quality label |

Also shows live interface throughput (↓/↑ Mbps from `/proc/net/dev`).

---

## 7. Cloudflare speed test

Open with **`t`** or the **Cloudflare Speed Test** button.

### Duration

- **Sec/phase** (default **5**): each of latency, download, upload runs for that many seconds.
- Full run ≈ **3 × sec/phase** (e.g. 5s → ~15s total).
- Adjust with `−` / `+` or `←` / `→` (Shift = ±5).

### Controls

| Control | Action |
|---------|--------|
| **Run all** | Latency + download + upload |
| **↺ ↓ / ↑ / lat** | Re-run only that phase |
| **Back** / `Esc` | Return to main screen |

Graphs update as probes complete. Active NetLimit rules **affect** results (expected).

---

## 8. History

Press **`h`** or **Hist**.

- Lists recent Cloudflare runs (time, Mbps, latency, interface, limits snapshot).
- Stored in `~/.config/netlimit/speedtest_history.json`.
- `Esc` / click returns to main.

---

## 9. Keyboard & mouse

### Main screen

| Key | Action |
|-----|--------|
| `↑` `↓` `Tab` | Select metric |
| `←` `→` `+` `−` | Adjust |
| `Shift` + adjust | Larger step |
| `d` `u` `l` `y` `j` | Focus Download / Upload / Loss / Delay / Jitter |
| `1`–`9` | Load preset |
| `s` | Save preset |
| `x` / `Del` | Delete custom preset |
| `a` | Apply |
| `r` | Reset all limits |
| `t` | Speed test |
| `h` | History |
| `i` `[` `]` | Cycle interface |
| `q` / `Esc` | Quit |

### Mouse

- Click interface rows, presets, ±, sliders, buttons.
- Scroll wheel over a metric card to nudge values.

---

## 10. How traffic control works

| Direction | Mechanism |
|-----------|-----------|
| Upload | HTB class + netem on real interface **egress** |
| Download | Ingress redirected to **ifb0**, then HTB + netem |
| Loss / delay / jitter | `netem` parameters |
| Reset | Remove qdiscs; take `ifb0` down |

This is **system-wide** for the selected interface (not per browser tab).

---

## 11. Troubleshooting

| Problem | What to try |
|---------|-------------|
| `sudo: netlimit: command not found` | Use full path or install to `/usr/local/bin` |
| Apply fails | Run as root; check `tc` / `ip` installed |
| Download limit has no effect | `sudo modprobe ifb`; correct interface selected |
| Speed test fails | Network/DNS; firewall; try longer duration |
| Path quality stuck | Ensure `ping` works: `ping -c 1 1.1.1.1` |
| Rules left after crash | `sudo netlimit` → **Reset**, or manually clear `tc qdisc` |

Inspect kernel state:

```bash
tc qdisc show
tc qdisc show dev ifb0
tc class show dev eth0
```

---

## 12. Safety

- Limits apply to **all** traffic on the chosen interface.
- Always **Reset** after testing.
- Do not leave harsh loss/delay on production machines.

---

## License

MIT — see project root.
