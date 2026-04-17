class Martins < Formula
  desc "TUI for managing AI agent teams via git worktrees"
  homepage "https://github.com/bayma/martins"
  version "0.1.0"

  on_macos do
    url "https://github.com/bayma/martins/releases/download/v#{version}/martins-macos-universal"
    sha256 "0000000000000000000000000000000000000000000000000000000000000000"
  end

  on_linux do
    url "https://github.com/bayma/martins/releases/download/v#{version}/martins-linux-x86_64"
    sha256 "0000000000000000000000000000000000000000000000000000000000000000"
  end

  def install
    bin.install "martins-macos-universal" => "martins" if OS.mac?
    bin.install "martins-linux-x86_64" => "martins" if OS.linux?
  end

  test do
    assert_match "martins #{version}", shell_output("#{bin}/martins --version 2>&1 || true")
  end
end
