# Known issues

- The default **LoL EUNE** target (`104.160.142.3:443`) does not answer. A live probe on 2026-08-20 received no answer, while Cloudflare DNS, Google DNS, and the Genshin EU host answered. A reachable replacement today would be a Riot API edge, not a game server, so it would measure the path to Riot's front door rather than in-game latency. The preset is marked `verified = false` and can be edited or disabled in the settings file.
- The **Genshin EU** preset is also marked `verified = false`. It is user-editable; use `alidade.exe probe-targets` to check it from your own connection.
- A measurement round pings only the first enabled target. Use `ping-monitor` when you need dense monitoring of another configured target.
- The desktop app currently provides a five-screen tab shell only. The screen bodies, system-tray controls, and measurement notifications are not present in the current source.
- The release binaries are unsigned, so Windows SmartScreen may warn on first run.
