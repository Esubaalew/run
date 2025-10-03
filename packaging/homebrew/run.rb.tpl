class Run < Formula
  desc "Universal multi-language runner and smart REPL"
  homepage "https://github.com/${REPO}"

  on_macos do
    on_intel do
      url "${MAC_INTEL_URL}"
      sha256 "${MAC_INTEL_SHA}"
    end

    on_arm do
      url "${MAC_ARM_URL}"
      sha256 "${MAC_ARM_SHA}"
    end
  end

  version "${VERSION}"
  license "Apache-2.0"

  def install
    Dir["run-*-apple-darwin-*"].each do |archive|
      Dir.chdir(archive) do
        bin.install "run"
        prefix.install Dir["README.md", "LICENSE"]
      end
    end
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/run --version")
  end
end
