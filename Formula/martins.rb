class Martins < Formula
  desc "Terminal workspace manager for AI coding agents"
  homepage "https://github.com/lucasbayma/martins"
  version "0.4.0"
  license "MIT"

  url "https://github.com/lucasbayma/martins/releases/download/v#{version}/martins-aarch64-apple-darwin.tar.gz"
  sha256 "f55a8b73c9e793e5e35257c77f9c0fc8cb49e0506df3578cbdd7c8e6ad646554"

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
