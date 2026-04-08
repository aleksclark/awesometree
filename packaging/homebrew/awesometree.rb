class Awesometree < Formula
  desc "Workspace manager: window management + Zed + git worktrees"
  homepage "https://github.com/aleksclark/awesometree"
  version "2026.4.8"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/aleksclark/awesometree/releases/download/v#{version}/awesometree-#{version}-macos-arm64.tar.gz"
      sha256 "PLACEHOLDER"
    else
      url "https://github.com/aleksclark/awesometree/releases/download/v#{version}/awesometree-#{version}-macos-x86_64.tar.gz"
      sha256 "PLACEHOLDER"
    end
  end

  on_linux do
    url "https://github.com/aleksclark/awesometree/releases/download/v#{version}/awesometree-#{version}-linux-x86_64.tar.gz"
    sha256 "PLACEHOLDER"
  end

  def install
    bin.install "awesometree"
    bin.install "awesometree-daemon"

    if OS.mac?
      (prefix/"com.awesometree.daemon.plist").install "com.awesometree.daemon.plist"
    else
      (prefix/"awesometree-daemon.service").install "awesometree-daemon.service"
    end
  end

  def caveats
    if OS.mac?
      <<~EOS
        To start the daemon as a launchd service:
          cp #{prefix}/com.awesometree.daemon.plist ~/Library/LaunchAgents/
          launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.awesometree.daemon.plist
      EOS
    else
      <<~EOS
        To start the daemon as a systemd service:
          cp #{prefix}/awesometree-daemon.service ~/.config/systemd/user/
          systemctl --user daemon-reload
          systemctl --user enable --now awesometree-daemon
      EOS
    end
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/awesometree --version", 2)
  end
end
