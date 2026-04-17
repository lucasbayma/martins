class Martins < Formula
  desc "TUI for managing AI agent teams via git worktrees"
  homepage "https://github.com/bayma/martins"
  version "0.1.0"

  url "https://github.com/bayma/martins/releases/download/v#{version}/martins-macos-universal"
  sha256 "0000000000000000000000000000000000000000000000000000000000000000"

  def install
    bin.install "martins-macos-universal" => "martins"
  end

  test do
    assert_match "martins #{version}", shell_output("#{bin}/martins --version 2>&1 || true")
  end
end
