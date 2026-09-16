import argparse
import json
import sys


def until_value(text):
    if text in ("winner", "seated"):
        return text
    if text.startswith("turns:"):
        return {"turns": int(text.split(":", 1)[1])}
    raise SystemExit(f"until must be winner, seated or turns:N, not {text!r}")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", action="store_true")
    parser.add_argument("--join", default=None)
    parser.add_argument("--seed", type=int, required=True)
    parser.add_argument("--name", required=True)
    parser.add_argument("--until", default="winner")
    parser.add_argument("--timeout-s", type=int, default=600)
    parser.add_argument(
        "--deck",
        default=None,
        help="a pool deck prefix, or saved:<label> for a held history row on that device",
    )
    parser.add_argument("--enforced", default="true")
    args = parser.parse_args()
    if args.host == (args.join is not None):
        raise SystemExit("exactly one of --host or --join <ticket>")
    plan = {
        "role": "host" if args.host else {"join": {"host": args.join}},
        "brain": {"random": {"seed": args.seed}},
        "until": until_value(args.until),
        "timeout_s": args.timeout_s,
        "enforced": args.enforced == "true",
        "name": args.name,
    }
    if args.deck:
        if args.deck.startswith("saved:") and plan["enforced"]:
            raise SystemExit(
                "saved:<label> decks seat on free tables only; pass --enforced false"
            )
        plan["deck"] = args.deck
    sys.stdout.write(json.dumps(plan, separators=(",", ":")))


if __name__ == "__main__":
    main()
