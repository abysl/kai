import json
import sys
import time


def stamped(line):
    record = json.loads(line)
    if not isinstance(record, dict):
        raise ValueError("not an object")
    record["at"] = int(time.time() * 1000)
    return json.dumps(record, separators=(",", ":"))


def main():
    bad = None
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            sys.stdout.write(stamped(line) + "\n")
            sys.stdout.flush()
        except ValueError as error:
            if len(sys.argv) > 1:
                bad = bad or open(sys.argv[1], "a")
                bad.write(f"{error}: {line}\n")
                bad.flush()


if __name__ == "__main__":
    main()
