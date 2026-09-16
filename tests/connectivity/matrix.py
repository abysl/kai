import json
import os
import sys


def pairs(path):
    with open(path) as file:
        return json.load(file)["pairs"]


def main():
    path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "matrix.json")
    verb = sys.argv[1]
    listed = pairs(path)
    if verb == "ids":
        print(" ".join(pair["id"] for pair in listed))
        return
    if verb == "default":
        print(" ".join(pair["id"] for pair in listed if pair["ci"] != "extra"))
        return
    if verb == "ci":
        print(" ".join(pair["id"] for pair in listed if pair["ci"] == sys.argv[2]))
        return
    if verb == "show":
        pair = next((pair for pair in listed if pair["id"] == sys.argv[2]), None)
        if pair is None:
            raise SystemExit(f"no pair {sys.argv[2]} in matrix.json")
        print(pair["host"], pair["joiner"], ",".join(pair["needs"]), "offline" if pair.get("offline") else "online", pair.get("spec", "drivers"))
        return
    raise SystemExit(f"unknown verb {verb}")


if __name__ == "__main__":
    main()
