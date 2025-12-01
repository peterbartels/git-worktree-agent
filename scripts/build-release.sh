#!/bin/bash
#
# Build release binaries for all supported platforms
#
# Requirements:
# - cross (cargo install cross)
# - Docker (for cross-compilation)
#

set -e

VERSION=$(grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)
BIN_NAME="gwm"

echo "Building git-worktree-manager v${VERSION}"
echo "======================================="

# Create output directory
mkdir -p dist

# Build for each target
TARGETS=(
    "x86_64-unknown-linux-gnu"
    "aarch64-unknown-linux-gnu"
    "x86_64-apple-darwin"
    "aarch64-apple-darwin"
    "x86_64-pc-windows-gnu"
)

for target in "${TARGETS[@]}"; do
    echo ""
    echo "Building for ${target}..."
    
    if [[ "$target" == *"apple"* ]]; then
        # For macOS, need native toolchain or special setup
        if [[ "$(uname)" == "Darwin" ]]; then
            cargo build --release --target "$target"
        else
            echo "  Skipping macOS target on non-macOS host"
            continue
        fi
    else
        cross build --release --target "$target"
    fi
    
    # Copy binary to dist
    if [[ "$target" == *"windows"* ]]; then
        cp "target/${target}/release/${BIN_NAME}.exe" "dist/${BIN_NAME}-${target}.exe"
    else
        cp "target/${target}/release/${BIN_NAME}" "dist/${BIN_NAME}-${target}"
    fi
    
    echo "  Built: dist/${BIN_NAME}-${target}"
done

# Create platform-specific npm packages
echo ""
echo "Creating npm packages..."

create_npm_package() {
    local platform=$1
    local arch=$2
    local target=$3
    local binary_suffix=$4
    
    local pkg_name="@gwm/${platform}-${arch}"
    local pkg_dir="dist/npm/${platform}-${arch}"
    
    mkdir -p "$pkg_dir"
    
    # Copy binary
    if [[ -f "dist/${BIN_NAME}-${target}${binary_suffix}" ]]; then
        cp "dist/${BIN_NAME}-${target}${binary_suffix}" "$pkg_dir/${BIN_NAME}${binary_suffix}"
        chmod +x "$pkg_dir/${BIN_NAME}${binary_suffix}"
    else
        echo "  Warning: Binary not found for ${target}"
        return
    fi
    
    # Create package.json
    cat > "$pkg_dir/package.json" << EOF
{
  "name": "${pkg_name}",
  "version": "${VERSION}",
  "description": "Platform-specific binary for git-worktree-manager (${platform}-${arch})",
  "os": ["${platform}"],
  "cpu": ["${arch}"],
  "main": "${BIN_NAME}${binary_suffix}",
  "files": ["${BIN_NAME}${binary_suffix}"],
  "license": "MIT"
}
EOF
    
    echo "  Created: ${pkg_name}"
}

create_npm_package "linux" "x64" "x86_64-unknown-linux-gnu" ""
create_npm_package "linux" "arm64" "aarch64-unknown-linux-gnu" ""
create_npm_package "darwin" "x64" "x86_64-apple-darwin" ""
create_npm_package "darwin" "arm64" "aarch64-apple-darwin" ""
create_npm_package "win32" "x64" "x86_64-pc-windows-gnu" ".exe"

echo ""
echo "Build complete!"
echo ""
echo "To publish:"
echo "  1. Publish platform packages:"
echo "     cd dist/npm/linux-x64 && npm publish --access public"
echo "     cd dist/npm/linux-arm64 && npm publish --access public"
echo "     # etc."
echo "  2. Publish main package:"
echo "     npm publish --access public"

