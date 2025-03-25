class vuit < Formula
  desc "VUIT: Vim User Interface Terminal"
  homepage "https://github.com/MaxEJohnson/vuit"
  url "https://github.com/MaxEJohnson/vuit/releases/download/vuit-v0.1.1.tar.gz"
  sha256 "720a29c3f2fa8a556179b9f7732719ea0e761afc2a6471c8845b63f8749e03b3"
  version "0.1.1"

  depends_on "vim"
  depends_on "fzf"
  depends_on "fd"

  def install
    bin.install "vuit"
  end

  test do
    system "#{bin}/vuit", "--version"
  end
end
