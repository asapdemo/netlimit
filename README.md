# NetLimit

Interactive **btop-style** TUI for system-wide Linux network traffic control.

Built with **Rust** + **ratatui**. Limits download, upload, and packet loss via `tc` / `netem` / IFB.

## Features

- Dense dark dashboard (btop-inspired)
- **Shape:** download / upload Mbps, packet loss, **delay**, **jitter** (`tc` + `netem` + IFB)
- Full interface list (click to select)
- Keyboard + mouse (`−`/`+`, sliders, buttons)
- **Presets:** No limits, 4G, 3G, Starlink (+ custom save/delete)
- **Path quality** — live ICMP loss + latency graphs
- **Cloudflare speed test** — full-screen graphs, per-phase re-run, duration
- **History [h]** — last speed tests on disk
- Single binary: `netlimit`

## Requirements

- Linux + `iproute2` (`tc`, `ip`)
- Root for Apply / Reset
- Rust 1.74+ (only if building from source)

## Install (prebuilt binaries)

GitHub Actions builds release archives for:

| Archive | Use on |
|---------|--------|
| `netlimit-linux-x86_64.tar.gz` | **Arch Linux**, Ubuntu/Debian/Fedora x86_64, most PCs/servers |
| `netlimit-linux-aarch64.tar.gz` | **Raspberry Pi** (64-bit OS), other ARM64 Linux |

### Arch Linux (x86_64)

```bash
# From a GitHub Release asset (replace VERSION / URL as needed)
curl -LO https://github.com/virtuoz-afk/netlimit/releases/latest/download/netlimit-linux-x86_64.tar.gz
tar -xzf netlimit-linux-x86_64.tar.gz
sudo install -m 755 netlimit-linux-x86_64/netlimit /usr/local/bin/netlimit
sudo pacman -S --needed iproute2
sudo netlimit
```

### Raspberry Pi (aarch64, 64-bit OS)

```bash
curl -LO https://github.com/virtuoz-afk/netlimit/releases/latest/download/netlimit-linux-aarch64.tar.gz
tar -xzf netlimit-linux-aarch64.tar.gz
sudo install -m 755 netlimit-linux-aarch64/netlimit /usr/local/bin/netlimit
sudo apt update && sudo apt install -y iproute2
sudo netlimit
```

> Use the **aarch64** build on 64-bit Raspberry Pi OS. 32-bit Pi OS is not built by default.

### How releases are produced

Push a version tag to trigger packaging and a GitHub Release:

```bash
git tag v0.1.0
git push origin v0.1.0
```

Workflow: [`.github/workflows/release.yml`](.github/workflows/release.yml)  
(also runs on PRs / pushes for CI artifacts; manual **workflow_dispatch** can create a release.)

## Build from source

```bash
cargo build --release

# absolute path works with sudo (secure_path safe)
sudo ./target/release/netlimit

# optional system link
sudo ln -sf "$(pwd)/target/release/netlimit" /usr/local/bin/netlimit
sudo netlimit
```

Development:

```bash
cargo run -- --no-sudo          # UI without elevation
cargo run -- -i wlan0
```

If launched without root, the app re-execs via `sudo` using its absolute path.

## Controls

### Metric cards
- **`[ − ]` / `[ + ]`** — step values
- **Slider** — click or drag (0–200 Mbps / 0–100% loss)
- Scroll wheel over a card to nudge

### Interfaces
- Full list with state (`up` / `down`) and default marker
- Click a row to select · `[` / `]` or `i` to cycle

### Presets
| Action | How |
|--------|-----|
| Load | `1`–`9` or click chip |
| Save | `s` or **+ Save** |
| Delete custom | click **`×`**, or select + `x` / `Del` |

Customs: `~/.config/netlimit/presets.json`.  
Loading only fills the draft — press **Apply** to enforce.

### Keys

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

### Path quality & speed test

| Feature | Where |
|---------|--------|
| LOSS sparkline + RTT | Main screen **PATH QUALITY** panel (ping `1.1.1.1`) |
| Live ↓/↑ Mbps readout | Same panel (from `/proc/net/dev`) |
| Cloudflare test | Full-screen (`t` or the speed-test button) |

**Speed test screen:** set duration **5–120s** with `←`/`→` or `−`/`+`, **Run** with Enter/`t`, **Back** with Esc/`b`.  
Longer duration uses larger payloads for more stable Mbps. Active NetLimit rules affect results.

## How traffic control works

| Direction | Mechanism |
|-----------|-----------|
| Upload | HTB + netem on interface egress |
| Download | Ingress → `ifb0`, then HTB + netem |
| Loss | `netem loss` on shaped paths |
| Reset | Remove root/ingress qdiscs; down `ifb0` |

## Project layout

```
netlimit/
├── Cargo.toml
├── README.md
└── src/
    ├── main.rs       # CLI flags + terminal lifecycle
    ├── app.rs        # State, keys, mouse, apply/reset
    ├── ui.rs         # Layout & widgets
    ├── theme.rs      # Colors / styles
    ├── tc.rs         # tc / netem / IFB
    ├── netinfo.rs    # Interface discovery
    ├── presets.rs    # Built-in + saved presets
    └── elevate.rs    # sudo re-exec
```

## Safety

- Affects **all** traffic on the selected interface
- Always **Reset** when finished
- Requires root for rule changes

## License

MIT
