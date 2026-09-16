import pathlib
import tomllib

lockfile = pathlib.Path(__file__).resolve().parent.parent / "Cargo.lock"
with lockfile.open("rb") as stream:
    packages = tomllib.load(stream)["package"]
package = next(package for package in packages if package["name"] == "agni-core")
source = package["source"]
if not source.startswith("git+https://github.com/abysl/agni"):
    raise SystemExit("agni-core must come from the public Agni repository")
revision = source.rsplit("#", 1)[1]
if len(revision) != 40 or any(char not in "0123456789abcdef" for char in revision):
    raise SystemExit("Cargo.lock does not contain a full Agni commit hash")
print(revision)
