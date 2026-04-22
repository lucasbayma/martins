class Martins < Formula
  desc "Terminal workspace manager for AI coding agents"
  homepage "https://github.com/lucasbayma/martins"
  version "0.7.0"
  license "MIT"

  url "https://github.com/lucasbayma/martins/releases/download/v#{version}/martins-macos-universal"
  sha256 "4d0a78210728d83edfedc51ce3b4fca14bb4f1c4bb962113a58aea6715b099b8"

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
