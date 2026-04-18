class Martins < Formula
  desc "Terminal workspace manager for AI coding agents"
  homepage "https://github.com/lucasbayma/martins"
  version "0.3.2"
  license "MIT"

  url "https://github.com/lucasbayma/martins/releases/download/v#{version}/martins-aarch64-apple-darwin.tar.gz"
  sha256 "3c3395f8fa1843357bb5eed87916019f0e5efce59d3ab5c95281b8935a2ebb87"

  depends_on :macos
  depends_on "tmux"
  depends_on "git"

  def install
    bin.install "martins"
  end

  test do
    assert_match "martins", shell_output("#{bin}/martins --help 2>&1", 0)
  end
end
