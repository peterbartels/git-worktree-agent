#!/usr/bin/env node

/**
 * Post-install script for git-worktree-agent
 *
 * This script downloads and installs the appropriate binary for the current platform.
 * It's run automatically after `npm install`.
 */

const fs = require("fs");
const path = require("path");
const { execSync } = require("child_process");
const https = require("https");

const PACKAGE_NAME = "git-worktree-agent";
const BIN_NAME = "gwa";
const VERSION = require("../package.json").version;

// Platform mappings
const PLATFORMS = {
  darwin: {
    x64: "darwin-x64",
    arm64: "darwin-arm64",
  },
  linux: {
    x64: "linux-x64",
    arm64: "linux-arm64",
  },
  win32: {
    x64: "win32-x64",
  },
};

function getPlatformPackage() {
  const platform = process.platform;
  const arch = process.arch;

  const platformMap = PLATFORMS[platform];
  if (!platformMap) {
    console.error(`Unsupported platform: ${platform}`);
    process.exit(1);
  }

  const variant = platformMap[arch];
  if (!variant) {
    console.error(`Unsupported architecture: ${arch} on ${platform}`);
    process.exit(1);
  }

  return `@gwa/${variant}`;
}

function getBinaryPath() {
  const binDir = path.join(__dirname, "..", "bin");
  const binName = process.platform === "win32" ? `${BIN_NAME}.exe` : BIN_NAME;
  return path.join(binDir, binName);
}

function ensureBinDir() {
  const binDir = path.join(__dirname, "..", "bin");
  if (!fs.existsSync(binDir)) {
    fs.mkdirSync(binDir, { recursive: true });
  }
  return binDir;
}

async function downloadBinary() {
  const platformPackage = getPlatformPackage();
  const binaryPath = getBinaryPath();

  console.log(`Installing ${PACKAGE_NAME} for ${process.platform}-${process.arch}...`);

  // Try to find the platform-specific package
  try {
    const platformPkgPath = require.resolve(`${platformPackage}/package.json`);
    const platformPkgDir = path.dirname(platformPkgPath);
    const platformPkg = require(platformPkgPath);

    // Copy binary from platform package
    const sourceBinary = path.join(
      platformPkgDir,
      process.platform === "win32" ? `${BIN_NAME}.exe` : BIN_NAME
    );

    if (fs.existsSync(sourceBinary)) {
      ensureBinDir();
      fs.copyFileSync(sourceBinary, binaryPath);
      if (process.platform !== "win32") {
        fs.chmodSync(binaryPath, 0o755);
      }
      console.log(`Successfully installed ${BIN_NAME}`);
      return;
    }
  } catch (e) {
    // Platform package not found, try to download
  }

  // Fallback: try to download from GitHub releases
  const releaseUrl = `https://github.com/peterbartels/git-worktree-agent/releases/download/v${VERSION}`;
  const binaryName =
    process.platform === "win32"
      ? `${BIN_NAME}-${process.platform}-${process.arch}.exe`
      : `${BIN_NAME}-${process.platform}-${process.arch}`;
  const downloadUrl = `${releaseUrl}/${binaryName}`;

  console.log(`Downloading from ${downloadUrl}...`);

  try {
    await downloadFile(downloadUrl, binaryPath);
    if (process.platform !== "win32") {
      fs.chmodSync(binaryPath, 0o755);
    }
    console.log(`Successfully installed ${BIN_NAME}`);
  } catch (e) {
    console.error(`Failed to download binary: ${e.message}`);
    console.error("");
    console.error("You can try installing manually:");
    console.error("  cargo install git-worktree-agent");
    console.error("");
    console.error("Or download from:");
    console.error(`  ${releaseUrl}`);
    process.exit(1);
  }
}

function downloadFile(url, dest) {
  return new Promise((resolve, reject) => {
    const file = fs.createWriteStream(dest);
    https
      .get(url, (response) => {
        if (response.statusCode === 302 || response.statusCode === 301) {
          // Follow redirect
          downloadFile(response.headers.location, dest)
            .then(resolve)
            .catch(reject);
          return;
        }
        if (response.statusCode !== 200) {
          reject(new Error(`HTTP ${response.statusCode}`));
          return;
        }
        response.pipe(file);
        file.on("finish", () => {
          file.close();
          resolve();
        });
      })
      .on("error", (err) => {
        fs.unlink(dest, () => {});
        reject(err);
      });
  });
}

// Run installation
downloadBinary().catch((e) => {
  console.error(e);
  process.exit(1);
});

