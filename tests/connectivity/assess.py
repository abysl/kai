import json
import os
import sys

from events import first, last, load

MAX_REFUSALS = 8
TAIL_LINES = 200
LOG_NAMES = ("stderr.log", "console.log", "logcat.log", "driver.log")


def read_text(path):
    try:
        with open(path) as file:
            return file.read().strip()
    except OSError:
        return None


def tail(path, lines):
    text = read_text(path)
    if text is None:
        return None
    return text.splitlines()[-lines:]


def side_summary(side_dir):
    records = load(os.path.join(side_dir, "events.jsonl"))
    tally = {}
    for record in records:
        tally[record["event"]] = tally.get(record["event"], 0) + 1
    exit_text = read_text(os.path.join(side_dir, "exit"))
    shots = sorted(
        os.path.join(side_dir, name)
        for name in os.listdir(side_dir)
        if name.endswith(".png")
    ) if os.path.isdir(side_dir) else []
    return {
        "dir": side_dir,
        "events": len(records),
        "tally": tally,
        "outcome": first(records, "outcome"),
        "seated": first(records, "seated"),
        "started": first(records, "started"),
        "hosting": first(records, "hosting"),
        "roster": last(records, "roster"),
        "rejoins": [r["text"] for r in records if r["event"] == "warn" and "rejoin" in r.get("text", "")],
        "exit": int(exit_text) if exit_text and exit_text.lstrip("-").isdigit() else exit_text,
        "screenshots": shots,
        "records": records,
    }


def expected_result(until):
    if until == "seated":
        return {"seated"}
    if until == "winner":
        return {"winner"}
    return {"turns", "winner"}


def wanted_turns(until):
    if isinstance(until, str) and until.startswith("turns:"):
        return int(until.split(":", 1)[1])
    return 1


def played(label, outcome, players, until):
    failures = []
    if outcome["result"] == "winner" and outcome.get("winner") not in range(players):
        failures.append(f"{label} reports winner {outcome.get('winner')}, not a seat")
    if outcome.get("turns", 0) < wanted_turns(until):
        failures.append(f"{label} reached turn {outcome.get('turns', 0)}, wanted {wanted_turns(until)}")
    if outcome.get("sent", 0) < 1:
        failures.append(f"{label} sent no intent")
    return failures


def check(meta, host, joiner):
    failures = []
    notes = []
    for label, side in (("host", host), ("joiner", joiner)):
        outcome = side["outcome"]
        if outcome is None:
            failures.append(f"{label} produced no outcome")
        elif outcome.get("result") == "failed":
            failures.append(f"{label} failed: {outcome.get('reason', '?')}")
    if failures:
        return failures, notes
    ho, jo = host["outcome"], joiner["outcome"]
    wanted = expected_result(meta["until"])
    if ho["result"] != jo["result"]:
        failures.append(f"results differ: host {ho['result']} vs joiner {jo['result']}")
    elif ho["result"] not in wanted:
        failures.append(f"result {ho['result']} is not the plan's until {meta['until']}")
    if ho["result"] in ("winner", "turns"):
        for label, outcome in (("host", ho), ("joiner", jo)):
            failures.extend(played(label, outcome, meta["players"], meta["until"]))
        if ho.get("winner") != jo.get("winner"):
            failures.append(f"winners differ: host {ho.get('winner')} vs joiner {jo.get('winner')}")
        if ho.get("turns") != jo.get("turns"):
            failures.append(f"turn counts differ: host {ho.get('turns')} vs joiner {jo.get('turns')}")
        for label, side in (("host", host), ("joiner", joiner)):
            started = side["started"]
            if started is None:
                failures.append(f"{label} never emitted started")
            elif started.get("enforced") != meta["enforced"]:
                failures.append(f"{label} started enforced={started.get('enforced')}, plan says {meta['enforced']}")
    for label, side, seat in (("host", host, 0), ("joiner", joiner, 1)):
        seated = side["seated"]
        if seated is None:
            failures.append(f"{label} never emitted seated")
        elif seated.get("seat") != seat:
            failures.append(f"{label} seated at seat {seated.get('seat')}, expected {seat}")
        roster = side["roster"]
        connected = sum(1 for s in (roster or {}).get("seats", []) if s.get("connected"))
        if roster is None or connected < meta["players"]:
            failures.append(f"{label}'s final roster lists {connected} connected seats, expected {meta['players']}")
        if side["rejoins"]:
            if meta["allow_rejoin"]:
                notes.append(f"{label} rejoined: {'; '.join(side['rejoins'])}")
            else:
                failures.append(f"{label} rejoined mid-game: {'; '.join(side['rejoins'])}")
        refused = side["outcome"].get("refused", 0)
        if refused > MAX_REFUSALS:
            failures.append(f"{label} had {refused} refusals, above {MAX_REFUSALS}")
        if side["exit"] not in (0, None):
            notes.append(f"{label} driver exited {side['exit']}")
    return failures, notes


def detail_line(meta, host, joiner):
    ho = host["outcome"] or {}
    jo = joiner["outcome"] or {}
    if ho.get("result") in ("winner", "turns"):
        head = f"{ho['result']} seat {ho.get('winner')} in {ho.get('turns')} turns"
    else:
        head = ho.get("result") or "no outcome"
    return (
        f"{head}, {meta['seconds']}s "
        f"(sent {ho.get('sent', '-')}/{jo.get('sent', '-')}, refused {ho.get('refused', '-')}/{jo.get('refused', '-')})"
    )


def strip_records(side):
    public = dict(side)
    del public["records"]
    return public


def main():
    pair_dir, meta_path = sys.argv[1], sys.argv[2]
    with open(meta_path) as file:
        meta = json.load(file)
    result = {
        "id": meta["id"],
        "host_platform": meta["host_platform"],
        "joiner_platform": meta["joiner_platform"],
        "seeds": meta.get("seeds"),
        "until": meta.get("until"),
        "plans": meta.get("plans"),
        "ticket": meta.get("ticket"),
        "preflight": meta.get("preflight"),
        "build": meta.get("build"),
        "started_at": meta.get("started_at"),
        "ended_at": meta.get("ended_at"),
        "seconds": meta.get("seconds"),
        "notes": [],
    }
    if meta.get("skipped"):
        result["status"] = "skipped"
        result["reason"] = meta["skipped"]
        line = f"{meta['id']:<18} skipped  {meta['skipped']}"
    else:
        host = side_summary(os.path.join(pair_dir, "host"))
        joiner = side_summary(os.path.join(pair_dir, "joiner"))
        failures, notes = check(meta, host, joiner)
        if meta.get("error"):
            failures.insert(0, meta["error"])
        result["host"] = strip_records(host)
        result["joiner"] = strip_records(joiner)
        result["notes"] = notes
        if failures:
            result["status"] = "fail"
            result["reason"] = "; ".join(failures)
            result["logs"] = {
                label: {name: tail(os.path.join(side["dir"], name), TAIL_LINES) for name in LOG_NAMES}
                for label, side in (("host", host), ("joiner", joiner))
            }
            line = f"{meta['id']:<18} FAIL     {result['reason']} — {detail_line(meta, host, joiner)}"
        else:
            result["status"] = "pass"
            result["reason"] = ""
            line = f"{meta['id']:<18} pass     {detail_line(meta, host, joiner)}"
            if notes:
                line += " [" + "; ".join(notes) + "]"
    with open(os.path.join(pair_dir, "result.json"), "w") as file:
        json.dump(result, file, indent=1)
    print(line)
    sys.exit(0 if result["status"] != "fail" else 1)


if __name__ == "__main__":
    main()
