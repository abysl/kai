{
  pkgs,
  lib,
  ...
}: let
  bevyRuntimeLibs = with pkgs; [
    alsa-lib-with-plugins
    libxkbcommon
    udev
    vulkan-loader
    wayland
    xorg.libX11
    xorg.libXcursor
    xorg.libXi
    xorg.libXrandr
  ];

  wasmBindgenCli = pkgs.rustPlatform.buildRustPackage rec {
    pname = "wasm-bindgen-cli";
    version = "0.2.127";
    src = pkgs.fetchCrate {
      inherit pname version;
      hash = "sha256-di+qBAdd7pENLiIB9CoZoab+W5xeDoByMREcCGTSzWo=";
    };
    cargoHash = "sha256-FTv2GZIAQs0ePdIZXIXil7JbZ6kIT05VG6vqC1qNFxQ=";
    doCheck = false;
  };
in {
  cachix.enable = false;

  languages.rust = {
    enable = true;
    channel = "stable";
    components = ["rustc" "cargo" "clippy" "rustfmt" "rust-analyzer" "rust-src"];
    targets = ["wasm32-unknown-unknown"];
  };

  packages = with pkgs;
    [
      cargo-nextest
      wasmBindgenCli
      binaryen
      python3
      librsvg
      pkg-config
      clang
      mold
    ]
    ++ bevyRuntimeLibs;

  env.LD_LIBRARY_PATH = lib.makeLibraryPath bevyRuntimeLibs;
  env.CC_wasm32_unknown_unknown = "${pkgs.llvmPackages.clang-unwrapped}/bin/clang";
  env.AR_wasm32_unknown_unknown = "${pkgs.llvmPackages.llvm}/bin/llvm-ar";

  enterShell = ''
    echo "kai dev env — rust $(rustc --version), $(nproc) cores"
    echo "run: dev (fast, dynamically linked) | run (as CI builds it)"
  '';

  scripts."engine-build".exec = ''
    set -e
    agni="$DEVENV_ROOT/../agni"
    (cd "$agni" && cargo build -p agni-engine-wasm --target wasm32-unknown-unknown --release)
    mkdir -p "$DEVENV_ROOT/assets/engine"
    (cd "$agni" && cargo run --release -p agni-harden -- \
      "''${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release/agni_engine_wasm.wasm" \
      "$DEVENV_ROOT/assets/engine/engine.wasm" --engine)
  '';
  scripts."plugin-build".exec = ''
    set -e
    agni="$DEVENV_ROOT/../agni"
    (cd "$agni" && cargo build -p agni-riftbound-plugin -p agni-mtg-plugin --target wasm32-unknown-unknown --release)
    mkdir -p "$DEVENV_ROOT/assets/plugins"
    (cd "$agni" && cargo run --release -p agni-harden -- \
      "''${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release/riftbound_plugin.wasm" \
      "$DEVENV_ROOT/assets/plugins/riftbound.wasm")
    (cd "$agni" && cargo run --release -p agni-harden -- \
      "''${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release/mtg_plugin.wasm" \
      "$DEVENV_ROOT/assets/plugins/mtg.wasm")
  '';
  scripts.icons.exec = ''
    set -e
    cd "$DEVENV_ROOT"
    rsvg-convert -w 256 -h 256 assets/icon/kai.svg -o assets/icon/kai-256.png
    layers=$(mktemp -d)
    python3 - "$layers" <<'PY'
    import re, sys
    out = sys.argv[1]
    svg = open("assets/icon/kai.svg").read()
    body = re.search(r'<g clip-path="url\(#round\)">(.*)</g>\s*</svg>', svg, re.S).group(1)
    backdrop = re.search(r'<g id="backdrop">.*?</g>', body, re.S).group(0)
    art = re.search(r'<g id="art">.*</g>', body, re.S).group(0)
    head = svg[:svg.index('<g clip-path="url(#round)">')]
    open(f"{out}/background.svg", "w").write(head + backdrop + "\n</svg>\n")
    open(f"{out}/foreground.svg", "w").write(
        head + '<g transform="translate(102.4 102.4) scale(0.6)">' + art + "</g>\n</svg>\n"
    )
    PY
    for pair in mdpi:48 hdpi:72 xhdpi:96 xxhdpi:144 xxxhdpi:192; do
      density=''${pair%%:*}
      size=''${pair##*:}
      layer=$((size * 9 / 4))
      dir="android/app/src/main/res/mipmap-$density"
      mkdir -p "$dir"
      rsvg-convert -w "$size" -h "$size" assets/icon/kai.svg -o "$dir/ic_launcher.png"
      rsvg-convert -w "$layer" -h "$layer" "$layers/foreground.svg" -o "$dir/ic_launcher_foreground.png"
      rsvg-convert -w "$layer" -h "$layer" "$layers/background.svg" -o "$dir/ic_launcher_background.png"
    done
    rm -rf "$layers"
  '';
  scripts."desktop-entry".exec = ''
    set -e
    cd "$DEVENV_ROOT"
    apps="''${XDG_DATA_HOME:-$HOME/.local/share}/applications"
    icons="''${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor"
    mkdir -p "$apps" "$icons/scalable/apps"
    cp assets/icon/kai.svg "$icons/scalable/apps/kai.svg"
    for size in 16 32 48 64 128 256 512; do
      mkdir -p "$icons/''${size}x''${size}/apps"
      rsvg-convert -w "$size" -h "$size" assets/icon/kai.svg -o "$icons/''${size}x''${size}/apps/kai.png"
    done
    sed "s|^Exec=.*|Exec=$DEVENV_ROOT/target/debug/kai|; s|^Icon=.*|Icon=kai|" linux/kai.desktop > "$apps/kai.desktop"
    gtk-update-icon-cache -q "$icons" 2>/dev/null || true
    echo "installed $apps/kai.desktop"
  '';
  scripts."modules-build".exec = ''
    set -e
    cd "$DEVENV_ROOT"
    agni="$DEVENV_ROOT/../agni"
    stale() {
      target="$1"
      shift
      [ ! -f "$target" ] && return 0
      [ -n "$(find "$@" -name '*.rs' -newer "$target" -print -quit 2>/dev/null)" ]
    }
    if stale assets/engine/engine.wasm "$agni/core/src" "$agni/sim/src" "$agni/engine"; then engine-build; fi
    if stale assets/plugins/riftbound.wasm "$agni/core/src" "$agni/sim/src" "$agni/plugins/sdk/src" "$agni/plugins/riftbound" "$agni/games/riftbound-turns/src" "$agni/games/riftbound/src" \
      || stale assets/plugins/mtg.wasm "$agni/core/src" "$agni/sim/src" "$agni/plugins/sdk/src" "$agni/plugins/mtg" "$agni/games/mtg/src"; then plugin-build; fi
  '';
  scripts.dev.exec = "modules-build && cargo build --bin kai-cli --features headless,fast-compile && cargo run --bin kai --features fast-compile \"$@\"";
  scripts.run.exec = "modules-build && cargo build --bin kai-cli --features headless && cargo run --bin kai \"$@\"";
  scripts.build.exec = "cargo build --workspace --all-targets";
  scripts."unit-test".exec = "cargo test --workspace";
  scripts.clippy.exec = "cargo clippy --workspace --all-targets -- -D warnings";
  scripts."web-clippy".exec = ''
    RUSTFLAGS='--cfg getrandom_backend="wasm_js"' \
      cargo clippy --target wasm32-unknown-unknown --lib --bins -- -D warnings
  '';
  scripts."web-build".exec = ''
    set -e
    RUSTFLAGS='--cfg getrandom_backend="wasm_js"' \
      cargo build --bin kai --target wasm32-unknown-unknown --release
    rm -rf web/dist
    mkdir -p web/dist
    PATH=".wasm-tools/bin:$PATH" wasm-bindgen --target web --no-typescript \
      --out-dir web/dist --out-name hand \
      "''${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release/kai.wasm"
    wasm-opt -Oz --strip-debug --enable-bulk-memory --enable-nontrapping-float-to-int \
      -o web/dist/hand_bg.wasm.opt web/dist/hand_bg.wasm
    mv web/dist/hand_bg.wasm.opt web/dist/hand_bg.wasm
    engine-build
    plugin-build
    cp assets/engine/engine.wasm web/dist/engine.wasm
    cp web/index.html web/dist/
    cp -r assets web/dist/assets
    du -sh web/dist
  '';
  scripts."web-serve".exec = "python3 -m http.server 8123 -d web/dist";
  scripts.connectivity.exec = "\"$DEVENV_ROOT/tests/connectivity/run.sh\" \"$@\"";
  scripts.fmt.exec = "treefmt";
  scripts."fmt-check".exec = "treefmt --fail-on-change";
}
