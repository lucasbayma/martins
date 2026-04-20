class Martins < Formula
  desc "Terminal workspace manager for AI coding agents"
  homepage "https://github.com/lucasbayma/martins"
  version "0.6.0"
  license "MIT"

  url "https://github.com/lucasbayma/martins/releases/download/v#{version}/martins-macos-universal"
  sha256 "db11946abe869d9f2b609e69f9da62234aff2e1bec80dc88caec2cfca583baa3"

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
