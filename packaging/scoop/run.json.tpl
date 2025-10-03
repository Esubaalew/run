{
  "version": "${VERSION}",
  "description": "Universal multi-language runner and smart REPL",
  "homepage": "https://github.com/${REPO}",
  "license": "Apache-2.0",
  "architecture": {
    "64bit": {
      "url": "${WINDOWS_URL}",
      "hash": "sha256:${WINDOWS_SHA}"
    }
  },
  "bin": [
    "run.exe"
  ],
  "checkver": "github",
  "autoupdate": {
    "architecture": {
      "64bit": {
        "url": "https://github.com/${REPO}/releases/download/v$version/${WINDOWS_ASSET}"
      }
    }
  }
}
