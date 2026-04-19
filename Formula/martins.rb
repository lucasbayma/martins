class Martins < Formula
  desc "Terminal workspace manager for AI coding agents"
  homepage "https://github.com/lucasbayma/martins"
  version "0.3.3"
  license "MIT"

  url "https://github.com/lucasbayma/martins/releases/download/v#{version}/martins-aarch64-apple-darwin.tar.gz"
  sha256 "0728998241b79ce84ee20526e70b478df88d5cf3ac8826f80a00609eb7b00ef8"

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
