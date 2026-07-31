# NetLimit

**Interactive TUI for system-wide network traffic control on Linux** — inspired by [btop](https://github.com/aristocratos/btop).

Limit download speed, upload speed, and packet loss in real time using a modern dark terminal interface. Built on Linux `tc` + `netem` + IFB.

![NetLimit TUI](https://img.shields.io/badge/TUI-Textual-blue) ![Linux](https://img.shields.io/badge/OS-Linux-orange) ![Python](https://img.shields.io/badge/Python-3.11%2B-green)

---

## Features

- Full-screen, btop-style dark UI (Textual)
- Live display of download / upload / packet loss
- Keyboard **and** mouse controls
- Auto-detect default network interface (or pick another)
- **Apply** / **Reset** with clear status feedback
- **Upload** shaping via HTB + netem on interface egress
- **Download** shaping via IFB ingress redirect + HTB + netem
- Packet loss via netem
- Clean teardown of qdiscs and IFB on reset

---

## Requirements

- Linux (`tc`, `ip`, `modprobe ifb`)
- Python 3.11+
- Root privileges for applying/resetting limits
- `iproute2` (`tc`, `ip`)

```bash
# Debian / Ubuntu
sudo apt install iproute2

# Fedora
sudo dnf install iproute-tc
```

---

## Install

```bash
cd netlimit
uv sync
```

### Running with sudo

`sudo` uses a restricted `PATH` and **cannot see** project venvs.

**Always works (absolute path):**

```bash
sudo .venv/bin/netlimit
# or
sudo /path/to/netlimit/.venv/bin/netlimit
```

**Install a system link once** (then plain `sudo netlimit` works):

```bash
sudo ln -sf "$(pwd)/.venv/bin/netlimit" /usr/local/bin/netlimit
sudo netlimit
```

**Auto re-exec** (app escalates with the absolute path):

```bash
uv run netlimit
# → sudo /full/path/to/.venv/bin/netlimit
```

If sudo prints `Sorry, try again`, that is a **password / sudo auth** problem before NetLimit starts. Check with `sudo whoami`.

---

## Usage

```bash
sudo .venv/bin/netlimit
sudo .venv/bin/netlimit -i wlan0
uv run netlimit --no-sudo          # UI only, no elevation (Apply needs root)
```

### Keybindings

| Key | Action |
|-----|--------|
| `↑` / `↓` / `Tab` | Select metric |
| `←` / `→` or `+` / `−` | Adjust value |
| `Shift` + `±` / arrows | Coarse step (×10) |
| `d` / `u` / `l` | Focus download / upload / loss |
| `a` | Apply settings |
| `r` | Reset (remove all limits) |
| `i` | Cycle network interface |
| `q` / `Esc` | Quit |

Mouse: click **+** / **−** on cards, interface arrows, and action buttons.

**Values:** `0` Mbps means **unlimited** for download/upload.

---

## Project structure

```
netlimit/
├── pyproject.toml
├── README.md
└── netlimit/
    ├── __init__.py
    ├── __main__.py
    ├── main.py              # Entry point (argparse + launch TUI)
    ├── elevate.py           # sudo re-exec helpers
    ├── core/
    │   ├── __init__.py
    │   ├── tc.py            # tc / netem / IFB logic
    │   └── utils.py         # iface detection, formatting, root checks
    └── ui/
        ├── __init__.py
        ├── app.py           # Textual App
        ├── widgets.py       # Metric cards, banners, interface bar
        └── styles.tcss      # Theme & layout CSS
```

---

## How it works

| Direction | Mechanism |
|-----------|-----------|
| **Upload** | `tc` HTB class on interface egress + optional `netem` loss |
| **Download** | Ingress redirected to `ifb0`, then HTB + netem on IFB egress |
| **Loss** | `netem loss X%` on shaped paths |
| **Reset** | Delete root/ingress qdiscs; bring `ifb0` down |

---

## Safety notes

- Traffic control affects **all** traffic on the selected interface.
- Always use **Reset** when finished.
- Requires root; the app re-execs with `sudo` (absolute path) if launched as a normal user.
- IFB (`ifb0`) is used for download shaping; it is brought down on reset.

---

## Troubleshooting

| Problem | Fix |
|---------|-----|
| `sudo: netlimit: command not found` | Use `sudo .venv/bin/netlimit` or install the `/usr/local/bin` link |
| `Sorry, try again` | Sudo password rejected — run `sudo whoami` |
| `Missing required command(s): tc` | Install `iproute2` |
| Download limit has no effect | `sudo modprobe ifb` |
| No interface detected | Pass `-i eth0` / `-i wlan0` |
| Rules persist after crash | `sudo .venv/bin/netlimit` → press `r` (Reset) |

Check kernel state:

```bash
tc qdisc show
tc class show dev eth0
tc qdisc show dev ifb0
```

---

## License

MIT
