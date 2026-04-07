#!/usr/bin/env bash
# bootstrap_macos.sh — Set up macOS development environment for Re-Flora
#
# Usage:
#   ./tools/bootstrap_macos.sh          # Install deps + configure environment
#   source ./tools/bootstrap_macos.sh   # Also export vars into current shell
#
# Prerequisites: Homebrew (https://brew.sh)

set -euo pipefail

echo "=== Re-Flora macOS Bootstrap ==="

# ── Install dependencies via Homebrew ──────────────────────────────────────────

BREW_PACKAGES=(
    vulkan-headers
    vulkan-loader
    vulkan-validationlayers
    molten-vk
    shaderc
    cmake
)

echo "Installing Homebrew packages..."
for pkg in "${BREW_PACKAGES[@]}"; do
    if brew list --formula "$pkg" &>/dev/null; then
        echo "  ✓ $pkg (already installed)"
    else
        echo "  Installing $pkg..."
        brew install "$pkg"
    fi
done

# ── Configure environment variables ───────────────────────────────────────────

SHELL_RC=""
if [ -f "$HOME/.zshrc" ]; then
    SHELL_RC="$HOME/.zshrc"
elif [ -f "$HOME/.bashrc" ]; then
    SHELL_RC="$HOME/.bashrc"
fi

ENV_BLOCK='# Re-Flora Vulkan environment (macOS)
export DYLD_LIBRARY_PATH="/opt/homebrew/lib:/opt/homebrew/opt/vulkan-loader/lib${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
export DYLD_FALLBACK_LIBRARY_PATH="/opt/homebrew/lib:/opt/homebrew/opt/vulkan-loader/lib${DYLD_FALLBACK_LIBRARY_PATH:+:$DYLD_FALLBACK_LIBRARY_PATH}"
export VK_ICD_FILENAMES="/opt/homebrew/etc/vulkan/icd.d/MoltenVK_icd.json"
export VK_LAYER_PATH="/opt/homebrew/opt/vulkan-validationlayers/share/vulkan/explicit_layer.d"
export SHADERC_LIB_DIR="/opt/homebrew/lib"
export LIBRARY_PATH="/opt/homebrew/lib:/opt/homebrew/opt/vulkan-loader/lib${LIBRARY_PATH:+:$LIBRARY_PATH}"
export CMAKE_POLICY_VERSION_MINIMUM="3.5"'

# Export for the current shell session
eval "$ENV_BLOCK"

# Persist to shell rc if not already present
if [ -n "$SHELL_RC" ]; then
    if ! grep -q "Re-Flora Vulkan environment" "$SHELL_RC" 2>/dev/null; then
        echo "" >> "$SHELL_RC"
        echo "$ENV_BLOCK" >> "$SHELL_RC"
        echo ""
        echo "Environment variables added to $SHELL_RC"
    else
        echo ""
        echo "Environment variables already present in $SHELL_RC"
    fi
else
    echo ""
    echo "Warning: Could not find .zshrc or .bashrc"
    echo "Add the following to your shell profile manually:"
    echo ""
    echo "$ENV_BLOCK"
fi

# ── Verify ────────────────────────────────────────────────────────────────────

echo ""
echo "=== Verification ==="

if command -v cargo &>/dev/null; then
    echo "  ✓ cargo $(cargo --version | awk '{print $2}')"
else
    echo "  ✗ cargo not found — install Rust via https://rustup.rs"
fi

if [ -f "$VK_ICD_FILENAMES" ]; then
    echo "  ✓ MoltenVK ICD found"
else
    echo "  ✗ MoltenVK ICD not found at $VK_ICD_FILENAMES"
fi

if [ -d "$VK_LAYER_PATH" ]; then
    echo "  ✓ Vulkan validation layers found"
else
    echo "  ⚠ Validation layers not found (build with --features no_validation_layer)"
fi

echo ""
echo "Bootstrap complete! Run 'cargo build --release' to build."
echo "If this is a new shell, run: source $SHELL_RC"
