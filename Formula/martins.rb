class Martins < Formula
  desc "Terminal workspace manager for AI coding agents"
  homepage "https://github.com/lucasbayma/martins"
  version "0.3.1"
  license "MIT"

  url "https://github.com/lucasbayma/martins/releases/download/v#{version}/martins-aarch64-apple-darwin.tar.gz"
  sha256 "d0af768f3f846915d6e35fab727373b66cb0126ad9f7bce066bcc4cf48713897"

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
