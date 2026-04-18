class Martins < Formula
  desc "Terminal workspace manager for AI coding agents"
  homepage "https://github.com/lucasbayma/martins"
  version "0.2.0"
  license "MIT"

  url "https://github.com/lucasbayma/martins/releases/download/v#{version}/martins-aarch64-apple-darwin.tar.gz"
  sha256 "84570c01e4647d54f0e71ebc7d139e9955e071ea39f2f6f932fecf3507f5e8ca"

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
