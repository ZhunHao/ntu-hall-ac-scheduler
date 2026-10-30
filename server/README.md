# Server-side configuration

Files for `nextcloud-server`, the home server described in `docs/architecture.md`. Nothing here
runs on the ESP32.

| Path | Installs to | Purpose |
|---|---|---|
| `chrony/ac-scheduler.conf` | `/etc/chrony/conf.d/ac-scheduler.conf` | Home NTP server for the ESP32 (Phase 1) |

## chrony (Phase 1)

```bash
sudo apt install chrony   # if not installed
sudo cp server/chrony/ac-scheduler.conf /etc/chrony/conf.d/
sudo systemctl restart chrony
chronyc sources           # upstream servers
chronyc clients           # the ESP32 appears here after its first SNTP request
```

Debian and Ubuntu's stock `chrony.conf` already includes `rtcsync` and reads `conf.d/`. The
EdgeRouter must allow UDP 123 from the trusted LAN (`192.168.0.0/24`) to `192.168.2.44`.

Check from a Mac on the trusted Wi-Fi: `sntp 192.168.2.44` should print an offset.
