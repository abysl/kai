import json
import sys


def load(path):
    records = []
    try:
        with open(path) as file:
            for line in file:
                line = line.strip()
                if not line:
                    continue
                try:
                    record = json.loads(line)
                except ValueError:
                    continue
                if isinstance(record, dict) and "event" in record:
                    records.append(record)
    except FileNotFoundError:
        pass
    return records


def first(records, name):
    return next((record for record in records if record["event"] == name), None)


def last(records, name):
    return next((record for record in reversed(records) if record["event"] == name), None)


def main():
    path, verb = sys.argv[1], sys.argv[2]
    records = load(path)
    if verb == "has":
        sys.exit(0 if first(records, sys.argv[3]) else 1)
    if verb == "first":
        record = first(records, sys.argv[3])
        if record is None:
            sys.exit(1)
        sys.stdout.write(json.dumps(record))
        return
    if verb == "field":
        record = first(records, sys.argv[3])
        if record is None or sys.argv[4] not in record:
            sys.exit(1)
        sys.stdout.write(str(record[sys.argv[4]]))
        return
    if verb == "count":
        sys.stdout.write(str(len(records)))
        return
    if verb == "tally":
        tally = {}
        for record in records:
            tally[record["event"]] = tally.get(record["event"], 0) + 1
        sys.stdout.write(" ".join(f"{name}={n}" for name, n in tally.items()))
        return
    raise SystemExit(f"unknown verb {verb}")


if __name__ == "__main__":
    main()
