class Martins < Formula
  desc "Terminal workspace manager for AI coding agents"
  homepage "https://github.com/lucasbayma/martins"
  version "0.8.0"
  license "MIT"

  url "https://github.com/lucasbayma/martins/releases/download/v#{version}/martins-macos-universal"
  sha256 "23e1e6394f80fb6eb6713f429acd3be5404a621a658e35e2bfa58ec1919cffac"

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
