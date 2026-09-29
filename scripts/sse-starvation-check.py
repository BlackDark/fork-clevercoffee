#!/usr/bin/env python3
"""SSE starvation regression check.

The bug: ESP-IDF's httpd runs every handler in ONE task, and the /events
handler used to loop in the handler. One SSE client therefore made every other
API request time out. The C++ does not do this -- AsyncEventSource returns
from the handler and pushes later from the loop task.

This script: connects one SSE client, then hammers /api/parameters and
reports how many answered. Run it BEFORE and AFTER a fix and compare.
"""
import socket
import sys
import threading
import time
import urllib.error
import urllib.request

HOST = sys.argv[1] if len(sys.argv) > 1 else "10.0.1.168"
REQUESTS = int(sys.argv[2]) if len(sys.argv) > 2 else 150
TIMEOUT = 4.0

sse_ok = {"open": False, "frames": 0, "err": None}


def sse_client():
    """Hold one /events connection open for the duration of the test."""
    try:
        s = socket.create_connection((HOST, 80), timeout=TIMEOUT)
        s.sendall(
            b"GET /events HTTP/1.1\r\nHost: cc\r\nAccept: text/event-stream\r\n\r\n"
        )
        buf = b""
        deadline = time.time() + 40
        s.settimeout(1.0)
        while time.time() < deadline:
            try:
                chunk = s.recv(1024)
            except socket.timeout:
                continue
            if not chunk:
                break
            buf += chunk
            sse_ok["frames"] += buf.count(b"data:")
        s.close()
    except Exception as e:  # noqa: BLE001 - diagnostic script
        sse_ok["err"] = repr(e)


t = threading.Thread(target=sse_client, daemon=True)
t.start()
time.sleep(2.5)  # let the stream establish

ok = fail = 0
codes = {}
times = []
url = f"http://{HOST}/api/parameters?filter=all"
for _ in range(REQUESTS):
    t0 = time.time()
    try:
        with urllib.request.urlopen(url, timeout=TIMEOUT) as r:
            r.read()
            codes[r.status] = codes.get(r.status, 0) + 1
        times.append(time.time() - t0)
        ok += 1
    except Exception:  # noqa: BLE001 - any failure is a failure
        codes["timeout/error"] = codes.get("timeout/error", 0) + 1
        fail += 1

print(f"SSE frames seen : {sse_ok['frames']}  err={sse_ok['err']}")
print(f"requests        : {ok} ok / {fail} failed of {REQUESTS}")
print(f"status codes    : {codes}")
if times:
    times.sort()
    print(f"latency min/p50 : {times[0]*1000:.0f} / {times[len(times)//2]*1000:.0f} ms")
print()
if sse_ok["frames"] == 0:
    print("INCONCLUSIVE: the SSE client received no frames, so nothing was proven")
    sys.exit(2)
if fail:
    print(f"REGRESSION: {fail}/{REQUESTS} requests failed while one SSE client was open")
    sys.exit(1)
print("PASS: one SSE client does not starve the API")
