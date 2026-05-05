class Martins < Formula
  desc "Terminal workspace manager for AI coding agents"
  homepage "https://github.com/lucasbayma/martins"
  version "0.9.0"
  license "MIT"

  url "https://github.com/lucasbayma/martins/releases/download/v#{version}/martins-macos-universal"
  sha256 "673e0198d983b0067e4b39418cf4d6c71ce9411d98996d8454a0e91f2ed400d3"

  depends_on :macos
  depends_on "tmux"
  depends_on "git"

  def install
    bin.install "martins-macos-universal" => "martins"
  end

  test do
    assert_match "martins", shell_output("#{bin}/martins --help 2>&1", 0)
  end
end
