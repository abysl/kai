{pkgs, ...}: {
  cachix.enable = false;

  android = {
    enable = true;
    platforms.version = ["34"];
    ndk.enable = true;
    ndk.version = ["26.3.11579264"];
  };

  languages.rust = {
    enable = true;
    channel = "stable";
    components = ["rustc" "cargo"];
    targets = ["aarch64-linux-android" "x86_64-linux-android"];
  };

  packages = with pkgs; [
    cargo-ndk
    gradle
  ];

  enterShell = ''
    echo "kai android env"
  '';

  scripts."android-native".exec = ''
    set -e
    export ANDROID_NDK_HOME=$ANDROID_NDK_ROOT
    cd "$DEVENV_ROOT"
    abis=$(echo "''${KAI_ANDROID_ABIS:-arm64-v8a}" | tr ',' ' ')
    targets=""
    for abi in $abis; do
      targets="$targets -t $abi"
    done
    (cd .. && cargo ndk $targets -P 28 -o android/app/src/main/jniLibs build --release --lib)
    find app/src/main/jniLibs -name '*.so' ! -name libkai.so -delete
    for abi in $abis; do
      "$ANDROID_NDK_ROOT"/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-strip \
        "app/src/main/jniLibs/$abi/libkai.so"
    done
  '';

  scripts."android-build".exec = ''
    set -e
    android-native
    cd "$DEVENV_ROOT"
    gradle --no-daemon -PkaiAbis="''${KAI_ANDROID_ABIS:-arm64-v8a}" assembleDebug
    ls -lh app/build/outputs/apk/debug/app-debug.apk
  '';

  scripts."android-build-emulator".exec = ''
    set -e
    export KAI_ANDROID_ABIS=x86_64
    android-build
  '';

  scripts."android-release".exec = ''
    set -e
    if [ -n "''${KAI_SIGNING_ENV:-}" ] && [ -f "$KAI_SIGNING_ENV" ]; then
      set -a
      . "$KAI_SIGNING_ENV"
      set +a
    fi
    if [ "''${KAI_REQUIRE_SIGNING:-}" = 1 ] && [ -z "''${KAI_KEYSTORE_BASE64:-}" ]; then
      echo "KAI_REQUIRE_SIGNING needs release credentials"
      exit 1
    fi
    if [ -z "''${KAI_KEYSTORE_BASE64:-}" ]; then
      echo "!! KAI_KEYSTORE_BASE64 unset — falling back to the debug key, NOT publishable"
    fi
    android-native
    cd "$DEVENV_ROOT"
    gradle --no-daemon assembleRelease
    out=app/build/outputs/apk/release
    cp "$out/app-release.apk" "$out/$(cat app/build/kai-release-name)"
    ls -lh "$out"/kai-*.apk
  '';
}
