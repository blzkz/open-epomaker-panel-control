> ⚠️ Experimental: the protocol is verified against the official software byte for byte,
> but it has not been tested on a physical keyboard yet. Start with `epomaker probe`
> (read-only) and make a backup (`epomaker backup before.json`) before changing anything.
> Not affiliated with EPOMAKER.

## Downloads

| System | File | Install |
|---|---|---|
| **Windows 10/11** | `open-epomaker-panel-windows-x86_64.zip` | Unzip and run `epomaker-panel.exe` (close the official EPOMAKER software first) |
| **Debian, Ubuntu, Mint, Pop!_OS** | `open-epomaker-panel_{{VERSION}}-1_amd64.deb` | `sudo apt install ./open-epomaker-panel_{{VERSION}}-1_amd64.deb` |
| **Fedora, RHEL, openSUSE** | `open-epomaker-panel-{{VERSION}}-1.x86_64.rpm` | `sudo dnf install ./open-epomaker-panel-{{VERSION}}-1.x86_64.rpm` · `sudo zypper install ./open-epomaker-panel-{{VERSION}}-1.x86_64.rpm` |
| **Arch, Manjaro, EndeavourOS, CachyOS** | `open-epomaker-panel-{{VERSION}}-1-x86_64.pkg.tar.zst` | `sudo pacman -U open-epomaker-panel-{{VERSION}}-1-x86_64.pkg.tar.zst` |
| **Any Linux** (portable) | `open-epomaker-panel-x86_64.AppImage` | `chmod +x` and run. Also install the udev rule (see below) |
| **Any Linux** (tarball) | `open-epomaker-panel-linux-x86_64.tar.gz` | Extract and run `linux/install.sh` |

The deb, rpm and Arch packages install the udev rule automatically. With the AppImage, install it once:

```bash
curl -fsSL https://raw.githubusercontent.com/blzkz/open-epomaker-panel-control/v{{VERSION}}/packaging/linux/70-epomaker.rules | sudo tee /etc/udev/rules.d/70-epomaker.rules
sudo udevadm control --reload-rules && sudo udevadm trigger
```

Each package ships the GUI (`epomaker-panel`) and the command line tool (`epomaker`; with the AppImage, `./open-epomaker-panel-x86_64.AppImage cli …`).
Verify your downloads with `SHA256SUMS.txt`.
