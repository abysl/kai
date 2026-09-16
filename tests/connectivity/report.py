import json
import os
import sys
from xml.sax.saxutils import escape

TAIL_EVENTS = 20


def load_results(out):
    results = []
    for name in sorted(os.listdir(out)):
        path = os.path.join(out, name, "result.json")
        if os.path.isfile(path):
            with open(path) as file:
                results.append(json.load(file))
    return results


def event_tail(side):
    path = os.path.join(side.get("dir", ""), "events.jsonl")
    try:
        with open(path) as file:
            lines = [line.rstrip("\n") for line in file if line.strip()]
    except OSError:
        return []
    return lines[-TAIL_EVENTS:]


def failure_body(result):
    parts = [result.get("reason", "")]
    for label in ("host", "joiner"):
        side = result.get(label) or {}
        parts.append(f"--- {label} ({result.get(label + '_platform')}) exit {side.get('exit')} ---")
        parts.extend(event_tail(side))
    if result.get("ticket"):
        parts.append(f"ticket: {result['ticket']}")
    parts.append(f"build: {json.dumps(result.get('build'))}")
    parts.append(f"preflight: {json.dumps(result.get('preflight'))}")
    return "\n".join(parts)


def junit(results, out_path):
    failures = sum(1 for r in results if r["status"] == "fail")
    skipped = sum(1 for r in results if r["status"] == "skipped")
    seconds = sum(float(r.get("seconds") or 0) for r in results)
    lines = [
        '<?xml version="1.0" encoding="UTF-8"?>',
        f'<testsuite name="kai-connectivity" tests="{len(results)}" failures="{failures}" skipped="{skipped}" time="{seconds:.1f}">',
    ]
    for result in results:
        lines.append(
            f'  <testcase classname="connectivity" name="{escape(result["id"])}" time="{float(result.get("seconds") or 0):.1f}">'
        )
        if result["status"] == "fail":
            lines.append(f'    <failure message="{escape(result.get("reason", ""), {chr(34): "&quot;"})}">')
            lines.append(escape(failure_body(result)))
            lines.append("    </failure>")
        elif result["status"] == "skipped":
            lines.append(f'    <skipped message="{escape(result.get("reason", ""), {chr(34): "&quot;"})}"/>')
        lines.append("  </testcase>")
    lines.append("</testsuite>")
    with open(out_path, "w") as file:
        file.write("\n".join(lines) + "\n")


def public(result):
    return {
        "id": result["id"],
        "status": result["status"],
        "reason": result.get("reason", ""),
        "notes": result.get("notes", []),
        "seconds": result.get("seconds"),
        "seeds": result.get("seeds"),
        "ticket": result.get("ticket"),
        "host": {
            "platform": result.get("host_platform"),
            "outcome": (result.get("host") or {}).get("outcome"),
            "exit": (result.get("host") or {}).get("exit"),
            "events": (result.get("host") or {}).get("tally"),
            "dir": (result.get("host") or {}).get("dir"),
        },
        "joiner": {
            "platform": result.get("joiner_platform"),
            "outcome": (result.get("joiner") or {}).get("outcome"),
            "exit": (result.get("joiner") or {}).get("exit"),
            "events": (result.get("joiner") or {}).get("tally"),
            "dir": (result.get("joiner") or {}).get("dir"),
        },
        "build": result.get("build"),
        "preflight": result.get("preflight"),
    }


def main():
    out = sys.argv[1]
    report_path = sys.argv[2] if len(sys.argv) > 2 else os.path.join(out, "report.json")
    results = load_results(out)
    report = {
        "out": out,
        "pairs": [public(r) for r in results],
        "passed": sum(1 for r in results if r["status"] == "pass"),
        "failed": sum(1 for r in results if r["status"] == "fail"),
        "skipped": sum(1 for r in results if r["status"] == "skipped"),
    }
    with open(report_path, "w") as file:
        json.dump(report, file, indent=1)
    junit(results, os.path.join(out, "junit.xml"))
    print(f"{report['passed']} passed, {report['failed']} failed, {report['skipped']} skipped — {report_path}")
    sys.exit(1 if report["failed"] else 0)


if __name__ == "__main__":
    main()
