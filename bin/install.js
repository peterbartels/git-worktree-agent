#!/usr/bin/env node

/**
 * Post-install script for git-worktree-agent
 * 
 * Copies the correct platform binary to bin/gwa-binary
 */

const fs = require("fs");
const path = require("path");

// Skip in CI
if (process.env.CI || process.env.GWA_SKIP_INSTALL) {
  process.exit(0);
}

const PLATFORM_MAP = {
  "darwin-x64": "gwa-darwin-x64",
  "darwin-arm64": "gwa-darwin-arm64",
  "linux-x64": "gwa-linux-x64",
  "linux-arm64": "gwa-linux-arm64",
  "win32-x64": "gwa-win32-x64.exe",
};

const platform = process.platform;
const arch = process.arch;
const key = `${platform}-${arch}`;

const binaryName = PLATFORM_MAP[key];
if (!binaryName) {
  console.error(`Unsupported platform: ${platform}-${arch}`);
  console.error("You can install via cargo: cargo install git-worktree-agent");
  process.exit(1);
}

const binariesDir = path.join(__dirname, "..", "binaries");
const sourcePath = path.join(binariesDir, binaryName);
const targetName = platform === "win32" ? "gwa-binary.exe" : "gwa-binary";
const targetPath = path.join(__dirname, targetName);

if (!fs.existsSync(sourcePath)) {
  console.error(`Binary not found: ${sourcePath}`);
  console.error("You can install via cargo: cargo install git-worktree-agent");
  process.exit(1);
}

try {
  fs.copyFileSync(sourcePath, targetPath);
  if (platform !== "win32") {
    fs.chmodSync(targetPath, 0o755);
  }
  console.log(`Installed gwa for ${platform}-${arch}`);
} catch (err) {
  console.error(`Failed to install binary: ${err.message}`);
  process.exit(1);
}

