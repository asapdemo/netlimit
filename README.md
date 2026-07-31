# NetLimit

Interactive **btop-style** TUI for system-wide Linux network traffic control.

Built with **Rust** + **ratatui**. Limits download, upload, and packet loss via `tc` / `netem` / IFB.

## Features

- Dense dark dashboard (btop-inspired)
- Download / upload Mbps + packet loss %
- Full interface list (click to select)
- Keyboard + mouse (`−`/`+`, sliders, buttons)
- Presets (built-in + custom, save/delete)
- Apply / Reset with status feedback
- Single binary: `netlimit`

## Requirements

- Linux + `iproute2` (`tc`, `ip`)
- Root for Apply / Reset
- Rust 1.74+ (to build)

## Build & run

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
| `i` / `[` `]` | Cycle interface |
| `q` / `Esc` | Quit |

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
