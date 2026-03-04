export TARGET="armv7-unknown-linux-gnueabihf"

# CC/CXX env vars from the SDK contain embedded flags (e.g. "-mthumb -mfpu=neon ...").
# Cargo/rustc split RUSTFLAGS and CARGO_TARGET_*_LINKER on whitespace, so we must
# wrap the compiler in a single-path wrapper script to avoid misinterpretation.
CC_BIN=$(echo "$CC" | awk '{print $1}')
CXX_BIN=$(echo "$CXX" | awk '{print $1}')
# Flags embedded in CC/CXX (everything after the binary name)
CC_EXTRA=$(echo "$CC" | cut -d' ' -f2-)
CXX_EXTRA=$(echo "$CXX" | cut -d' ' -f2-)

# Create a linker wrapper so cargo can reference it as a single path
LINKER_WRAPPER=$(mktemp /tmp/armv7-linker-XXXXXX.sh)
cat > "$LINKER_WRAPPER" << EOF
#!/bin/bash
exec "$CXX_BIN" $CXX_EXTRA "\$@"
EOF
chmod +x "$LINKER_WRAPPER"
trap "rm -f '$LINKER_WRAPPER'" EXIT

# Use the wrapper as the linker (single path, no embedded spaces)
export CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER="$LINKER_WRAPPER"

# cc-rs reads these for compiling C/C++ dependencies; pass binary + flags separately
export CC_armv7_unknown_linux_gnueabihf="$CC_BIN"
export CXX_armv7_unknown_linux_gnueabihf="$CXX_BIN"
export CFLAGS_armv7_unknown_linux_gnueabihf="$CC_EXTRA"
export CXXFLAGS_armv7_unknown_linux_gnueabihf="$CXX_EXTRA"

# No RUSTFLAGS linker override needed — CARGO_TARGET_*_LINKER handles it
unset RUSTFLAGS

#skia doesnt build

cargo build --target $TARGET --release --bin printerdemo --features slint/backend-winit,slint/renderer-femtovg
