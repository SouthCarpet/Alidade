# Engine acceptance evidence

Date: 2026-08-20. This is a record of the supplied soak logs and the local
SQLite database. It is not a build or test result.

## What was run

`soak-run.json` records a start at `2026-08-20T17:51:06.3822422+02:00`, PID
8936, and this exact command text:

```text
continuous --throughput-every 5m --ping-every 1m
```

The metadata does not record the launcher prefix or an end time. The stdout
contains 13 full rounds and 49 ping-only rounds (62 stored-round messages).
The matching database rows run from 17:51:06 to 18:52:06 CEST: 61 minutes
between first and last scheduled-row timestamps. The acceptance brief calls
the process a 62-minute soak; its exact stop time cannot be verified from the
provided metadata or timestamp-free stdout.

The cadence was deliberately compressed for the soak. The shipped defaults
are hourly full throughput rounds and five-minute ping-only rounds. At the
shipped hourly throughput cadence, an hour would produce only one full round;
the 5-minute / 1-minute cadence exercised repeated scheduling and storage.

## Results from stdout

The 49 ping-only rounds range from 17.5 ms to 67.0 ms. Their arithmetic mean,
calculated from the displayed values, is 24.047 ms. `soak.err.log` is empty.

The table preserves the CLI's displayed precision. `NULL` means the database
column is `NULL`, not a measured zero.

| CEST start | download | upload | idle ping | loaded download ping | loaded upload ping |
|---|---:|---:|---:|---:|---:|
| 17:51:06 | 104.10 Mbit/s | 11.09 Mbit/s | 21.5 ms | 27.3 ms | 25.6 ms |
| 17:56:06 | 207.37 Mbit/s | 11.97 Mbit/s | 21.8 ms | NULL — 2 samples in 1.2 s | 22.8 ms |
| 18:01:06 | NULL — HTTP 429 | 18.56 Mbit/s | 21.5 ms | NULL — 1 sample in 0.2 s | 20.9 ms |
| 18:06:06 | NULL — HTTP 429 | 17.91 Mbit/s | 30.0 ms | NULL — 1 sample in 0.3 s | 24.6 ms |
| 18:11:06 | NULL — HTTP 429 | 0.33 Mbit/s | 19.0 ms | 156.0 ms | 51.0 ms |
| 18:16:06 | NULL — HTTP 429 | 12.63 Mbit/s | 23.0 ms | NULL — 1 sample in 0.1 s | 24.0 ms |
| 18:21:06 | NULL — HTTP 429 | 15.69 Mbit/s | 24.5 ms | NULL — 1 sample in 0.1 s | 24.0 ms |
| 18:26:06 | NULL — HTTP 429 | 14.03 Mbit/s | 43.0 ms | NULL — 1 sample in 0.3 s | 23.3 ms |
| 18:31:06 | NULL — HTTP 429 | 20.02 Mbit/s | 22.5 ms | NULL — 1 sample in 0.1 s | 20.1 ms |
| 18:36:06 | NULL — HTTP 429 | 15.05 Mbit/s | 31.0 ms | NULL — 1 sample in 0.2 s | 19.4 ms |
| 18:41:06 | NULL — HTTP 429 | 13.33 Mbit/s | 21.2 ms | NULL — 1 sample in 0.2 s | 19.9 ms |
| 18:46:06 | NULL — HTTP 429 | 9.88 Mbit/s | 26.0 ms | NULL — 1 sample in 0.3 s | 32.4 ms |
| 18:51:06 | NULL — HTTP 429 | 17.49 Mbit/s | 20.2 ms | NULL — 1 sample in 0.2 s | 24.2 ms |

## Rate-limit finding

This is the main outcome of the soak. The first two full rounds downloaded
successfully at 104.10 and 207.37 Mbit/s. The next 11 full rounds returned
`provider: server returned status 429` for download. Upload continued on each
of those rounds.

The database confirms that all 11 logged HTTP-429 rows have `down_bps = NULL`,
not zero; all have `capped = 0`. Ten of those rows also have `ping_down_ms =
NULL` because there were too few loaded-download samples. The one exception
has three samples and stores 156.0 ms.

The database has a further HTTP-429 full row at 18:53:55 CEST. It is not in
the supplied stdout, so it is a log/database discrepancy. It shows the limit
was still active after the final logged ping-only row at 18:52:06. A later
successful full-mode row at 20:52:57 CEST recorded 142.19829 Mbit/s download
(142.20 Mbit/s at the CLI's displayed precision). This is 1 h 59 m 2 s after
the extra 429 row, which supports that the limit had cleared by then. The raw
stdout does not contain this later measurement, and `rounds.mode` is only
`full`, so the evidence cannot independently establish that its command was
`single` or the exact time the limit cleared.

## Database cross-check

`%LOCALAPPDATA%\Alidade\alidade.db` reports `PRAGMA user_version = 3`.

The 62 rows represented in stdout correspond to database rows 8 through 69:
13 `full` and 49 `ping`. Their start timestamps are exactly one minute apart;
the full rows fall every five minutes and replace the same-minute ping row.
The displayed stdout values agree with the database at the CLI's displayed
precision. There are no lock errors in either supplied log.

The database does **not** show growth in `ping_samples`: it contains zero rows
in total, and `ping_minute` also contains zero rows. Per-round ping summaries
are present in `rounds`, but the requested raw-sample storage cross-check
fails. This needs investigation before claiming the D7 raw ping-sample path
was exercised.

The one extra database row at 18:53:55 CEST is the other discrepancy. It is a
full HTTP-429 row with 13.55 Mbit/s upload and has no matching stdout line.

**Resolved after this report was written:** that row is a manual `single` run
made an hour after the soak stopped, to test whether the rate limit had
cleared. It had not — hence the 429. It is not a store fault, and the writer of
this report could not have known its origin from the evidence available. The
row is genuine data and stays. The database holds 71 rounds in total: 7 from
before the soak, 62 from the soak itself, and two later manual checks (the
18:53 one above, and one at 20:52 which finally measured 142.20 Mbit/s and
confirmed the limit had lifted).

## Live probe targets

| target | kind | host | answered | RTT |
|---|---|---|---|---:|
| Cloudflare DNS | ICMP | 1.1.1.1 | yes | 30.0 ms |
| Google DNS | ICMP | 8.8.8.8 | yes | 24.0 ms |
| LoL EUNE | TCP | 104.160.142.3:443 | no | — |
| Genshin EU | TCP | hk4e-api-os.hoyoverse.com:443 | yes | 70.7 ms |

The LoL EUNE preset did not answer. The follow-up reachable candidates are
Riot API edges, not League game servers; a result from one measures the path
to Riot's front door, not in-game latency. Any replacement must remain
editable and be labelled accordingly.

## Not covered or not verified

- No UI beyond a shell, tray behaviour, notifications, or retention-job
  scheduling was exercised. These are part of the later UI work.
- The optional daily budget was not exercised across process restarts. Its
  byte counter is in process memory, so restart behaviour remains unproven.
- A measurement round pings only its first configured target. Multi-target
  ping within one round was not exercised.
- The supplied evidence has no recorded database size before/after or process
  memory at start/end.
- Builds and tests were intentionally not run during this review because other
  workers were editing the checkout. Their result and any test count are
  unverified here.
- The raw `ping_samples` persistence path was not exercised successfully: the
  table is empty despite the recorded round-level ping measurements.
- The rate-limit recovery point is bounded only by the 18:53:55 failure and
  20:52:57 success; it is not known more precisely.
