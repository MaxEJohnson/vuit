class vuit < Formula
  desc "VUIT: Vim User Interface Terminal"
  homepage "https://github.com/MaxEJohnson/vuit"
  url "https://github.com/MaxEJohnson/vuit/releases/download/vuit-v0.1.0.tar.gz"
  sha256 "cdeb8ff5ead50d96841447eb68ab0c55eeba98d012f4e785804cf33950301ba6"
  version "0.1.0"

  depends_on "vim"
  depends_on "fzf"
  depends_on "fd"

  def install
    bin.install "vuit"
  end

  test do
    system "#{bin}/vuit", "--version
  end
end
