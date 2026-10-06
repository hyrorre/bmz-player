#!/usr/bin/env python3
"""Read-only host-side companion to BMZ_LATENCY_JSON (no service/config changes).

Run while BMZ is playing. PID is the host PID, including for Flatpak. Server
observations are evidence from separate connections, not BMZ stream API values.
pw-top remains necessary to observe the actual driver quantum over time.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import time


def query(args, as_json=False):
    try:
        result = subprocess.run(args, capture_output=True, text=True, timeout=3, check=True)
        return {"value": json.loads(result.stdout) if as_json else result.stdout.strip()}
    except (OSError, subprocess.SubprocessError, ValueError) as error:
        return {"value": None, "unavailable": str(error)}


def select_nodes(objects, pid):
    result = []
    client_ids = {obj.get("id") for obj in objects
                  if str(obj.get("info", {}).get("props", {}).get("pipewire.sec.pid")) == str(pid)}
    for obj in objects:
        props = obj.get("info", {}).get("props", {})
        if (obj.get("id") not in client_ids and props.get("client.id") not in client_ids
                and str(props.get("application.process.id")) != str(pid)):
            continue
        keys = ("application.process.id", "application.name", "media.class", "media.category",
                "node.name", "node.latency", "node.rate", "audio.rate", "target.object",
                "client.id", "pipewire.access", "pipewire.access.effective", "pipewire.sec.flatpak",
                "pipewire.sec.pid", "pipewire.sec.socket", "pipewire.access.portal.app_id",
                "core.version", "object.serial")
        result.append({"id": obj.get("id"), "type": obj.get("type"),
                       "props": {k: props[k] for k in keys if k in props}})
    return result


def cpu_ticks(pid):
    # comm can contain spaces and parentheses. Fields after ')' start at #3.
    fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
    return int(fields[11]) + int(fields[12]), int(fields[19])


def select_pulse_inputs(inputs, pid):
    # CPAL 0.18.1's pure-Rust Pulse client supplies an application name, but
    # does not always provide application.process.id. Never match by a prefix.
    return [item for item in inputs if (
        str(item.get("properties", {}).get("application.process.id")) == str(pid)
        or item.get("properties", {}).get("application.name") == f"cpal-pulseaudio-{pid}")]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("pid", type=int, help="host PID of BMZ")
    parser.add_argument("--seconds", type=float, default=10, help="CPU sampling window")
    args = parser.parse_args()
    if args.seconds <= 0:
        parser.error("--seconds must be positive")
    summary = {"schema": 1, "kind": "linux_server", "pid": args.pid,
               "observer": "host, separate server connections",
               "driver_quantum_frames": None, "confirmed_xruns": None,
               "observer_tool_versions": {name: query([name, "--version"])
                                   for name in ("pw-cli", "pactl", "flatpak")},
               "pulse_server": query(["pactl", "--format=json", "info"], True)}
    pw = query(["pw-dump"], True)
    summary["pipewire_server_core"] = [{"id": o.get("id"), "info": o.get("info")}
        for o in pw["value"] if o.get("type") == "PipeWire:Interface:Core"] if isinstance(pw["value"], list) else pw
    summary["pipewire_nodes"] = select_nodes(pw["value"], args.pid) if isinstance(pw["value"], list) else pw
    pulse = query(["pactl", "--format=json", "list", "sink-inputs"], True)
    summary["pulse_sink_inputs"] = select_pulse_inputs(pulse["value"], args.pid) if isinstance(pulse["value"], list) else pulse
    try:
        before, identity = cpu_ticks(args.pid)
        start = time.monotonic()
        time.sleep(args.seconds)
        after, current_identity = cpu_ticks(args.pid)
        elapsed = time.monotonic() - start
        if identity != current_identity or after < before:
            raise ValueError("PID reused or CPU counter reset")
        cpu = (after - before) / os.sysconf("SC_CLK_TCK")
        summary["cpu"] = {"wall_seconds": elapsed, "process_seconds": cpu,
                          "one_core_percent": cpu / elapsed * 100}
    except (OSError, ValueError, IndexError) as error:
        summary["cpu"] = {"value": None, "unavailable": str(error)}
    print("BMZ_LATENCY_JSON " + json.dumps(summary, ensure_ascii=False))


if __name__ == "__main__":
    main()
